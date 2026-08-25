//! The Unresolved workbench (docs/specs/SLICE_007e.md §4): admin-only
//! detail (decrypt on demand), Try again (guarded reset + the shared
//! `complete_intake`), and Discard. All three take a `CommandContext`
//! (the acting admin) and are org-scoped by it; the route layer enforces
//! the admin role (D-037).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::RawPayloadKey;
use crate::domain::commands::receive_inquiry::{
    complete_intake, duplicate_outcome, CompleteIntake, ReceiveInquiryOutcome,
};
use crate::domain::commands::CommandError;
use crate::domain::envelope::{CommandContext, Origin};
use crate::domain::inquiry::parse::{self, Source};
use crate::domain::intake::email;
use crate::domain::intake::IntakeActor;
use crate::domain::raw_payload::{crypto, store};
use crate::realtime::{Publication, Publisher, RealtimeEvent};

/// Every text field in a detail response is capped here (raw mail can be
/// ~1.5 MiB; the workbench needs enough to decide, not the whole blob) —
/// docs/specs/SLICE_007e.md §4, safe default (d). Truncation is
/// UTF-8-boundary-safe via the shared `truncate_to_bytes`.
const DETAIL_TEXT_MAX_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub enum WorkbenchError {
    /// Unknown, cross-org, or (for the detail read) terminal — all
    /// byte-identical 404 upstream.
    NotFound,
    /// Retry on a discarded row (409 `discarded`).
    Discarded,
    /// Discard on a resolved row (409 `already_resolved`).
    AlreadyResolved,
    /// Decrypt failure — a corrupted row; Try again will not help,
    /// Discard is the remedy (docs/specs/SLICE_007e.md §4).
    Crypto,
    /// Stored `source`/`payload_format` no longer parse — fail closed.
    Corrupt,
    Command(CommandError),
    Database(sqlx::Error),
}

impl From<sqlx::Error> for WorkbenchError {
    fn from(err: sqlx::Error) -> Self {
        WorkbenchError::Database(err)
    }
}

impl WorkbenchError {
    /// Static tag for span `outcome` fields — never error text
    /// (docs/specs/SLICE_007e.md §9).
    pub fn kind(&self) -> &'static str {
        match self {
            WorkbenchError::NotFound => "not_found",
            WorkbenchError::Discarded => "already_discarded",
            WorkbenchError::AlreadyResolved => "already_resolved",
            WorkbenchError::Crypto => "crypto",
            WorkbenchError::Corrupt => "corrupt",
            WorkbenchError::Command(e) => e.kind(),
            WorkbenchError::Database(_) => "database",
        }
    }
}

pub struct UnresolvedDetail {
    pub id: Uuid,
    pub source: String,
    pub payload_format: String,
    pub received_at: DateTime<Utc>,
    pub resolution: String,
    pub unresolved_reason: Option<String>,
    pub byte_len: i32,
    pub content: UnresolvedContent,
}

pub enum UnresolvedContent {
    Email {
        subject: Option<String>,
        from_display: Option<String>,
        from_addr: Option<String>,
        date: Option<DateTime<Utc>>,
        text: Option<String>,
        truncated: bool,
    },
    Text {
        text: String,
        truncated: bool,
    },
}

/// Never derived: this is the decrypted lead content
/// (docs/specs/SLICE_007e.md §8).
impl std::fmt::Debug for UnresolvedContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnresolvedContent::Email { truncated, .. } => f
                .debug_struct("UnresolvedContent::Email")
                .field("truncated", truncated)
                .finish(),
            UnresolvedContent::Text { truncated, .. } => f
                .debug_struct("UnresolvedContent::Text")
                .field("truncated", truncated)
                .finish(),
        }
    }
}

impl std::fmt::Debug for UnresolvedDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnresolvedDetail")
            .field("id", &self.id)
            .field("payload_format", &self.payload_format)
            .finish()
    }
}

pub enum DiscardOutcome {
    Discarded,
    /// Idempotent repeat — original attribution unchanged (first writer
    /// wins).
    AlreadyDiscarded,
}

/// Caps a text field, tracking whether anything was cut.
fn cap(text: String, truncated: &mut bool) -> String {
    if text.len() > DETAIL_TEXT_MAX_BYTES {
        *truncated = true;
        parse::truncate_to_bytes(&text, DETAIL_TEXT_MAX_BYTES)
    } else {
        text
    }
}

/// Decrypt-on-demand detail (docs/specs/SLICE_007e.md §4): visible only
/// while `pending` or `unresolved`; `resolved`/`discarded`/unknown/
/// cross-org are the same `NotFound`.
pub async fn unresolved_detail(
    pool: &PgPool,
    key: &RawPayloadKey,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<UnresolvedDetail, WorkbenchError> {
    let mut conn = pool.acquire().await?;
    let row = store::unresolved_row_for_detail(&mut conn, id, ctx.organization_id)
        .await?
        .ok_or(WorkbenchError::NotFound)?;
    drop(conn);

    let plaintext = crypto::open(
        key,
        ctx.organization_id,
        row.id,
        &row.nonce,
        &row.ciphertext,
    )
    .map_err(|_| WorkbenchError::Crypto)?;

    let mut truncated = false;
    let content = match row.payload_format.as_str() {
        "rfc822_v1" => match email::mime::parse(&plaintext) {
            Some(mail) => UnresolvedContent::Email {
                subject: mail.subject.map(|s| cap(s, &mut truncated)),
                from_display: mail.from_display.map(|s| cap(s, &mut truncated)),
                from_addr: mail.from_addr.map(|s| cap(s, &mut truncated)),
                date: mail.date,
                text: mail.text_body.map(|s| cap(s, &mut truncated)),
                truncated,
            },
            // An email_unparsed row: nothing email-shaped — show what
            // arrived, lossily.
            None => UnresolvedContent::Text {
                text: cap(
                    String::from_utf8_lossy(&plaintext).into_owned(),
                    &mut truncated,
                ),
                truncated,
            },
        },
        "generic_v1" => {
            let text = match serde_json::from_slice::<serde_json::Value>(&plaintext) {
                Ok(value) => serde_json::to_string_pretty(&value)
                    .unwrap_or_else(|_| String::from_utf8_lossy(&plaintext).into_owned()),
                // An invalid_json row — lossy raw.
                Err(_) => String::from_utf8_lossy(&plaintext).into_owned(),
            };
            UnresolvedContent::Text {
                text: cap(text, &mut truncated),
                truncated,
            }
        }
        // Unknown format: display-only fail-open as raw text (retry fails
        // closed instead — docs/specs/SLICE_007e.md safe default (e)).
        _ => UnresolvedContent::Text {
            text: cap(
                String::from_utf8_lossy(&plaintext).into_owned(),
                &mut truncated,
            ),
            truncated,
        },
    };

    Ok(UnresolvedDetail {
        id: row.id,
        source: row.source,
        payload_format: row.payload_format,
        received_at: row.received_at,
        resolution: row.resolution,
        unresolved_reason: row.unresolved_reason,
        byte_len: row.byte_len,
        content,
    })
}

/// Try again (docs/specs/SLICE_007e.md §4): a guarded reset-to-pending,
/// then the shared `complete_intake` under the System actor with the
/// acting admin recorded as `on_behalf_of_user_id` — the rescued lead
/// routes per D-035 (the org default / unassigned), NOT to the admin.
pub async fn retry_intake(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<ReceiveInquiryOutcome, WorkbenchError> {
    // Step 1: guarded reset in its own transaction.
    let mut tx = pool.begin().await?;
    let locked = store::lock_for_processing(&mut tx, id, ctx.organization_id)
        .await?
        .ok_or(WorkbenchError::NotFound)?;
    let row_source = locked.source.clone();

    match locked.resolution.as_str() {
        // The two-admin race: return the stored outcome, never reprocess.
        "resolved" => {
            let outcome = duplicate_outcome(&mut tx, ctx.organization_id, locked)
                .await
                .map_err(WorkbenchError::Command)?;
            tx.commit().await?;
            return Ok(outcome);
        }
        "discarded" => {
            tx.rollback().await?;
            return Err(WorkbenchError::Discarded);
        }
        // pending (a no-op reset) or unresolved.
        _ => {}
    }

    let meta = sqlx::query!(
        r#"SELECT payload_format, received_at, content_hmac
           FROM raw_payload WHERE id = $1 AND organization_id = $2"#,
        id,
        ctx.organization_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    // The static format string is span-safe (docs/specs/SLICE_007e.md
    // §9); recording is a no-op if the ambient span lacks the field.
    tracing::Span::current().record("payload_format", meta.payload_format.as_str());
    // Fail closed BEFORE mutating: a retry that can never succeed
    // (unknown payload_format, a stored source that no longer validates)
    // must not destroy the stored diagnostic reason (adversarial
    // finding, SLICE_007e verification).
    let retryable = match meta.payload_format.as_str() {
        "rfc822_v1" => true,
        "generic_v1" => Source::parse(&row_source).is_some(),
        _ => false,
    };
    if !retryable {
        tx.rollback().await?;
        return Err(WorkbenchError::Corrupt);
    }
    // The reset also re-arms LLM extraction (docs/specs/SLICE_007f.md
    // §4b): an explicit human Try-again clears the attempt counters, so
    // a terminal not_a_lead / email_extraction_failed row that lands
    // back at email_unrecognized_format becomes eligible again.
    sqlx::query!(
        r#"UPDATE raw_payload
           SET resolution = 'pending', unresolved_reason = NULL, resolved_at = NULL,
               extraction_attempts = 0, extraction_next_attempt_at = NULL
           WHERE id = $1 AND organization_id = $2"#,
        id,
        ctx.organization_id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // Step 2: the shared completion path. Fresh correlation id (this is a
    // new unattended execution); the admin lives in on_behalf_of.
    let actor = IntakeActor::System {
        organization_id: ctx.organization_id,
        origin: Origin::WebSession,
        correlation_id: Uuid::new_v4(),
        on_behalf_of_user_id: Some(ctx.actor_user_id),
    };
    let params = CompleteIntake {
        raw_payload_id: id,
        content_hmac: &meta.content_hmac,
        received_at: meta.received_at,
        assign_to_user_id: None,
    };

    let result = match meta.payload_format.as_str() {
        "rfc822_v1" => {
            complete_intake(pool, key, publisher, &actor, params, email::parse_payload).await
        }
        "generic_v1" => {
            // Already validated above; the ok_or is unreachable belt.
            let source = Source::parse(&row_source).ok_or(WorkbenchError::Corrupt)?;
            complete_intake(pool, key, publisher, &actor, params, move |bytes| {
                parse::parse(bytes).map(|parsed| (source.clone(), parsed))
            })
            .await
        }
        _ => return Err(WorkbenchError::Corrupt),
    };
    match result {
        Ok(outcome) => Ok(outcome),
        Err(err) => {
            // The reset already committed (the row is now `pending`,
            // reason cleared) and this error path publishes nothing from
            // `complete_intake` — without an event, every connected
            // client keeps showing the stale "Unresolved / <reason>" row
            // (adversarial finding; mirrors 007d's IntakeBusy publish).
            // Ids-only, best-effort.
            let event = RealtimeEvent::intake_unresolved_changed(
                ctx.organization_id,
                Utc::now(),
                Uuid::new_v4(),
                id,
            );
            publisher
                .publish_after_commit(Publication::for_event(event))
                .await;
            Err(WorkbenchError::Command(err))
        }
    }
}

/// Discard (docs/specs/SLICE_007e.md §4): explicit, attributed,
/// idempotent; not deletion — ciphertext retained until the erasure
/// runbook / O-013.
pub async fn discard_raw_payload(
    pool: &PgPool,
    publisher: &Publisher,
    ctx: &CommandContext,
    id: Uuid,
) -> Result<DiscardOutcome, WorkbenchError> {
    let discarded_at = Utc::now();

    let mut tx = pool.begin().await?;
    let locked = store::lock_for_processing(&mut tx, id, ctx.organization_id)
        .await?
        .ok_or(WorkbenchError::NotFound)?;

    match locked.resolution.as_str() {
        "discarded" => {
            // Idempotent repeat; original attribution unchanged.
            tx.rollback().await?;
            return Ok(DiscardOutcome::AlreadyDiscarded);
        }
        "resolved" => {
            tx.rollback().await?;
            return Err(WorkbenchError::AlreadyResolved);
        }
        _ => {}
    }

    sqlx::query!(
        r#"UPDATE raw_payload
           SET resolution = 'discarded', discarded_by_user_id = $3, discarded_at = $4
           WHERE id = $1 AND organization_id = $2"#,
        id,
        ctx.organization_id,
        ctx.actor_user_id,
        discarded_at,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    // The row must leave every member's queue live (ids-only, fresh
    // correlation id; occurred_at = the discard time).
    let event = RealtimeEvent::intake_unresolved_changed(
        ctx.organization_id,
        discarded_at,
        Uuid::new_v4(),
        id,
    );
    publisher
        .publish_after_commit(Publication::for_event(event))
        .await;

    Ok(DiscardOutcome::Discarded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_is_utf8_boundary_safe_and_tracks_truncation() {
        let mut truncated = false;
        // A multi-byte char straddling the cap must not panic.
        let long = "é".repeat(DETAIL_TEXT_MAX_BYTES); // 2 bytes each
        let capped = cap(long, &mut truncated);
        assert!(truncated);
        assert!(capped.len() <= DETAIL_TEXT_MAX_BYTES);
        assert!(capped.chars().all(|c| c == 'é'));

        let mut untruncated = false;
        assert_eq!(cap("short".into(), &mut untruncated), "short");
        assert!(!untruncated);
    }

    #[test]
    fn content_and_detail_debug_never_print_content() {
        let content = UnresolvedContent::Email {
            subject: Some("SECRET SUBJECT".into()),
            from_display: Some("Secret Sender".into()),
            from_addr: Some("secret@example.com".into()),
            date: None,
            text: Some("secret body".into()),
            truncated: false,
        };
        let debug = format!("{content:?}");
        assert!(!debug.contains("SECRET"));
        assert!(!debug.contains("secret"));

        let text = UnresolvedContent::Text {
            text: "secret json".into(),
            truncated: true,
        };
        let debug = format!("{text:?}");
        assert!(!debug.contains("secret"));
        assert!(debug.contains("truncated: true"));
    }
}
