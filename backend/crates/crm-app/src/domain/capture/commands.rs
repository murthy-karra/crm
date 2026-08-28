//! The unmatched held-queue mutations (docs/specs/SLICE_009.md §8):
//! `link_unmatched` and `dismiss_unmatched`. Both are member-self,
//! attributed-agent-only — `store::lock_for_transition` scopes the row to
//! (organization, agent) so a foreign row is indistinguishable from a
//! nonexistent one (D-042.3: 404 for others, admins included).
//!
//! Transition matrix (spec §4.4/§8): `held -> linked` and `held ->
//! dismissed` succeed; `linked -> linked` with the SAME Person is an
//! idempotent no-op, with a DIFFERENT Person is `Conflict` (409);
//! `dismissed -> dismissed` is an idempotent no-op; the two remaining
//! cross-terminal cases (`linked -> dismissed`, `dismissed -> linked`)
//! are also `Conflict`, symmetrically — a terminal row never silently
//! re-enters the other terminal state.
//!
//! "Same Person" for the idempotent re-link check is derived, not stored:
//! `capture_message` carries no memory of which Person a prior link
//! targeted, but a held row's `correspondence_raw_id` can have AT MOST
//! one `correspondence_captured` row (the live pipeline never creates
//! fact rows for a raw that reached the held queue — that is what "held"
//! means), so "was this already linked to Person X" is exactly "does a
//! `correspondence_captured` row exist for this `correspondence_raw_id`
//! naming Person X".

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::RawPayloadKey;
use crate::domain::capture::store::HeldStatus;
use crate::domain::capture::{pipeline, store};
use crate::domain::contact;
use crate::domain::person::queries as person_queries;
use crate::domain::raw_payload::crypto;
use crate::ids::{CaptureMessageId, CorrelationId, OrganizationId, PersonId, UserId};
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

#[derive(Debug)]
pub enum CaptureCommandError {
    /// No such row for this (organization, agent) pair — 404, admins
    /// included (D-042.3).
    NotFound,
    /// A terminal-state conflict: re-link with a different Person,
    /// link-after-dismissed, or dismiss-after-linked — 409.
    Conflict,
    Crypto,
    /// A held row's own invariants didn't hold at link time (missing raw
    /// row, unparseable raw, or a NULL `direction_hint` — all should be
    /// unreachable given every held row is created by the pipeline with
    /// these already populated; a read path fails closed rather than
    /// panicking, docs/specs/SLICE_009.md's house style).
    Corrupt,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for CaptureCommandError {
    fn from(err: sqlx::Error) -> Self {
        CaptureCommandError::Database(err)
    }
}

impl CaptureCommandError {
    pub fn kind(&self) -> &'static str {
        match self {
            CaptureCommandError::NotFound => "not_found",
            CaptureCommandError::Conflict => "conflict",
            CaptureCommandError::Crypto => "crypto",
            CaptureCommandError::Corrupt => "corrupt",
            CaptureCommandError::Database(_) => "database",
        }
    }
}

pub struct LinkUnmatched {
    pub id: CaptureMessageId,
    pub person_id: PersonId,
    pub add_contact_method: bool,
}

/// `POST /api/capture/unmatched/{id}/link` (docs/specs/SLICE_009.md §8).
/// Direction is DETERMINISTIC — read directly from the held row's own
/// `direction_hint`, never re-derived from current state.
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %organization_id,
        agent_id = %agent_user_id,
        capture_message_id = %cmd.id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn link_unmatched(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    organization_id: OrganizationId,
    agent_user_id: UserId,
    cmd: LinkUnmatched,
) -> Result<(), CaptureCommandError> {
    let result =
        link_unmatched_attempt(pool, key, publisher, organization_id, agent_user_id, cmd).await;
    match &result {
        Ok(()) => tracing::Span::current().record("outcome", "linked"),
        Err(err) => tracing::Span::current().record("outcome", err.kind()),
    };
    result
}

async fn link_unmatched_attempt(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    organization_id: OrganizationId,
    agent_user_id: UserId,
    cmd: LinkUnmatched,
) -> Result<(), CaptureCommandError> {
    let mut tx = pool.begin().await?;

    let held = store::lock_for_transition(&mut tx, cmd.id, organization_id, agent_user_id)
        .await?
        .ok_or(CaptureCommandError::NotFound)?;

    match held.status {
        HeldStatus::Dismissed => return Err(CaptureCommandError::Conflict),
        HeldStatus::Linked => {
            let existing_person: Option<Uuid> = sqlx::query_scalar!(
                r#"SELECT person_id FROM correspondence_captured
                   WHERE organization_id = $1 AND correspondence_raw_id = $2"#,
                organization_id.0,
                held.correspondence_raw_id.0,
            )
            .fetch_optional(&mut *tx)
            .await?;
            return match existing_person {
                Some(pid) if pid == cmd.person_id.0 => Ok(()), // idempotent no-op
                _ => Err(CaptureCommandError::Conflict),
            };
        }
        HeldStatus::Held => {}
    }

    // Tenant guard (adversarial H1): the request-supplied person_id must
    // be a Person of THIS organization — `correspondence_captured` and
    // `contact_attempted` carry no person FK, and the Today reads join by
    // person id, so an unchecked cross-org id here would be a permanent
    // cross-tenant write (arming/clearing another org's Today). Same
    // pattern as every other person-writing command
    // (`log_contact_attempt`/`assign_person`: lock, then NotFound).
    person_queries::lock_person(&mut tx, cmd.person_id, organization_id)
        .await?
        .ok_or(CaptureCommandError::NotFound)?;

    let (received_at, nonce, ciphertext) =
        store::read_for_link(&mut tx, held.correspondence_raw_id, organization_id)
            .await?
            .ok_or(CaptureCommandError::Corrupt)?;
    let raw = crypto::open_correspondence(
        key,
        organization_id,
        held.correspondence_raw_id,
        &nonce,
        &ciphertext,
    )
    .map_err(|_| CaptureCommandError::Crypto)?;
    // A held row was, by construction, parseable at capture time (an
    // unparseable raw never reaches the held queue — see
    // `domain/capture/receive.rs::process_attempt`), so failure here
    // would be a genuine data-integrity surprise, not an expected path.
    let meta = pipeline::parse_metadata(&raw, received_at).ok_or(CaptureCommandError::Corrupt)?;
    let direction = held.direction_hint.ok_or(CaptureCommandError::Corrupt)?;

    let correlation_id = CorrelationId::new(Uuid::new_v4());
    let inserted = pipeline::insert_fact_and_maybe_attempt(
        &mut tx,
        organization_id,
        agent_user_id,
        cmd.person_id,
        direction,
        &meta,
        held.correspondence_raw_id,
        correlation_id,
    )
    .await?;

    if cmd.add_contact_method {
        if let Some(email) = held.counterparty_email.as_deref() {
            if let Some(normalized) = contact::normalize_email(email) {
                sqlx::query!(
                    r#"INSERT INTO contact_method (organization_id, person_id, kind, value, normalized_value)
                       VALUES ($1, $2, 'email', $3, $4)
                       ON CONFLICT (person_id, kind, normalized_value) DO NOTHING"#,
                    organization_id.0,
                    cmd.person_id.0,
                    email,
                    normalized.as_str(),
                )
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    store::mark_linked(&mut tx, cmd.id, organization_id).await?;
    tx.commit().await?;

    if inserted.is_some() {
        let event = RealtimeEvent::person_changed(
            organization_id,
            meta.occurred_at,
            correlation_id,
            cmd.person_id,
            PersonChange::CorrespondenceCaptured,
        );
        publisher
            .publish_after_commit(Publication::for_event(event))
            .await;
    }

    Ok(())
}

/// `POST /api/capture/unmatched/{id}/dismiss` (docs/specs/SLICE_009.md
/// §8): idempotent; `dismissed -> dismissed` is a no-op, `linked ->
/// dismissed` is `Conflict`.
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %organization_id,
        agent_id = %agent_user_id,
        capture_message_id = %id,
        outcome = tracing::field::Empty,
    )
)]
pub async fn dismiss_unmatched(
    pool: &PgPool,
    organization_id: OrganizationId,
    agent_user_id: UserId,
    id: CaptureMessageId,
) -> Result<(), CaptureCommandError> {
    let mut tx = pool.begin().await?;

    let held = store::lock_for_transition(&mut tx, id, organization_id, agent_user_id)
        .await?
        .ok_or(CaptureCommandError::NotFound)?;

    let span = tracing::Span::current();
    match held.status {
        HeldStatus::Dismissed => {
            span.record("outcome", "already_dismissed");
            return Ok(());
        }
        HeldStatus::Linked => {
            span.record("outcome", "conflict");
            return Err(CaptureCommandError::Conflict);
        }
        HeldStatus::Held => {}
    }

    store::mark_dismissed(&mut tx, id, organization_id).await?;
    tx.commit().await?;
    span.record("outcome", "dismissed");
    Ok(())
}
