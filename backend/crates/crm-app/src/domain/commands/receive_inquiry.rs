//! `ReceiveInquiry` — the two-phase intake command (docs/specs/SLICE_002.md
//! §3, §4).

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use rand::RngExt;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::config::RawPayloadKey;
use crate::domain::admin::queries as admin_queries;
use crate::domain::commands::CommandError;
use crate::domain::contact;
use crate::domain::facts::{
    self, AssignmentChangedFact, InquiryReceivedFact, RoutingDecisionFact, StageChangedFact,
};
use crate::domain::inquiry::parse::{self, ParsedLead, Source, UnresolvedReason};
use crate::domain::inquiry::queries as inquiry_queries;
use crate::domain::intake::IntakeActor;
use crate::domain::person::queries as person_queries;
use crate::domain::raw_payload::{crypto, store, PayloadFormat, Resolution};
use crate::domain::stage;
use crate::ids::{OrganizationId, UserId};
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

pub struct ReceiveInquiry {
    pub source: Source,
    pub payload: Vec<u8>,
    pub assign_to_user_id: Option<UserId>,
    pub received_at: DateTime<Utc>,
}

/// Total wall-clock time `receive_inquiry` spends retrying the
/// per-Organization advisory lock before giving up with `IntakeBusy`.
/// Chosen to be comfortably longer than several legitimate Phase-B
/// attempts ever need (each is a handful of small, indexed queries —
/// realistically low single-digit milliseconds), while still failing fast
/// and predictably rather than parking a connection indefinitely under
/// real contention. "Low single-digit seconds" per the chosen trade-off;
/// 3s leaves room for roughly a dozen-plus retries at the backoff schedule
/// below without ever looking "hung" from a client's perspective.
///
/// `pub`, not just an implementation detail: the adversarial test in
/// tests/db_intake.rs holds an Organization's lock externally for longer
/// than this budget and needs the real value, not a duplicated constant
/// that could silently drift out of sync.
pub const ADVISORY_LOCK_BUDGET: Duration = Duration::from_secs(3);
/// First retry's base backoff (jitter is added on top — see the retry loop
/// below), short enough that normal contention (two or three concurrent
/// intakes for the same Organization) resolves in one or two retries.
const ADVISORY_LOCK_INITIAL_BACKOFF_MS: u64 = 25;
/// Backoff doubles each retry up to this cap, so a sustained burst settles
/// into a steady retry rate rather than growing unbounded.
const ADVISORY_LOCK_MAX_BACKOFF_MS: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    Explicit,
    ActorDefault,
    KeptExisting,
    /// System-actor intake, default assignee set and an active member
    /// (docs/specs/SLICE_007c.md §4). Declared additive extension of the
    /// frozen `POST /api/inquiries` `routing_strategy` vocabulary
    /// (SLICE_002 §5) — reachable there only via a `duplicate: true`
    /// replay of a system-routed row.
    OrganizationDefault,
    /// System-actor intake, no default set or the configured default is
    /// not currently an active member — `assignee_user_id` NULL
    /// (docs/specs/SLICE_007c.md §4). Same declared additive extension.
    Unassigned,
}

impl RoutingStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            RoutingStrategy::Explicit => "explicit",
            RoutingStrategy::ActorDefault => "actor_default",
            RoutingStrategy::KeptExisting => "kept_existing",
            RoutingStrategy::OrganizationDefault => "organization_default",
            RoutingStrategy::Unassigned => "unassigned",
        }
    }

    /// Fails closed (docs/specs/SLICE_002.md §9's failure-behavior spirit)
    /// rather than panicking on an unrecognized value: `strategy` is
    /// `CHECK`-constrained at the database, so this should be unreachable
    /// in practice, but a read path must never crash the process on
    /// unexpected data.
    fn from_str(s: &str) -> Result<Self, CommandError> {
        match s {
            "explicit" => Ok(RoutingStrategy::Explicit),
            "actor_default" => Ok(RoutingStrategy::ActorDefault),
            "kept_existing" => Ok(RoutingStrategy::KeptExisting),
            "organization_default" => Ok(RoutingStrategy::OrganizationDefault),
            "unassigned" => Ok(RoutingStrategy::Unassigned),
            _ => Err(CommandError::Corrupt),
        }
    }
}

pub enum ReceiveInquiryOutcome {
    Resolved {
        inquiry_id: Uuid,
        person_id: Uuid,
        person_created: bool,
        routing_strategy: RoutingStrategy,
        assigned_user_id: Option<UserId>,
        duplicate: bool,
    },
    Unresolved {
        raw_payload_id: Uuid,
        reason: UnresolvedReason,
        duplicate: bool,
    },
}

/// The two same-role assignee ids `determine_routing` compares
/// (docs/design/type-safety-hardening.md, N2 residual): a shared `UserId`
/// newtype can't stop `assign_to_user_id` and `current_assignee` from being
/// transposed positionally, so they're named fields instead.
struct RoutingAssignees {
    /// The request's explicit assignee, if any.
    assign_to_user_id: Option<UserId>,
    /// The Person's assignee *before* this command runs — always `None`
    /// for a brand-new Person.
    current_assignee: Option<UserId>,
}

/// `assignees` -> `(strategy, assignee)` (docs/specs/SLICE_002.md §3;
/// routing matrix extended by docs/specs/SLICE_007c.md §4). A Person that
/// already has an assignee always keeps it (`kept_existing`), regardless of
/// what the request asked for; otherwise an explicit request wins;
/// otherwise a User actor's own id (`actor_default`); otherwise (a System
/// actor) the Organization's configured default, re-checked for active
/// membership inside this same Phase-B transaction (`organization_default`
/// if set and active, `unassigned` — NULL assignee — otherwise). Applies
/// uniformly to a brand-new Person (`current_assignee` always `None`) and a
/// matched one.
async fn determine_routing(
    tx: &mut PgConnection,
    actor: &IntakeActor,
    assignees: RoutingAssignees,
) -> Result<(RoutingStrategy, Option<UserId>), CommandError> {
    if let Some(existing) = assignees.current_assignee {
        return Ok((RoutingStrategy::KeptExisting, Some(existing)));
    }
    if let Some(explicit) = assignees.assign_to_user_id {
        return Ok((RoutingStrategy::Explicit, Some(explicit)));
    }
    if let Some(actor_user_id) = actor.user_actor_id() {
        return Ok((RoutingStrategy::ActorDefault, Some(actor_user_id)));
    }
    let default =
        admin_queries::active_intake_default_assignee(tx, actor.organization_id()).await?;
    Ok(match default {
        Some(user_id) => (RoutingStrategy::OrganizationDefault, Some(user_id)),
        None => (RoutingStrategy::Unassigned, None),
    })
}

/// Public entry point: keeps the `#[instrument]` span (and its "outcome"
/// field) live for the whole attempt, and — since every fallible step below
/// returns through one `Result` here — is the single place that logs *why*
/// a command failed. Every prior version of this function recorded
/// `outcome` only on success ("resolved"/"unresolved"/"duplicate"); a
/// `CommandError` return left zero server-side signal beyond the HTTP
/// status code. `err.kind()` is a stable, static tag — never the error's
/// `Display`/`Debug` text, to preserve the no-plaintext-leak property
/// (docs/specs/SLICE_002.md §8).
#[tracing::instrument(
    skip_all,
    fields(
        organization_id = %actor.organization_id(),
        actor_kind = actor.actor_kind().as_str(),
        actor_id = tracing::field::Empty,
        correlation_id = %actor.correlation_id(),
        source = cmd.source.as_str(),
        raw_payload_id = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn receive_inquiry(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    actor: &IntakeActor,
    cmd: ReceiveInquiry,
) -> Result<ReceiveInquiryOutcome, CommandError> {
    if let Some(actor_user_id) = actor.user_actor_id() {
        tracing::Span::current().record("actor_id", tracing::field::display(actor_user_id));
    }
    let result = receive_inquiry_attempt(pool, key, publisher, actor, cmd).await;
    // The `outcome` record lives with each caller, derived from the
    // returned outcome, not inside `complete_intake`
    // (docs/specs/SLICE_007d.md §4c): the email caller's span uses a
    // different vocabulary, and re-recording inside the shared function
    // would double-stamp fields across the two spans.
    match &result {
        Ok(outcome) => {
            let label = match outcome {
                ReceiveInquiryOutcome::Resolved {
                    duplicate: true, ..
                }
                | ReceiveInquiryOutcome::Unresolved {
                    duplicate: true, ..
                } => "duplicate",
                ReceiveInquiryOutcome::Resolved { .. } => "resolved",
                ReceiveInquiryOutcome::Unresolved { .. } => "unresolved",
            };
            tracing::Span::current().record("outcome", label);
        }
        Err(err) => {
            tracing::warn!(error_kind = err.kind(), "receive_inquiry failed");
            tracing::Span::current().record("outcome", err.kind());
        }
    }
    result
}

async fn receive_inquiry_attempt(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    actor: &IntakeActor,
    cmd: ReceiveInquiry,
) -> Result<ReceiveInquiryOutcome, CommandError> {
    let organization_id = actor.organization_id();

    // Assignee validity is checked before anything is stored, so a
    // rejected request leaves no `pending` row (docs/specs/SLICE_002.md §3,
    // §9; acceptance criterion 12).
    if let Some(assignee) = cmd.assign_to_user_id {
        let mut conn = pool.acquire().await?;
        let is_member =
            person_queries::is_organization_member(&mut conn, organization_id, assignee).await?;
        if !is_member {
            return Err(CommandError::InvalidAssignee);
        }
    }

    // --- Phase A: store the encrypted payload before parsing (D-015 §4) ---
    let content_hmac = crypto::content_hmac(key, &cmd.payload);
    let candidate_id = Uuid::new_v4();
    let sealed = crypto::seal(key, organization_id, candidate_id, &cmd.payload)
        .map_err(|_| CommandError::Crypto)?;

    let raw_payload_id = store::insert_pending(
        pool,
        candidate_id,
        organization_id,
        &cmd.source,
        PayloadFormat::GenericV1,
        actor.origin(),
        cmd.received_at,
        &sealed.nonce,
        &sealed.ciphertext,
        &content_hmac,
        cmd.payload.len() as i32,
    )
    .await?;
    tracing::Span::current().record("raw_payload_id", tracing::field::display(raw_payload_id));

    // --- Phase B: shared with the email path (docs/specs/SLICE_007d.md
    // §4c). The closure re-states this caller's parse: `generic_v1` JSON
    // plus the request's own validated `source`.
    let source = cmd.source.clone();
    complete_intake(
        pool,
        key,
        publisher,
        actor,
        CompleteIntake {
            raw_payload_id,
            content_hmac: &content_hmac,
            received_at: cmd.received_at,
            assign_to_user_id: cmd.assign_to_user_id,
        },
        move |bytes| parse::parse(bytes).map(|parsed| (source.clone(), parsed)),
    )
    .await
}

/// Phase-B parameters shared by both intake entry points
/// (docs/specs/SLICE_007d.md §4c). `raw_payload_id` is the *stored* row's
/// id from Phase A (which differs from the candidate on a duplicate);
/// `content_hmac` is only threaded into the `inquiry_received` fact.
pub(crate) struct CompleteIntake<'a> {
    pub raw_payload_id: Uuid,
    pub content_hmac: &'a [u8],
    pub received_at: DateTime<Utc>,
    pub assign_to_user_id: Option<UserId>,
}

/// Everything after Phase A, extracted verbatim from `receive_inquiry`
/// (docs/specs/SLICE_007d.md §4c) and shared by `POST /api/inquiries` and
/// the inbound-email path: row lock → duplicate short-circuit → decrypt →
/// parse (the closure) → per-Organization advisory lock with bounded
/// retry → identify → Person/contact methods → routing → Inquiry → facts
/// → `mark_resolved` → one `person_changed` publish; parse failure →
/// `mark_unresolved` + `intake_unresolved_changed`. Callers record their
/// own span `outcome` from the returned value — this function records
/// none.
pub(crate) async fn complete_intake<F>(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    actor: &IntakeActor,
    params: CompleteIntake<'_>,
    parse_payload: F,
) -> Result<ReceiveInquiryOutcome, CommandError>
where
    F: Fn(&[u8]) -> Result<(Source, ParsedLead), UnresolvedReason>,
{
    let organization_id = actor.organization_id();
    let correlation_id = actor.correlation_id();
    let raw_payload_id = params.raw_payload_id;

    // --- Phase B: bounded retry around the per-Organization advisory lock
    // ---------------------------------------------------------------------
    // Each iteration is a fresh, complete transaction attempt: acquire a
    // connection, re-take the row lock, and (if the row is still pending)
    // try the advisory lock *non-blocking*. A blocking
    // `pg_advisory_xact_lock` here would hold a checked-out pool connection
    // for however long another request holds this Organization's lock —
    // that's the mechanism behind cross-tenant pool starvation (one
    // Organization's intake burst parking connections that every other
    // Organization's logins/queries also need). `pg_try_advisory_xact_lock`
    // never blocks: on failure we roll back (releasing the connection
    // immediately, not held while waiting), back off briefly, and retry —
    // so a stuck or contended Organization can only ever cost this request
    // its own bounded wait, never someone else's connection.
    //
    // Duplicate-delivery detection and parse-failure handling are
    // unchanged and, notably, never need the advisory lock at all — they
    // short-circuit (commit and return) before it's ever attempted, exactly
    // as before this change, so a duplicate re-POST isn't subject to the
    // retry/backoff even when the Organization is contended.
    let advisory_lock_deadline = Instant::now() + ADVISORY_LOCK_BUDGET;
    let mut backoff_ms = ADVISORY_LOCK_INITIAL_BACKOFF_MS;

    loop {
        let mut tx = pool.begin().await?;
        let locked = store::lock_for_processing(&mut tx, raw_payload_id, organization_id)
            .await?
            .ok_or(CommandError::Database(sqlx::Error::RowNotFound))?;

        if locked.resolution != Resolution::Pending {
            let outcome = duplicate_outcome(&mut tx, organization_id, locked).await?;
            tx.commit().await?;
            return Ok(outcome);
        }

        // The AAD was computed against `raw_payload_id` (the *stored*
        // row's id, which equals `candidate_id` unless a concurrent
        // delivery won the ON CONFLICT race) — decrypt the stored row, not
        // the request's own freshly-sealed copy (docs/specs/SLICE_002.md
        // §3, §14a).
        let plaintext = crypto::open(
            key,
            organization_id,
            raw_payload_id,
            &locked.nonce,
            &locked.ciphertext,
        )
        .map_err(|_| CommandError::Crypto)?;

        let (source, parsed) = match parse_payload(&plaintext) {
            Ok(parsed) => parsed,
            Err(reason) => {
                store::mark_unresolved(&mut tx, locked.id, organization_id, reason.as_str())
                    .await?;
                tx.commit().await?;

                // occurred_at = raw_payload.received_at (the value Phase A
                // stored it with); there is no fact for this outcome
                // (docs/specs/SLICE_003.md §4).
                let event = RealtimeEvent::intake_unresolved_changed(
                    organization_id,
                    params.received_at,
                    correlation_id,
                    locked.id,
                );
                publisher
                    .publish_after_commit(Publication::for_event(event))
                    .await;

                return Ok(ReceiveInquiryOutcome::Unresolved {
                    raw_payload_id: locked.id,
                    reason,
                    duplicate: false,
                });
            }
        };

        // Per-Organization intake lock (docs/specs/SLICE_002.md §3):
        // serializes identify + routing + writes across concurrent intakes
        // for this Organization so two different first-contacts sharing a
        // contact value cannot both create a Person. `$1` binds as text
        // (the `::text` cast in the expression makes Postgres infer a text
        // parameter — spec §14a's "the ::text cast is required"), so the
        // id is formatted in Rust rather than relying on an implicit
        // uuid->text coercion.
        let organization_id_text = organization_id.to_string();
        let lock_attempt = sqlx::query!(
            r#"SELECT pg_try_advisory_xact_lock(hashtextextended('intake:' || $1::text, 0)) as "acquired!""#,
            organization_id_text,
        )
        .fetch_one(&mut *tx)
        .await?;

        if !lock_attempt.acquired {
            // Not held: release the connection immediately rather than
            // waiting on it, then back off and retry from scratch.
            tx.rollback().await?;

            if Instant::now() >= advisory_lock_deadline {
                return Err(CommandError::IntakeBusy);
            }

            let jitter_ms = rand::rng().random_range(0..=(backoff_ms / 2).max(1));
            tokio::time::sleep(Duration::from_millis(backoff_ms + jitter_ms)).await;
            backoff_ms = (backoff_ms * 2).min(ADVISORY_LOCK_MAX_BACKOFF_MS);
            continue;
        }

        // Lock held for the rest of this transaction (advisory xact locks
        // release automatically on commit/rollback) — proceed exactly as
        // before.
        let identify_match = contact::identify(
            &mut tx,
            organization_id,
            parsed.email.as_deref(),
            parsed.phone.as_deref(),
        )
        .await?;

        // person_id, person_created, matched_by, current_assignee (the
        // Person's assignee *before* this command runs — None for a
        // brand-new Person).
        let (person_id, person_created, matched_by, current_assignee) = match identify_match {
            Some(m) => {
                let person = person_queries::lock_person(&mut tx, m.person_id, organization_id)
                    .await?
                    .ok_or(CommandError::Database(sqlx::Error::RowNotFound))?;
                person_queries::upsert_contact_methods(
                    &mut tx,
                    person.id,
                    organization_id,
                    &parsed,
                )
                .await?;
                (
                    person.id,
                    false,
                    Some(m.matched_by.as_str()),
                    person.assigned_user_id,
                )
            }
            None => {
                let first_stage_id = stage::first_id(&mut tx, organization_id)
                    .await?
                    .ok_or(CommandError::NoStagesConfigured)?;
                let new_person_id = person_queries::insert_person(
                    &mut tx,
                    organization_id,
                    parsed.first_name.as_deref(),
                    parsed.last_name.as_deref(),
                    first_stage_id,
                    None,
                )
                .await?;
                person_queries::upsert_contact_methods(
                    &mut tx,
                    new_person_id,
                    organization_id,
                    &parsed,
                )
                .await?;
                (new_person_id, true, None, None)
            }
        };

        let (routing_strategy, routing_assignee) = determine_routing(
            &mut tx,
            actor,
            RoutingAssignees {
                assign_to_user_id: params.assign_to_user_id,
                current_assignee,
            },
        )
        .await?;

        // A repeat lead must not leave a Person ownerless: whenever there
        // was no prior assignee (new Person, or a matched Person nobody
        // owned yet), persist the routing outcome onto the Person row.
        // `kept_existing` (current_assignee.is_some()) leaves the row
        // untouched.
        if current_assignee.is_none() {
            person_queries::update_assignment(
                &mut tx,
                person_id,
                organization_id,
                routing_assignee,
            )
            .await?;
        }

        // `source` is the closure's *detected* source (the request's own
        // for `generic_v1`; e.g. `website` for a pinned email format) —
        // `inquiry.source` takes it while `raw_payload.source` keeps the
        // transport value Phase A stored (docs/specs/SLICE_007d.md §4c).
        let new_inquiry_id = inquiry_queries::insert(
            &mut tx,
            inquiry_queries::NewInquiry {
                organization_id,
                person_id,
                raw_payload_id: locked.id,
                source: source.as_str(),
                source_external_id: parsed.external_id.as_deref(),
                message: parsed.message.as_deref(),
                received_at: params.received_at,
            },
        )
        .await?;

        let envelope = actor.envelope(params.received_at);

        facts::insert_inquiry_received(
            &mut tx,
            &envelope,
            InquiryReceivedFact {
                inquiry_id: new_inquiry_id,
                person_id,
                raw_payload_id: locked.id,
                content_hmac: params.content_hmac,
                source: source.as_str(),
                person_created,
                matched_by,
            },
        )
        .await?;

        let routing_decision_id = facts::insert_routing_decision(
            &mut tx,
            &envelope,
            RoutingDecisionFact {
                inquiry_id: new_inquiry_id,
                person_id,
                strategy: routing_strategy.as_str(),
                assignee_user_id: routing_assignee,
            },
        )
        .await?;

        // Only written when the Person did not already have an assignee
        // (docs/specs/SLICE_002.md §2: "On intake, causation_id = the
        // routing_decision.id") AND the routing outcome actually assigned
        // someone — a system-actor `unassigned` routing (NULL -> NULL)
        // gains no fact here; it would be noise (docs/specs/SLICE_007c.md
        // §4).
        if current_assignee.is_none() && routing_assignee.is_some() {
            facts::insert_assignment_changed(
                &mut tx,
                &envelope.clone().with_causation(routing_decision_id),
                AssignmentChangedFact {
                    person_id,
                    from_user_id: None,
                    to_user_id: routing_assignee,
                    reason: "intake",
                },
            )
            .await?;
        }

        // Only a brand-new Person gets a stage_changed fact on intake —
        // stage is unchanged on a repeat inquiry (spec §14 default 1).
        if person_created {
            let first_stage_id = stage::first_id(&mut tx, organization_id)
                .await?
                .ok_or(CommandError::NoStagesConfigured)?;
            facts::insert_stage_changed(
                &mut tx,
                &envelope,
                StageChangedFact {
                    person_id,
                    from_stage_id: None,
                    to_stage_id: first_stage_id,
                    reason: "intake",
                },
            )
            .await?;
        }

        store::mark_resolved(&mut tx, locked.id, organization_id, new_inquiry_id).await?;
        tx.commit().await?;

        // Exactly one event per command execution, not per fact: a
        // matched-Person intake writes up to three facts (inquiry_received,
        // routing_decision, and sometimes assignment_changed) but publishes
        // one person.changed{inquiry_received} (docs/specs/SLICE_003.md
        // §4). occurred_at = the fact's occurred_at (= received_at),
        // never publish time.
        let event = RealtimeEvent::person_changed(
            organization_id,
            params.received_at,
            correlation_id,
            person_id,
            PersonChange::InquiryReceived,
        );
        publisher
            .publish_after_commit(Publication::for_event(event))
            .await;

        return Ok(ReceiveInquiryOutcome::Resolved {
            inquiry_id: new_inquiry_id,
            person_id,
            person_created,
            routing_strategy,
            assigned_user_id: routing_assignee,
            duplicate: false,
        });
    }
}

/// `pub(crate)`: the workbench retry's resolved short-circuit reuses it
/// (docs/specs/SLICE_007e.md §4) so the two-admin race returns the stored
/// outcome instead of reprocessing.
pub(crate) async fn duplicate_outcome(
    tx: &mut sqlx::PgConnection,
    organization_id: OrganizationId,
    locked: store::LockedRawPayload,
) -> Result<ReceiveInquiryOutcome, CommandError> {
    match locked.resolution {
        Resolution::Resolved => {
            let inquiry_id = locked
                .inquiry_id
                .ok_or(CommandError::Database(sqlx::Error::RowNotFound))?;
            let lookup =
                store::resolved_outcome_for_inquiry(tx, organization_id, inquiry_id).await?;
            Ok(ReceiveInquiryOutcome::Resolved {
                inquiry_id: lookup.inquiry_id,
                person_id: lookup.person_id,
                person_created: lookup.person_created,
                routing_strategy: RoutingStrategy::from_str(&lookup.strategy)?,
                assigned_user_id: lookup.assigned_user_id,
                duplicate: true,
            })
        }
        // A discarded row (SLICE_007e §3) stays discarded: an admin's
        // explicit decision is not undone by a byte-identical re-send.
        // Same outcome shape as an unresolved duplicate — the callers
        // already map it (accepted-no-publish on /inbound/email; the
        // existing unresolved duplicate envelope on /api/inquiries). A
        // row discarded while still `pending` has a NULL reason — the
        // shared fallback decode below covers it.
        Resolution::Unresolved | Resolution::Discarded => {
            let reason = decode_unresolved_reason(locked.unresolved_reason.as_deref());
            Ok(ReceiveInquiryOutcome::Unresolved {
                raw_payload_id: locked.id,
                reason,
                duplicate: true,
            })
        }
        // Both call sites (above, and workbench.rs's `retry_intake`)
        // only reach `duplicate_outcome` once they've already
        // established the row isn't `pending`; `Resolution` can't
        // encode that exclusion in `locked`'s type, so this arm is the
        // caller-contract violation this match must still handle. Was
        // `unreachable!` — a corrupt/impossible state must yield a
        // 500-class error, not a process panic (hardening chunk S1,
        // docs/design/type-safety-hardening.md).
        Resolution::Pending => Err(CommandError::Corrupt),
    }
}

/// The duplicate-replay reason decode, shared with the workbench's
/// discarded arm (docs/specs/SLICE_007e.md §3). Unknown or NULL values
/// fall back to `no_contact_method` — the pre-existing posture.
pub(crate) fn decode_unresolved_reason(reason: Option<&str>) -> UnresolvedReason {
    match reason.unwrap_or("no_contact_method") {
        "invalid_json" => UnresolvedReason::InvalidJson,
        "not_an_object" => UnresolvedReason::NotAnObject,
        // The email vocabulary (docs/specs/SLICE_007d.md §4c), so a
        // duplicate replay of an email row decodes faithfully.
        "email_unparsed" => UnresolvedReason::EmailUnparsed,
        "email_unrecognized_format" => UnresolvedReason::EmailUnrecognizedFormat,
        // The extraction vocabulary (docs/specs/SLICE_007f.md §6).
        "not_a_lead" => UnresolvedReason::NotALead,
        "email_extraction_failed" => UnresolvedReason::EmailExtractionFailed,
        _ => UnresolvedReason::NoContactMethod,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips every strategy, including the two Slice 007c additions
    /// — the declared additive extension to `RoutingStrategy::from_str`
    /// (docs/specs/SLICE_007c.md §5) that lets a `duplicate: true` replay
    /// of a system-routed row decode without a 500.
    #[test]
    fn routing_strategy_round_trips_every_variant() {
        for strategy in [
            RoutingStrategy::Explicit,
            RoutingStrategy::ActorDefault,
            RoutingStrategy::KeptExisting,
            RoutingStrategy::OrganizationDefault,
            RoutingStrategy::Unassigned,
        ] {
            assert_eq!(
                RoutingStrategy::from_str(strategy.as_str()).unwrap(),
                strategy
            );
        }
    }

    #[test]
    fn decode_unresolved_reason_covers_the_vocabulary_and_falls_back() {
        assert_eq!(
            decode_unresolved_reason(Some("invalid_json")),
            UnresolvedReason::InvalidJson
        );
        assert_eq!(
            decode_unresolved_reason(Some("email_unparsed")),
            UnresolvedReason::EmailUnparsed
        );
        assert_eq!(
            decode_unresolved_reason(Some("email_unrecognized_format")),
            UnresolvedReason::EmailUnrecognizedFormat
        );
        // NULL (a row discarded while pending) and unknown values fall
        // back — the pre-existing posture (docs/specs/SLICE_007e.md §3).
        assert_eq!(
            decode_unresolved_reason(None),
            UnresolvedReason::NoContactMethod
        );
        assert_eq!(
            decode_unresolved_reason(Some("something_new")),
            UnresolvedReason::NoContactMethod
        );
    }

    #[test]
    fn routing_strategy_from_str_fails_closed_on_an_unknown_value() {
        assert!(matches!(
            RoutingStrategy::from_str("bogus"),
            Err(CommandError::Corrupt)
        ));
    }
}
