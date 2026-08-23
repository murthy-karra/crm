//! `ReceiveInquiry` — the two-phase intake command (docs/specs/SLICE_002.md
//! §3, §4).

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use rand::RngExt;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::RawPayloadKey;
use crate::domain::commands::CommandError;
use crate::domain::contact;
use crate::domain::envelope::{CommandContext, FactEnvelope};
use crate::domain::facts::{
    self, AssignmentChangedFact, InquiryReceivedFact, RoutingDecisionFact, StageChangedFact,
};
use crate::domain::inquiry::parse::{self, Source, UnresolvedReason};
use crate::domain::inquiry::queries as inquiry_queries;
use crate::domain::person::queries as person_queries;
use crate::domain::raw_payload::{crypto, store};
use crate::domain::stage;
use crate::realtime::{PersonChange, Publication, Publisher, RealtimeEvent};

pub struct ReceiveInquiry {
    pub source: Source,
    pub payload: Vec<u8>,
    pub assign_to_user_id: Option<Uuid>,
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
}

impl RoutingStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            RoutingStrategy::Explicit => "explicit",
            RoutingStrategy::ActorDefault => "actor_default",
            RoutingStrategy::KeptExisting => "kept_existing",
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
        assigned_user_id: Option<Uuid>,
        duplicate: bool,
    },
    Unresolved {
        raw_payload_id: Uuid,
        reason: UnresolvedReason,
        duplicate: bool,
    },
}

/// `assign_to_user_id`/`current_assignee` -> `(strategy, assignee)`
/// (docs/specs/SLICE_002.md §3). A Person that already has an assignee
/// always keeps it (`kept_existing`), regardless of what the request asked
/// for; otherwise an explicit request wins, else the actor. Applies
/// uniformly to a brand-new Person (`current_assignee` always `None`) and a
/// matched one.
fn determine_routing(
    assign_to_user_id: Option<Uuid>,
    actor_user_id: Uuid,
    current_assignee: Option<Uuid>,
) -> (RoutingStrategy, Option<Uuid>) {
    if let Some(existing) = current_assignee {
        return (RoutingStrategy::KeptExisting, Some(existing));
    }
    match assign_to_user_id {
        Some(explicit) => (RoutingStrategy::Explicit, Some(explicit)),
        None => (RoutingStrategy::ActorDefault, Some(actor_user_id)),
    }
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
        organization_id = %ctx.organization_id,
        actor_id = %ctx.actor_user_id,
        correlation_id = %ctx.correlation_id,
        source = cmd.source.as_str(),
        raw_payload_id = tracing::field::Empty,
        outcome = tracing::field::Empty,
    )
)]
pub async fn receive_inquiry(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: ReceiveInquiry,
) -> Result<ReceiveInquiryOutcome, CommandError> {
    let result = receive_inquiry_attempt(pool, key, publisher, ctx, cmd).await;
    if let Err(ref err) = result {
        tracing::warn!(error_kind = err.kind(), "receive_inquiry failed");
        tracing::Span::current().record("outcome", err.kind());
    }
    result
}

async fn receive_inquiry_attempt(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    ctx: &CommandContext,
    cmd: ReceiveInquiry,
) -> Result<ReceiveInquiryOutcome, CommandError> {
    // Assignee validity is checked before anything is stored, so a
    // rejected request leaves no `pending` row (docs/specs/SLICE_002.md §3,
    // §9; acceptance criterion 12).
    if let Some(assignee) = cmd.assign_to_user_id {
        let mut conn = pool.acquire().await?;
        let is_member =
            person_queries::is_organization_member(&mut conn, ctx.organization_id, assignee)
                .await?;
        if !is_member {
            return Err(CommandError::InvalidAssignee);
        }
    }

    // --- Phase A: store the encrypted payload before parsing (D-015 §4) ---
    let content_hmac = crypto::content_hmac(key, &cmd.payload);
    let candidate_id = Uuid::new_v4();
    let sealed = crypto::seal(key, ctx.organization_id, candidate_id, &cmd.payload)
        .map_err(|_| CommandError::Crypto)?;

    let raw_payload_id = store::insert_pending(
        pool,
        candidate_id,
        ctx.organization_id,
        cmd.source.as_str(),
        "generic_v1",
        ctx.origin.as_str(),
        cmd.received_at,
        &sealed.nonce,
        &sealed.ciphertext,
        &content_hmac,
        cmd.payload.len() as i32,
    )
    .await?;
    tracing::Span::current().record("raw_payload_id", tracing::field::display(raw_payload_id));

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
        let locked = store::lock_for_processing(&mut tx, raw_payload_id, ctx.organization_id)
            .await?
            .ok_or(CommandError::Database(sqlx::Error::RowNotFound))?;

        if locked.resolution != "pending" {
            let outcome = duplicate_outcome(&mut tx, ctx.organization_id, locked).await?;
            tx.commit().await?;
            tracing::Span::current().record("outcome", "duplicate");
            return Ok(outcome);
        }

        // The AAD was computed against `raw_payload_id` (the *stored*
        // row's id, which equals `candidate_id` unless a concurrent
        // delivery won the ON CONFLICT race) — decrypt the stored row, not
        // the request's own freshly-sealed copy (docs/specs/SLICE_002.md
        // §3, §14a).
        let plaintext = crypto::open(
            key,
            ctx.organization_id,
            raw_payload_id,
            &locked.nonce,
            &locked.ciphertext,
        )
        .map_err(|_| CommandError::Crypto)?;

        let parsed = match parse::parse(&plaintext) {
            Ok(parsed) => parsed,
            Err(reason) => {
                store::mark_unresolved(&mut tx, locked.id, ctx.organization_id, reason.as_str())
                    .await?;
                tx.commit().await?;
                tracing::Span::current().record("outcome", "unresolved");

                // occurred_at = raw_payload.received_at (= cmd.received_at,
                // the value Phase A stored it with); there is no fact for
                // this outcome (docs/specs/SLICE_003.md §4).
                let event = RealtimeEvent::intake_unresolved_changed(
                    ctx.organization_id,
                    cmd.received_at,
                    ctx.correlation_id,
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
        let organization_id_text = ctx.organization_id.to_string();
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
            ctx.organization_id,
            parsed.email.as_deref(),
            parsed.phone.as_deref(),
        )
        .await?;

        // person_id, person_created, matched_by, current_assignee (the
        // Person's assignee *before* this command runs — None for a
        // brand-new Person).
        let (person_id, person_created, matched_by, current_assignee) = match identify_match {
            Some(m) => {
                let person = person_queries::lock_person(&mut tx, m.person_id, ctx.organization_id)
                    .await?
                    .ok_or(CommandError::Database(sqlx::Error::RowNotFound))?;
                person_queries::upsert_contact_methods(
                    &mut tx,
                    person.id,
                    ctx.organization_id,
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
                let first_stage_id = stage::first_id(&mut tx, ctx.organization_id)
                    .await?
                    .ok_or(CommandError::NoStagesConfigured)?;
                let new_person_id = person_queries::insert_person(
                    &mut tx,
                    ctx.organization_id,
                    parsed.first_name.as_deref(),
                    parsed.last_name.as_deref(),
                    first_stage_id,
                    None,
                )
                .await?;
                person_queries::upsert_contact_methods(
                    &mut tx,
                    new_person_id,
                    ctx.organization_id,
                    &parsed,
                )
                .await?;
                (new_person_id, true, None, None)
            }
        };

        let (routing_strategy, routing_assignee) =
            determine_routing(cmd.assign_to_user_id, ctx.actor_user_id, current_assignee);

        // A repeat lead must not leave a Person ownerless: whenever there
        // was no prior assignee (new Person, or a matched Person nobody
        // owned yet), persist the routing outcome onto the Person row.
        // `kept_existing` (current_assignee.is_some()) leaves the row
        // untouched.
        if current_assignee.is_none() {
            person_queries::update_assignment(
                &mut tx,
                person_id,
                ctx.organization_id,
                routing_assignee,
            )
            .await?;
        }

        let new_inquiry_id = inquiry_queries::insert(
            &mut tx,
            inquiry_queries::NewInquiry {
                organization_id: ctx.organization_id,
                person_id,
                raw_payload_id: locked.id,
                source: cmd.source.as_str(),
                source_external_id: parsed.external_id.as_deref(),
                message: parsed.message.as_deref(),
                received_at: cmd.received_at,
            },
        )
        .await?;

        let envelope = FactEnvelope::for_command(ctx, cmd.received_at);

        facts::insert_inquiry_received(
            &mut tx,
            &envelope,
            InquiryReceivedFact {
                inquiry_id: new_inquiry_id,
                person_id,
                raw_payload_id: locked.id,
                content_hmac: &content_hmac,
                source: cmd.source.as_str(),
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
        // routing_decision.id").
        if current_assignee.is_none() {
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
            let first_stage_id = stage::first_id(&mut tx, ctx.organization_id)
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

        store::mark_resolved(&mut tx, locked.id, ctx.organization_id, new_inquiry_id).await?;
        tx.commit().await?;

        tracing::Span::current().record("outcome", "resolved");

        // Exactly one event per command execution, not per fact: a
        // matched-Person intake writes up to three facts (inquiry_received,
        // routing_decision, and sometimes assignment_changed) but publishes
        // one person.changed{inquiry_received} (docs/specs/SLICE_003.md
        // §4). occurred_at = the fact's occurred_at (= cmd.received_at),
        // never publish time.
        let event = RealtimeEvent::person_changed(
            ctx.organization_id,
            cmd.received_at,
            ctx.correlation_id,
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

async fn duplicate_outcome(
    tx: &mut sqlx::PgConnection,
    organization_id: Uuid,
    locked: store::LockedRawPayload,
) -> Result<ReceiveInquiryOutcome, CommandError> {
    match locked.resolution.as_str() {
        "resolved" => {
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
        "unresolved" => {
            let reason_str = locked
                .unresolved_reason
                .as_deref()
                .unwrap_or("no_contact_method");
            let reason = match reason_str {
                "invalid_json" => UnresolvedReason::InvalidJson,
                "not_an_object" => UnresolvedReason::NotAnObject,
                _ => UnresolvedReason::NoContactMethod,
            };
            Ok(ReceiveInquiryOutcome::Unresolved {
                raw_payload_id: locked.id,
                reason,
                duplicate: true,
            })
        }
        other => unreachable!("unknown raw_payload.resolution in database: {other}"),
    }
}
