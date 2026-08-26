//! The extraction worker (docs/specs/SLICE_007f.md §4b): the
//! `telephony/sweep.rs` pattern with exactly one delta — a pass drains
//! every claimable row (bounded by a backstop) instead of taking one item
//! per tick. `run_once` is the unit the DB-backed tests drive directly
//! with a fake extractor.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use sqlx::{PgConnection, PgPool};
use tokio::task::JoinHandle;
use tracing::Instrument;
use uuid::Uuid;

use crate::config::RawPayloadKey;
use crate::domain::commands::receive_inquiry::{
    complete_intake, CompleteIntake, ReceiveInquiryOutcome,
};
use crate::domain::commands::CommandError;
use crate::domain::envelope::Origin;
use crate::domain::intake::email;
use crate::domain::intake::extraction::{
    build_input, validate_reply, ClaimVerdict, ExtractorError, LeadExtractor,
};
use crate::domain::intake::IntakeActor;
use crate::domain::raw_payload::{crypto, store};
use crate::realtime::{Publication, Publisher, RealtimeEvent};

pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(10);
/// Quality failures cap here; the third is terminal
/// (`email_extraction_failed`).
pub const MAX_QUALITY_ATTEMPTS: i32 = 3;
/// Backoff after quality failure 1 and 2.
pub const QUALITY_BACKOFF: [Duration; 2] = [Duration::from_secs(60), Duration::from_secs(300)];
/// Transport-failure retry — doubles as the claim lease. The config
/// cross-check (spec §5) keeps max attempt duration below this.
pub const TRANSPORT_RETRY: Duration = Duration::from_secs(60);
/// Per-pass backstop so one pass's span stays bounded.
const MAX_CLAIMS_PER_PASS: usize = 50;

/// What one pass did — the span fields and the test observable.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ExtractionReport {
    pub claimed: usize,
    pub resolved: usize,
    pub not_a_lead: usize,
    pub failed_terminal: usize,
    pub retryable: usize,
    pub superseded: usize,
}

/// Spawns the periodic worker. First pass after one interval (the sweep
/// posture); when a pass claimed work the loop continues draining within
/// `run_once` itself, so no fast-path re-tick is needed here.
pub fn spawn(
    pool: PgPool,
    key: RawPayloadKey,
    publisher: Publisher,
    extractor: Arc<dyn LeadExtractor>,
    poll_interval: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval.tick().await;
        loop {
            interval.tick().await;
            // Drain immediately when a pass filled its backstop — the
            // excess must not wait a full poll interval.
            loop {
                match run_once(&pool, &key, &publisher, extractor.as_ref()).await {
                    Ok(report) if report.claimed >= MAX_CLAIMS_PER_PASS => continue,
                    Ok(_) => break,
                    Err(err) => {
                        tracing::warn!(error_kind = "database", error = %err, "extraction pass failed");
                        break;
                    }
                }
            }
        }
    })
}

struct ClaimedRow {
    id: Uuid,
    organization_id: Uuid,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
    content_hmac: Vec<u8>,
    received_at: chrono::DateTime<chrono::Utc>,
    extraction_attempts: i32,
}

/// One pass: drain every claimable row (up to the backstop).
#[tracing::instrument(
    name = "intake.extraction_sweep",
    skip_all,
    fields(
        claimed = tracing::field::Empty,
        resolved = tracing::field::Empty,
        not_a_lead = tracing::field::Empty,
        failed_terminal = tracing::field::Empty,
        retryable = tracing::field::Empty,
        superseded = tracing::field::Empty,
    )
)]
pub async fn run_once(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    extractor: &dyn LeadExtractor,
) -> Result<ExtractionReport, sqlx::Error> {
    let mut report = ExtractionReport::default();

    while report.claimed < MAX_CLAIMS_PER_PASS {
        let Some(row) = claim_one(pool).await? else {
            break;
        };
        report.claimed += 1;
        attempt(pool, key, publisher, extractor, row, &mut report).await?;
    }

    let span = tracing::Span::current();
    span.record("claimed", report.claimed);
    span.record("resolved", report.resolved);
    span.record("not_a_lead", report.not_a_lead);
    span.record("failed_terminal", report.failed_terminal);
    span.record("retryable", report.retryable);
    span.record("superseded", report.superseded);
    Ok(report)
}

/// The claim: FOR UPDATE SKIP LOCKED on the eligibility predicate, then
/// the 60 s lease. The row lock lives only for this short transaction —
/// the LLM call happens outside any transaction (spec §4b).
async fn claim_one(pool: &PgPool) -> Result<Option<ClaimedRow>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query_as!(
        ClaimedRow,
        r#"SELECT id, organization_id, nonce, ciphertext, content_hmac,
                  received_at, extraction_attempts
           FROM raw_payload
           WHERE resolution = 'unresolved'
             AND unresolved_reason = 'email_unrecognized_format'
             AND payload_format = 'rfc822_v1'
             AND extraction_attempts < $1
             AND (extraction_next_attempt_at IS NULL
                  OR extraction_next_attempt_at <= now())
           ORDER BY received_at
           FOR UPDATE SKIP LOCKED
           LIMIT 1"#,
        MAX_QUALITY_ATTEMPTS,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(None);
    };
    sqlx::query!(
        r#"UPDATE raw_payload SET extraction_next_attempt_at = now() + $3::interval
           WHERE id = $1 AND organization_id = $2"#,
        row.id,
        row.organization_id,
        sqlx::postgres::types::PgInterval::try_from(TRANSPORT_RETRY)
            .expect("static interval converts"),
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(row))
}

/// The attempt's terminal disposition, applied under the row lock in one
/// transaction together with its ledger row (spec §3 race-safety).
enum Disposition {
    /// Reset to pending happens separately; this records the final ledger
    /// outcome after `complete_intake`.
    LedgerOnly {
        outcome: &'static str,
        confidence: Option<f32>,
    },
    Terminal {
        reason: &'static str,
        outcome: &'static str,
        confidence: Option<f32>,
        /// Quality-terminal counts the final attempt; `not_a_lead` (a
        /// success-class terminal) does not.
        count_attempt: bool,
    },
    QualityRetry {
        outcome: &'static str,
        confidence: Option<f32>,
    },
    TransportRetry {
        outcome: &'static str,
    },
}

#[allow(clippy::too_many_arguments)]
async fn attempt(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    extractor: &dyn LeadExtractor,
    row: ClaimedRow,
    report: &mut ExtractionReport,
) -> Result<(), sqlx::Error> {
    let span = tracing::info_span!(
        "intake.extract",
        raw_payload_id = %row.id,
        organization_id = %row.organization_id,
        provider = extractor.provider(),
        model = extractor.model(),
        outcome = tracing::field::Empty,
        confidence = tracing::field::Empty,
        input_truncated = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
        // SLICE_007h1 §4: statics/scalars only — never the inner
        // sender, subject, or text.
        forwarded = tracing::field::Empty,
        forward_style = tracing::field::Empty,
        forward_depth = tracing::field::Empty,
    );
    // Instrumented, never a held `enter()` guard across the awaits below
    // (the multi-threaded-runtime span-corruption footgun — review F1).
    let handle = span.clone();
    attempt_inner(pool, key, publisher, extractor, row, report, handle)
        .instrument(span)
        .await
}

#[allow(clippy::too_many_arguments)]
async fn attempt_inner(
    pool: &PgPool,
    key: &RawPayloadKey,
    publisher: &Publisher,
    extractor: &dyn LeadExtractor,
    row: ClaimedRow,
    report: &mut ExtractionReport,
    span: tracing::Span,
) -> Result<(), sqlx::Error> {
    let correlation_id = Uuid::new_v4();
    let started = Instant::now();
    let occurred_at = Utc::now();

    // Decrypt + rebuild the D-038-scoped input, then call the model.
    type AttemptOk = (ClaimVerdict, Option<u32>, Option<u32>, bool);
    let attempt_outcome: Result<AttemptOk, &'static str> = {
        match crypto::open(
            key,
            row.organization_id,
            row.id,
            &row.nonce,
            &row.ciphertext,
        ) {
            Err(_) => Err("internal_error"),
            Ok(plaintext) => match email::mime::parse(&plaintext) {
                None => Err("internal_error"),
                Some(mail) => {
                    // The one shared unwrap seam (docs/specs/SLICE_007h1.md
                    // §3/§5): the model sees the same view pinned matching
                    // saw — the inner message of a recognized forward
                    // (inner subject/domain/text under the unchanged D-038
                    // scope), the whole message otherwise. Rows here
                    // matched no format as delivered, so no direct-detect
                    // pass is repeated.
                    let resolved = email::forward::resolve(mail);
                    if let email::SenderTrust::ForwardedClaim { depth } = resolved.trust {
                        span.record("forwarded", true);
                        span.record("forward_depth", depth);
                        if let Some(style) = resolved.style {
                            span.record("forward_style", style);
                        }
                    }
                    let input = build_input(&resolved.mail);
                    span.record("input_truncated", input.truncated);
                    match extractor.extract(&input).await {
                        Err(ExtractorError::Timeout) => Err("provider_timeout"),
                        Err(ExtractorError::Unavailable) => Err("provider_unavailable"),
                        Err(ExtractorError::RateLimited) => Err("rate_limited"),
                        Err(ExtractorError::Malformed) => Err("malformed_response"),
                        Ok(reply) => Ok((
                            validate_reply(&input, &reply.content),
                            reply.prompt_tokens,
                            reply.completion_tokens,
                            input.truncated,
                        )),
                    }
                }
            },
        }
    };
    let duration_ms = started.elapsed().as_millis().min(i32::MAX as u128) as i32;
    span.record("duration_ms", duration_ms);

    let ledger = LedgerRow {
        organization_id: row.organization_id,
        raw_payload_id: row.id,
        provider: extractor.provider(),
        model: extractor.model().to_string(),
        input_truncated: matches!(&attempt_outcome, Ok((_, _, _, t)) if *t),
        prompt_tokens: attempt_outcome.as_ref().ok().and_then(|(_, p, _, _)| *p),
        completion_tokens: attempt_outcome.as_ref().ok().and_then(|(_, _, c, _)| *c),
        duration_ms,
        occurred_at,
        correlation_id,
    };

    match attempt_outcome {
        Err(outcome_tag) => {
            span.record("outcome", outcome_tag);
            match outcome_tag {
                // Genuine provider outages: never count, never terminal —
                // the row waits forever (spec §4a). The claim already set
                // the 60 s retry.
                "provider_timeout" | "provider_unavailable" | "rate_limited" => {
                    apply(
                        pool,
                        &row,
                        Disposition::TransportRetry {
                            outcome: outcome_tag,
                        },
                        &ledger,
                        publisher,
                    )
                    .await?;
                    report.retryable += 1;
                }
                // Deterministic failures (corrupt row, unreadable reply):
                // COUNTED — an unbounded retry here would be an infinite
                // paid-call loop, not outage patience (adversarial H1).
                _ => {
                    let terminal = row.extraction_attempts + 1 >= MAX_QUALITY_ATTEMPTS;
                    let disposition = if terminal {
                        Disposition::Terminal {
                            reason: "email_extraction_failed",
                            outcome: outcome_tag,
                            confidence: None,
                            count_attempt: true,
                        }
                    } else {
                        Disposition::QualityRetry {
                            outcome: outcome_tag,
                            confidence: None,
                        }
                    };
                    apply(pool, &row, disposition, &ledger, publisher).await?;
                    if terminal {
                        report.failed_terminal += 1;
                    } else {
                        report.retryable += 1;
                    }
                }
            }
        }
        Ok((verdict, _, _, _)) => match verdict {
            ClaimVerdict::Lead {
                validated,
                confidence,
            } => {
                span.record("confidence", confidence);
                // Guarded reset (the workbench template), then the shared
                // completion path, then the ledger under the row lock.
                if !reset_to_pending(pool, &row).await? {
                    span.record("outcome", "superseded");
                    apply(
                        pool,
                        &row,
                        Disposition::LedgerOnly {
                            outcome: "superseded",
                            confidence: None,
                        },
                        &ledger,
                        publisher,
                    )
                    .await?;
                    report.superseded += 1;
                    return Ok(());
                }
                let actor = IntakeActor::System {
                    organization_id: row.organization_id,
                    origin: Origin::Webhook,
                    correlation_id,
                    on_behalf_of_user_id: None,
                };
                let params = CompleteIntake {
                    raw_payload_id: row.id,
                    content_hmac: &row.content_hmac,
                    received_at: row.received_at,
                    assign_to_user_id: None,
                };
                // The closure constructs a fresh (Source, ParsedLead) on
                // every invocation (the lock-retry loop may re-run it;
                // ParsedLead is not Clone — spec §4b).
                let result = complete_intake(pool, key, publisher, &actor, params, move |_| {
                    Ok(validated.to_parsed())
                })
                .await;
                match result {
                    Ok(ReceiveInquiryOutcome::Resolved { .. }) => {
                        span.record("outcome", "extracted");
                        apply(
                            pool,
                            &row,
                            Disposition::LedgerOnly {
                                outcome: "extracted",
                                confidence: Some(confidence),
                            },
                            &ledger,
                            publisher,
                        )
                        .await?;
                        report.resolved += 1;
                    }
                    // The closure never fails, so Unresolved cannot occur;
                    // treat defensively as superseded.
                    Ok(ReceiveInquiryOutcome::Unresolved { .. }) => {
                        span.record("outcome", "superseded");
                        apply(
                            pool,
                            &row,
                            Disposition::LedgerOnly {
                                outcome: "superseded",
                                confidence: None,
                            },
                            &ledger,
                            publisher,
                        )
                        .await?;
                        report.superseded += 1;
                    }
                    // ANY post-reset error: un-reset so the row never
                    // strands as pending (spec §4b). IntakeBusy is real
                    // contention (uncounted); everything else is
                    // deterministic and COUNTED (adversarial H1) so a
                    // poisoned row cannot loop on paid calls forever.
                    Err(err) => {
                        let (outcome_tag, counted) = match err {
                            CommandError::IntakeBusy => ("intake_busy", false),
                            _ => ("internal_error", true),
                        };
                        span.record("outcome", outcome_tag);
                        let terminal =
                            un_reset(pool, &row, outcome_tag, counted, &ledger, publisher).await?;
                        if terminal {
                            report.failed_terminal += 1;
                        } else {
                            report.retryable += 1;
                        }
                    }
                }
            }
            ClaimVerdict::NotALead { confidence } => {
                span.record("outcome", "not_a_lead");
                span.record("confidence", confidence);
                apply(
                    pool,
                    &row,
                    Disposition::Terminal {
                        reason: "not_a_lead",
                        outcome: "not_a_lead",
                        confidence: Some(confidence),
                        count_attempt: false,
                    },
                    &ledger,
                    publisher,
                )
                .await?;
                report.not_a_lead += 1;
            }
            ClaimVerdict::Failed {
                failure,
                confidence,
            } => {
                let tag = failure.ledger_tag();
                span.record("outcome", tag);
                if let Some(c) = confidence {
                    span.record("confidence", c);
                }
                let terminal = row.extraction_attempts + 1 >= MAX_QUALITY_ATTEMPTS;
                let disposition = if terminal {
                    Disposition::Terminal {
                        reason: "email_extraction_failed",
                        outcome: tag,
                        confidence,
                        count_attempt: true,
                    }
                } else {
                    Disposition::QualityRetry {
                        outcome: tag,
                        confidence,
                    }
                };
                apply(pool, &row, disposition, &ledger, publisher).await?;
                if terminal {
                    report.failed_terminal += 1;
                } else {
                    report.retryable += 1;
                }
            }
        },
    }
    Ok(())
}

/// The guarded reset: re-lock, verify still ours, flip to pending.
/// Returns false when a concurrent discard/retry/resolve superseded us.
async fn reset_to_pending(pool: &PgPool, row: &ClaimedRow) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let locked = store::lock_for_processing(&mut tx, row.id, row.organization_id).await?;
    let still_ours = matches!(
        &locked,
        Some(l) if l.resolution == "unresolved"
            && l.unresolved_reason.as_deref() == Some("email_unrecognized_format")
    );
    if !still_ours {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query!(
        r#"UPDATE raw_payload
           SET resolution = 'pending', unresolved_reason = NULL, resolved_at = NULL
           WHERE id = $1 AND organization_id = $2"#,
        row.id,
        row.organization_id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// The un-reset after a post-reset `complete_intake` error (spec §4b):
/// put the row back and ledger the attempt, all under the row lock.
/// `counted` errors take the quality accounting (terminal at the cap —
/// adversarial H1); returns whether the row went terminal. Skipped when
/// an admin acted meanwhile (the ledger row is still written).
async fn un_reset(
    pool: &PgPool,
    row: &ClaimedRow,
    outcome: &'static str,
    counted: bool,
    ledger: &LedgerRow,
    publisher: &Publisher,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let locked = store::lock_for_processing(&mut tx, row.id, row.organization_id).await?;
    let mut terminal = false;
    if matches!(&locked, Some(l) if l.resolution == "pending") {
        if counted && row.extraction_attempts + 1 >= MAX_QUALITY_ATTEMPTS {
            terminal = true;
            sqlx::query!(
                r#"UPDATE raw_payload
                   SET resolution = 'unresolved',
                       unresolved_reason = 'email_extraction_failed',
                       resolved_at = now(),
                       extraction_attempts = extraction_attempts + 1
                   WHERE id = $1 AND organization_id = $2"#,
                row.id,
                row.organization_id,
            )
            .execute(&mut *tx)
            .await?;
        } else if counted {
            let backoff =
                QUALITY_BACKOFF[(row.extraction_attempts as usize).min(QUALITY_BACKOFF.len() - 1)];
            sqlx::query!(
                r#"UPDATE raw_payload
                   SET resolution = 'unresolved',
                       unresolved_reason = 'email_unrecognized_format',
                       resolved_at = now(),
                       extraction_attempts = extraction_attempts + 1,
                       extraction_next_attempt_at = now() + $3::interval
                   WHERE id = $1 AND organization_id = $2"#,
                row.id,
                row.organization_id,
                sqlx::postgres::types::PgInterval::try_from(backoff)
                    .expect("static interval converts"),
            )
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query!(
                r#"UPDATE raw_payload
                   SET resolution = 'unresolved',
                       unresolved_reason = 'email_unrecognized_format',
                       resolved_at = now(),
                       extraction_next_attempt_at = now() + $3::interval
                   WHERE id = $1 AND organization_id = $2"#,
                row.id,
                row.organization_id,
                sqlx::postgres::types::PgInterval::try_from(TRANSPORT_RETRY)
                    .expect("static interval converts"),
            )
            .execute(&mut *tx)
            .await?;
        }
    }
    insert_ledger(&mut tx, ledger, outcome, None).await?;
    tx.commit().await?;

    if terminal {
        let event = RealtimeEvent::intake_unresolved_changed(
            row.organization_id,
            ledger.occurred_at,
            ledger.correlation_id,
            row.id,
        );
        publisher
            .publish_after_commit(Publication::for_event(event))
            .await;
    }
    Ok(terminal)
}

/// Applies a disposition and its ledger row in one transaction under the
/// row lock (spec §3). Publishes after commit where the queue changed.
async fn apply(
    pool: &PgPool,
    row: &ClaimedRow,
    disposition: Disposition,
    ledger: &LedgerRow,
    publisher: &Publisher,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    // Serialize with 007e actions and other workers.
    let locked = store::lock_for_processing(&mut tx, row.id, row.organization_id).await?;
    let still_ours = matches!(
        &locked,
        Some(l) if l.resolution == "unresolved"
            && l.unresolved_reason.as_deref() == Some("email_unrecognized_format")
    );

    let (outcome, confidence, publish) = match disposition {
        Disposition::LedgerOnly {
            outcome,
            confidence,
        } => (outcome, confidence, false),
        Disposition::Terminal {
            reason,
            outcome,
            confidence,
            count_attempt,
        } => {
            if still_ours {
                sqlx::query!(
                    r#"UPDATE raw_payload
                       SET unresolved_reason = $3, resolved_at = now(),
                           extraction_attempts = extraction_attempts
                               + CASE WHEN $4 THEN 1 ELSE 0 END
                       WHERE id = $1 AND organization_id = $2"#,
                    row.id,
                    row.organization_id,
                    reason,
                    count_attempt,
                )
                .execute(&mut *tx)
                .await?;
                (outcome, confidence, true)
            } else {
                ("superseded", None, false)
            }
        }
        Disposition::QualityRetry {
            outcome,
            confidence,
        } => {
            if still_ours {
                let backoff = QUALITY_BACKOFF
                    [(row.extraction_attempts as usize).min(QUALITY_BACKOFF.len() - 1)];
                sqlx::query!(
                    r#"UPDATE raw_payload
                       SET extraction_attempts = extraction_attempts + 1,
                           extraction_next_attempt_at = now() + $3::interval
                       WHERE id = $1 AND organization_id = $2"#,
                    row.id,
                    row.organization_id,
                    sqlx::postgres::types::PgInterval::try_from(backoff)
                        .expect("static interval converts"),
                )
                .execute(&mut *tx)
                .await?;
                (outcome, confidence, false)
            } else {
                ("superseded", None, false)
            }
        }
        Disposition::TransportRetry { outcome } => {
            // The claim's lease already set next_attempt_at; attempts
            // unchanged (never counts). Nothing user-visible changed.
            (outcome, None, false)
        }
    };

    insert_ledger(&mut tx, ledger, outcome, confidence).await?;
    tx.commit().await?;

    if publish {
        let event = RealtimeEvent::intake_unresolved_changed(
            row.organization_id,
            ledger.occurred_at,
            ledger.correlation_id,
            row.id,
        );
        publisher
            .publish_after_commit(Publication::for_event(event))
            .await;
    }
    Ok(())
}

struct LedgerRow {
    organization_id: Uuid,
    raw_payload_id: Uuid,
    provider: &'static str,
    model: String,
    input_truncated: bool,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    duration_ms: i32,
    occurred_at: chrono::DateTime<chrono::Utc>,
    correlation_id: Uuid,
}

/// The ledger INSERT, in the caller's row-locked transaction; `seq` is
/// computed under that lock so concurrent writers (a lapsed lease) can
/// never collide on the UNIQUE (spec §3).
async fn insert_ledger(
    tx: &mut PgConnection,
    ledger: &LedgerRow,
    outcome: &'static str,
    confidence: Option<f32>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO intake_extraction
            (id, organization_id, raw_payload_id, seq, provider, model,
             outcome, confidence, input_truncated, prompt_tokens,
             completion_tokens, duration_ms, occurred_at, correlation_id)
           VALUES ($1, $2, $3,
                   (SELECT COALESCE(MAX(seq), 0) + 1 FROM intake_extraction
                    WHERE raw_payload_id = $3),
                   $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
        Uuid::new_v4(),
        ledger.organization_id,
        ledger.raw_payload_id,
        ledger.provider,
        ledger.model,
        outcome,
        confidence,
        ledger.input_truncated,
        ledger.prompt_tokens.map(|t| t as i32),
        ledger.completion_tokens.map(|t| t as i32),
        ledger.duration_ms,
        ledger.occurred_at,
        ledger.correlation_id,
    )
    .execute(tx)
    .await?;
    Ok(())
}
