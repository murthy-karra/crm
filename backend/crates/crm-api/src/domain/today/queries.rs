//! Today candidates query (docs/specs/SLICE_003.md §3, §4). `assigned_user_id
//! = $viewer` is a Today-ownership rule applied inside
//! `PersonVisibilityScope::Organization`, not a new scope variant
//! (AGENTS.md §4.4) — nothing here adds an `AssignedUser` variant to
//! `visibility.rs`.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::commands::{ContactAttemptRef, ContactChannel, ContactOutcome};
use crate::domain::person::model::{compute_display_name, PersonSummary, StageRef, UserRef};
use crate::domain::person::visibility::PersonVisibilityScope;
use crate::domain::today::model::{InquiryRef, TodayCandidate};

struct TodayCandidateRow {
    id: Uuid,
    first_name: Option<String>,
    last_name: Option<String>,
    created_at: DateTime<Utc>,
    stage_id: Uuid,
    stage_name: String,
    assigned_user_id: Option<Uuid>,
    assigned_user_display_name: Option<String>,
    primary_email: Option<String>,
    primary_phone: Option<String>,
    inquiry_count: i64,
    latest_inquiry_id: Option<Uuid>,
    latest_inquiry_source: Option<String>,
    latest_inquiry_received_at: Option<DateTime<Utc>>,
    last_attempt_id: Option<Uuid>,
    last_attempt_channel: Option<String>,
    last_attempt_outcome: Option<String>,
    last_attempt_occurred_at: Option<DateTime<Utc>>,
    waiting_since: Option<DateTime<Utc>>,
    fresh: Option<bool>,
}

/// A read path fails closed on unexpected data rather than panicking
/// (AGENTS.md convention, e.g. `RoutingStrategy::from_str` ->
/// `CommandError::Corrupt`): these columns are guaranteed non-null by the
/// query's own `WHERE waiting.received_at IS NOT NULL` filter, but sqlx's
/// static nullability inference can't see that through the LATERAL joins,
/// so the row struct types them `Option<T>` and this converts, erroring
/// (never panicking) if the invariant is ever violated.
fn required<T>(value: Option<T>, column: &'static str) -> Result<T, sqlx::Error> {
    value.ok_or_else(|| {
        sqlx::Error::Decode(
            format!("today candidates query: expected {column} to be non-null").into(),
        )
    })
}

impl TryFrom<TodayCandidateRow> for TodayCandidate {
    type Error = sqlx::Error;

    fn try_from(row: TodayCandidateRow) -> Result<Self, sqlx::Error> {
        let display_name = compute_display_name(
            row.first_name.as_deref(),
            row.last_name.as_deref(),
            row.primary_email.as_deref(),
            row.primary_phone.as_deref(),
        );
        let assigned_user = match (row.assigned_user_id, row.assigned_user_display_name) {
            (Some(id), Some(display_name)) => Some(UserRef { id, display_name }),
            _ => None,
        };
        let latest_inquiry_received_at =
            required(row.latest_inquiry_received_at, "latest_inquiry.received_at")?;

        let person = PersonSummary {
            id: row.id,
            first_name: row.first_name,
            last_name: row.last_name,
            display_name,
            stage: StageRef {
                id: row.stage_id,
                name: row.stage_name,
            },
            assigned_user,
            primary_email: row.primary_email,
            primary_phone: row.primary_phone,
            inquiry_count: row.inquiry_count,
            last_inquiry_at: Some(latest_inquiry_received_at),
            created_at: row.created_at,
        };

        let latest_inquiry = InquiryRef {
            id: required(row.latest_inquiry_id, "latest_inquiry.id")?,
            source: required(row.latest_inquiry_source, "latest_inquiry.source")?,
            received_at: latest_inquiry_received_at,
        };

        let last_contact_attempt = match (
            row.last_attempt_id,
            row.last_attempt_channel,
            row.last_attempt_outcome,
            row.last_attempt_occurred_at,
        ) {
            (Some(id), Some(channel), Some(outcome), Some(occurred_at)) => {
                let channel = ContactChannel::decode(&channel).ok_or_else(|| {
                    sqlx::Error::Decode(
                        format!("today candidates query: unexpected contact_attempted.channel {channel:?}")
                            .into(),
                    )
                })?;
                let outcome = ContactOutcome::decode(&outcome).ok_or_else(|| {
                    sqlx::Error::Decode(
                        format!("today candidates query: unexpected contact_attempted.outcome {outcome:?}")
                            .into(),
                    )
                })?;
                Some(ContactAttemptRef {
                    id,
                    channel,
                    outcome,
                    occurred_at,
                })
            }
            _ => None,
        };

        Ok(TodayCandidate {
            person,
            latest_inquiry,
            last_contact_attempt,
            waiting_since: required(row.waiting_since, "waiting_since")?,
            inquiry_count: row.inquiry_count,
            fresh: required(row.fresh, "fresh")?,
        })
    }
}

/// Candidates for `viewer`'s Today (docs/specs/SLICE_003.md §3, §4): People
/// assigned to `viewer` in `scope`'s Organization whose latest Inquiry has
/// not yet been answered by a contact attempt at or after it. `fresh` is
/// computed once here, in SQL, from `now`, and used directly in the
/// `ORDER BY` (tier before `LIMIT 201`) so a fresh lead can never fall off
/// the cap behind stale ones. Tie-breaks (`received_at DESC, id DESC` for
/// the latest Inquiry; `occurred_at DESC, id DESC` for the last attempt)
/// are contract, not choice (§14a). `last_contact_attempt` is the
/// **effective** attempt (docs/specs/SLICE_006c.md §2, §3): rows that have
/// a corrector (`corrects_id` chain) are excluded before the tie-break —
/// a correction inherits its original's `occurred_at`, so without the
/// filter the `id DESC` tie-break would pick original or correction at
/// random.
pub async fn candidates(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    viewer: Uuid,
    now: DateTime<Utc>,
) -> Result<(Vec<TodayCandidate>, bool), sqlx::Error> {
    let organization_id = scope.organization_id();

    let mut rows = sqlx::query_as!(
        TodayCandidateRow,
        r#"SELECT
             p.id, p.first_name, p.last_name, p.created_at,
             s.id as stage_id, s.name as stage_name,
             u.id as "assigned_user_id?", u.display_name as "assigned_user_display_name?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'email'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_email?",
             (SELECT cm.value FROM contact_method cm
                WHERE cm.person_id = p.id AND cm.kind = 'phone'
                ORDER BY cm.created_at ASC LIMIT 1) as "primary_phone?",
             (SELECT count(*) FROM inquiry i WHERE i.person_id = p.id) as "inquiry_count!",
             latest.id as "latest_inquiry_id?",
             latest.source as "latest_inquiry_source?",
             latest.received_at as "latest_inquiry_received_at?",
             last_attempt.id as "last_attempt_id?",
             last_attempt.channel as "last_attempt_channel?",
             last_attempt.outcome as "last_attempt_outcome?",
             last_attempt.occurred_at as "last_attempt_occurred_at?",
             waiting.received_at as "waiting_since?",
             (latest.received_at > $3::timestamptz - interval '24 hours') as "fresh?"
           FROM person p
           JOIN stage s ON s.id = p.stage_id
           LEFT JOIN app_user u ON u.id = p.assigned_user_id
           LEFT JOIN LATERAL (
               SELECT i.id, i.source, i.received_at
               FROM inquiry i
               WHERE i.person_id = p.id
               ORDER BY i.received_at DESC, i.id DESC
               LIMIT 1
           ) latest ON true
           LEFT JOIN LATERAL (
               SELECT ca.id, ca.channel, ca.outcome, ca.occurred_at
               FROM contact_attempted ca
               WHERE ca.person_id = p.id
                 AND NOT EXISTS (SELECT 1 FROM contact_attempted c WHERE c.corrects_id = ca.id)
               ORDER BY ca.occurred_at DESC, ca.id DESC
               LIMIT 1
           ) last_attempt ON true
           LEFT JOIN LATERAL (
               SELECT i2.received_at
               FROM inquiry i2
               WHERE i2.person_id = p.id
                 AND i2.received_at > COALESCE(last_attempt.occurred_at, '-infinity'::timestamptz)
               ORDER BY i2.received_at ASC
               LIMIT 1
           ) waiting ON true
           WHERE p.organization_id = $1
             AND p.assigned_user_id = $2
             AND waiting.received_at IS NOT NULL
           ORDER BY "fresh?" DESC, waiting.received_at ASC, p.id ASC
           LIMIT 201"#,
        organization_id,
        viewer,
        now,
    )
    .fetch_all(conn)
    .await?;

    let truncated = rows.len() > 200;
    rows.truncate(200);

    let candidates = rows
        .into_iter()
        .map(TodayCandidate::try_from)
        .collect::<Result<Vec<_>, _>>()?;

    Ok((candidates, truncated))
}
