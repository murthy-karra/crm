//! Today: a computed, deterministic read model (AGENTS.md §4.7, D-010;
//! docs/specs/SLICE_003.md §3, §4). Not a table — computed per request
//! from authoritative rows inside one read-only repeatable-read transaction,
//! so there is no projection lag and no second source of truth. Its bounded
//! sequential statements share one snapshot while evaluating the built-in
//! queue and enabled live sources.

pub mod model;
pub mod rank;
pub mod sources;
pub mod system_feeds;
#[cfg(feature = "test-support")]
pub mod test_support;

pub use model::{
    InquiryRef, RecommendedAction, SystemFeedIssue, SystemFeedIssueError, TodayCandidate,
    TodayItem, TodayList, TodayPriority, TodayReason, TodaySourceIssue, TodaySourceIssueError,
    TodaySources, TodaySourcesStatus, FRESH_INQUIRY_WINDOW_HOURS,
};
pub use rank::rank;
pub use sources::{
    disable_today_work_source, enable_today_work_source, list_today_work_sources,
    DisableTodayWorkSource, EnableTodayWorkSource, TodaySource, TodaySourceChange,
    TODAY_SOURCE_LIMIT,
};

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use sqlx::pool::PoolConnection;
use sqlx::{Acquire, PgConnection, Postgres};
use tracing::Instrument;
use uuid::Uuid;

use crate::domain::person::visibility::PersonVisibilityScope;
use crate::ids::UserId;

/// What a Slice 005 Operator tool calls — never a separate path
/// (docs/specs/SLICE_003.md §3).
struct QueryOutcome {
    list: TodayList,
    connection_healthy: bool,
    telemetry: QueryTelemetry,
}

#[derive(Debug, Clone, Copy)]
enum SourceMetadataOutcome {
    Complete,
    Unavailable,
    TimedOut,
}

#[derive(Debug, Clone, Copy)]
struct QueryTelemetry {
    builtin_candidate_count: usize,
    builtin_truncated: bool,
    metadata_outcome: SourceMetadataOutcome,
    // These values stay absent when metadata was unavailable: zero would
    // falsely claim that no sources were configured or evaluated.
    enabled_source_count: Option<usize>,
    successful_source_count: Option<usize>,
    failed_source_count: Option<usize>,
    list_candidate_count: Option<usize>,
    list_item_count: Option<usize>,
    list_truncated: Option<bool>,
    /// docs/specs/SLICE_011d.md §8/§9.9: per-feed status classification
    /// only (`default|customized|disabled|fallback`) — never the
    /// definition itself.
    feed_statuses: Option<[(&'static str, &'static str); 3]>,
    person_state_candidate_count: Option<usize>,
    call_candidate_count: Option<usize>,
}

struct QuerySpanGuard {
    span: tracing::Span,
    started: Instant,
    finished: bool,
}

impl QuerySpanGuard {
    fn new(span: tracing::Span) -> Self {
        Self {
            span,
            started: Instant::now(),
            finished: false,
        }
    }

    fn finish(&mut self, outcome: &'static str) {
        self.span
            .record("duration_ms", self.started.elapsed().as_millis() as u64);
        self.span.record("outcome", outcome);
        self.finished = true;
    }
}

impl Drop for QuerySpanGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.span
                .record("duration_ms", self.started.elapsed().as_millis() as u64);
            self.span.record("outcome", "cancelled");
        }
    }
}

pub async fn query(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    _now: DateTime<Utc>,
) -> Result<TodayList, sqlx::Error> {
    Ok(query_inner(conn, scope, viewer, EvaluationClock::Database)
        .await?
        .list)
}

/// Test-only common-clock seam for filter parity and paired-baseline tests.
/// The request still reads PostgreSQL's snapshot clock first; the fixture
/// value then replaces only Today evaluation boundaries. HTTP callers can
/// neither provide nor select a clock.
#[cfg(feature = "test-support")]
pub async fn query_at(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    now: DateTime<Utc>,
) -> Result<TodayList, sqlx::Error> {
    Ok(
        query_inner(conn, scope, viewer, EvaluationClock::Fixed(now))
            .await?
            .list,
    )
}

enum EvaluationClock {
    Database,
    #[cfg(feature = "test-support")]
    Fixed(DateTime<Utc>),
}

async fn query_inner(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    evaluation_clock: EvaluationClock,
) -> Result<QueryOutcome, sqlx::Error> {
    let span = tracing::info_span!(
        "today.query",
        organization_id = %scope.organization_id(),
        actor_id = %viewer,
        outcome = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
        item_count = tracing::field::Empty,
        truncated = tracing::field::Empty,
        builtin_candidate_count = tracing::field::Empty,
        builtin_truncated = tracing::field::Empty,
        source_metadata_outcome = tracing::field::Empty,
        enabled_source_count = tracing::field::Empty,
        successful_source_count = tracing::field::Empty,
        failed_source_count = tracing::field::Empty,
        list_candidate_count = tracing::field::Empty,
        list_item_count = tracing::field::Empty,
        list_truncated = tracing::field::Empty,
        sources_status = tracing::field::Empty,
        source_issue_count = tracing::field::Empty,
        // docs/specs/SLICE_011d.md §8/§9.9: per-feed status classification
        // (default|customized|disabled|fallback) and candidate counts —
        // never the definition JSON, subject items, or bound parameters.
        feed_status_unanswered_inquiry = tracing::field::Empty,
        feed_status_client_replied = tracing::field::Empty,
        feed_status_call_outcome_needed = tracing::field::Empty,
        person_state_candidate_count = tracing::field::Empty,
        call_candidate_count = tracing::field::Empty,
    );
    let mut trace = QuerySpanGuard::new(span.clone());
    let result = async { query_inner_untraced(conn, scope, viewer, evaluation_clock).await }
        .instrument(span.clone())
        .await;
    match &result {
        Ok(outcome) => {
            span.record("item_count", outcome.list.items.len());
            span.record("truncated", outcome.list.truncated);
            span.record(
                "builtin_candidate_count",
                outcome.telemetry.builtin_candidate_count,
            );
            span.record("builtin_truncated", outcome.telemetry.builtin_truncated);
            span.record(
                "source_metadata_outcome",
                source_metadata_outcome_label(outcome.telemetry.metadata_outcome),
            );
            record_optional_usize(
                &span,
                "enabled_source_count",
                outcome.telemetry.enabled_source_count,
            );
            record_optional_usize(
                &span,
                "successful_source_count",
                outcome.telemetry.successful_source_count,
            );
            record_optional_usize(
                &span,
                "failed_source_count",
                outcome.telemetry.failed_source_count,
            );
            record_optional_usize(
                &span,
                "list_candidate_count",
                outcome.telemetry.list_candidate_count,
            );
            record_optional_usize(&span, "list_item_count", outcome.telemetry.list_item_count);
            if let Some(list_truncated) = outcome.telemetry.list_truncated {
                span.record("list_truncated", list_truncated);
            }
            span.record(
                "sources_status",
                sources_status_label(outcome.list.sources.status),
            );
            span.record("source_issue_count", outcome.list.sources.issues.len());
            if let Some(feed_statuses) = outcome.telemetry.feed_statuses {
                for (feed_key, status) in feed_statuses {
                    match feed_key {
                        "unanswered_inquiry" => {
                            span.record("feed_status_unanswered_inquiry", status);
                        }
                        "client_replied" => {
                            span.record("feed_status_client_replied", status);
                        }
                        "call_outcome_needed" => {
                            span.record("feed_status_call_outcome_needed", status);
                        }
                        _ => {}
                    }
                }
            }
            record_optional_usize(
                &span,
                "person_state_candidate_count",
                outcome.telemetry.person_state_candidate_count,
            );
            record_optional_usize(
                &span,
                "call_candidate_count",
                outcome.telemetry.call_candidate_count,
            );
            trace.finish(match outcome.list.sources.status {
                TodaySourcesStatus::Complete => "complete",
                TodaySourcesStatus::Partial => "sources_partial",
                TodaySourcesStatus::Unavailable => "sources_unavailable",
            });
        }
        Err(_) => {
            trace.finish("database_error");
        }
    }
    result
}

async fn query_inner_untraced(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    evaluation_clock: EvaluationClock,
) -> Result<QueryOutcome, sqlx::Error> {
    // One snapshot and one server-selected clock bind built-ins and all
    // source age filters together. `READ ONLY` keeps a malformed/slow source
    // from accidentally making a read-path write.
    let mut tx = conn.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    // Approved §8 transaction-local planning adjustment. `SET LOCAL` dies
    // with this transaction, including the error/drop path.
    sqlx::query("SET LOCAL jit = off").execute(&mut *tx).await?;
    // §8 planning change 4 (approved by the user 2026-09-06 after Phase B):
    // pins the Nested Loop Anti Join for the per-Person effective-contact
    // probe; with a full visibility map the planner otherwise picks a Merge
    // Anti Join that scans the whole corrections index per Person (about 10x
    // slower on large books); `SET LOCAL` dies with the transaction.
    sqlx::query("SET LOCAL enable_mergejoin = off")
        .execute(&mut *tx)
        .await?;
    let database_now: DateTime<Utc> = sqlx::query_scalar("SELECT statement_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    let now = match evaluation_clock {
        EvaluationClock::Database => database_now,
        #[cfg(feature = "test-support")]
        EvaluationClock::Fixed(now) => now,
    };
    let feeds_result = evaluate_feeds_builtins(&mut tx, scope, viewer, now).await?;
    if feeds_result.call_feed_unrecoverable {
        // Mirrors the source-metadata-unavailable early return exactly: the
        // call feed's own savepoint recovery could not complete inside its
        // shared grace, so the connection's state past that point is
        // unknown. Never attempt list-source work on it; report the whole
        // Today response as unavailable, with the call feed's issue still
        // present, and let `tx` drop uncommitted — the owned-connection
        // guard detaches rather than pools it (`connection_healthy: false`).
        return Ok(QueryOutcome {
            list: TodayList {
                generated_at: now,
                items: feeds_result.items,
                truncated: feeds_result.truncated,
                sources: TodaySources {
                    status: TodaySourcesStatus::Unavailable,
                    issues: Vec::new(),
                    system_feed_issues: feeds_result.system_feed_issues,
                },
            },
            connection_healthy: false,
            telemetry: QueryTelemetry {
                builtin_candidate_count: feeds_result.candidate_count,
                builtin_truncated: feeds_result.truncated,
                metadata_outcome: SourceMetadataOutcome::Unavailable,
                enabled_source_count: None,
                successful_source_count: None,
                failed_source_count: None,
                list_candidate_count: None,
                list_item_count: None,
                list_truncated: None,
                feed_statuses: feeds_result.feed_statuses,
                person_state_candidate_count: feeds_result.person_state_candidate_count,
                call_candidate_count: feeds_result.call_candidate_count,
            },
        });
    }
    let FeedsBuiltins {
        items: mut builtins,
        truncated: builtin_truncated,
        candidate_count: builtin_candidate_count,
        feed_statuses,
        person_state_candidate_count,
        call_candidate_count,
        system_feed_issues,
        call_feed_recovery_deadline,
        ..
    } = feeds_result;
    let builtin_ids: Vec<Uuid> = builtins
        .iter()
        .map(|item| item.person.id.as_uuid())
        .collect();

    #[cfg(feature = "test-support")]
    test_support::checkpoint(
        test_support::TodayQueryPhase::AfterBuiltins,
        None,
        None,
        &mut tx,
    )
    .await?;

    // The deadline includes savepoint setup, the metadata statement and its
    // release. A dropped timeout future is followed only by the single,
    // shared recovery grace below; it can never hold the request indefinitely.
    let enumeration_started = Instant::now();
    let enumeration_deadline = enumeration_started + ENUMERATION_BUDGET;
    let enumeration_span = tracing::info_span!(
        "today.source_enumeration",
        organization_id = %scope.organization_id(),
        actor_id = %viewer,
        outcome = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
        source_count = tracing::field::Empty,
    );
    let enumeration = async {
        tokio::time::timeout(ENUMERATION_BUDGET, async {
            set_source_statement_timeout_until(&mut tx, enumeration_deadline).await?;
            sqlx::query("SAVEPOINT today_source_metadata")
                .execute(&mut *tx)
                .await?;
            set_source_statement_timeout_until(&mut tx, enumeration_deadline).await?;
            let sources = sources::evaluated_sources_raw(&mut tx, scope.organization_id(), viewer)
                .await
                .map_err(|error| match error {
                    crate::domain::saved_list::SavedListError::Database(error) => error,
                    _ => sqlx::Error::Decode("today source metadata is corrupt".into()),
                })?;
            set_source_statement_timeout_until(&mut tx, enumeration_deadline).await?;
            sqlx::query("RELEASE SAVEPOINT today_source_metadata")
                .execute(&mut *tx)
                .await?;
            Ok::<_, sqlx::Error>(sources)
        })
        .await
    }
    .instrument(enumeration_span.clone())
    .await;
    enumeration_span.record(
        "duration_ms",
        enumeration_started.elapsed().as_millis() as u64,
    );
    let configured = match enumeration {
        Ok(Ok(sources)) => {
            enumeration_span.record("outcome", "complete");
            enumeration_span.record("source_count", sources.len());
            #[cfg(feature = "test-support")]
            test_support::record_source_enumeration(
                test_support::SourceEnumerationOutcome::Complete,
                enumeration_started.elapsed(),
            );
            sources
        }
        Ok(Err(_)) => {
            enumeration_span.record("outcome", "unavailable");
            // Metadata did not materialize, so zero would falsely claim an
            // observed configured-source count. Leave this optional span
            // field unset; the query summary likewise records counts as
            // unknown for this outcome.
            #[cfg(feature = "test-support")]
            test_support::record_source_enumeration(
                test_support::SourceEnumerationOutcome::Unavailable,
                enumeration_started.elapsed(),
            );
            let recovery_started = Instant::now();
            let recovery_deadline = recovery_started + RECOVERY_BUDGET;
            let recovered = match recovery_remaining(recovery_started) {
                Ok(remaining) => rollback_metadata_within(&mut tx, remaining).await,
                Err(_) => false,
            };
            let connection_healthy = if recovered {
                match recovery_remaining(recovery_started) {
                    Ok(remaining) => commit_within(tx, remaining, recovery_deadline).await,
                    Err(_) => false,
                }
            } else {
                false
            };
            return Ok(QueryOutcome {
                list: TodayList {
                    generated_at: now,
                    items: builtins,
                    truncated: builtin_truncated,
                    sources: TodaySources {
                        status: TodaySourcesStatus::Unavailable,
                        issues: Vec::new(),
                        system_feed_issues: system_feed_issues.clone(),
                    },
                },
                connection_healthy,
                telemetry: QueryTelemetry {
                    builtin_candidate_count,
                    builtin_truncated,
                    metadata_outcome: SourceMetadataOutcome::Unavailable,
                    enabled_source_count: None,
                    successful_source_count: None,
                    failed_source_count: None,
                    list_candidate_count: None,
                    list_item_count: None,
                    list_truncated: None,
                    feed_statuses,
                    person_state_candidate_count,
                    call_candidate_count,
                },
            });
        }
        Err(_) => {
            enumeration_span.record("outcome", "timed_out");
            // As with an unavailable metadata statement, the configured
            // source count is unknown rather than observed as zero.
            #[cfg(feature = "test-support")]
            test_support::record_source_enumeration(
                test_support::SourceEnumerationOutcome::TimedOut,
                enumeration_started.elapsed(),
            );
            let recovery_started = Instant::now();
            let recovery_deadline = recovery_started + RECOVERY_BUDGET;
            let recovered = match recovery_remaining(recovery_started) {
                Ok(remaining) => rollback_metadata_within(&mut tx, remaining).await,
                Err(_) => false,
            };
            let connection_healthy = if recovered {
                match recovery_remaining(recovery_started) {
                    Ok(remaining) => commit_within(tx, remaining, recovery_deadline).await,
                    Err(_) => false,
                }
            } else {
                false
            };
            return Ok(QueryOutcome {
                list: TodayList {
                    generated_at: now,
                    items: builtins,
                    truncated: builtin_truncated,
                    sources: TodaySources {
                        status: TodaySourcesStatus::Unavailable,
                        issues: Vec::new(),
                        system_feed_issues: system_feed_issues.clone(),
                    },
                },
                connection_healthy,
                telemetry: QueryTelemetry {
                    builtin_candidate_count,
                    builtin_truncated,
                    metadata_outcome: SourceMetadataOutcome::TimedOut,
                    enabled_source_count: None,
                    successful_source_count: None,
                    failed_source_count: None,
                    list_candidate_count: None,
                    list_item_count: None,
                    list_truncated: None,
                    feed_statuses,
                    person_state_candidate_count,
                    call_candidate_count,
                },
            });
        }
    };

    #[cfg(feature = "test-support")]
    test_support::checkpoint(
        test_support::TodayQueryPhase::AfterMetadata,
        None,
        None,
        &mut tx,
    )
    .await?;

    let enabled_source_count = configured.len();
    let mut issues = Vec::new();
    let mut successful = Vec::<(sources::TodaySource, Vec<sources::SourceCandidate>)>::new();
    // Once a recovery operation cannot complete inside its shared 100 ms
    // grace, no later query or transaction completion may reuse this socket.
    let mut connection_healthy = true;
    // Seeded from a recovered call-feed failure (spec §5 step 4), so the
    // final commit uses whatever remains of THAT recovery's shared grace
    // when no list source runs afterward to reset it — recoveries share one
    // total budget, never stack a fresh one per stage.
    let mut final_recovery_deadline = call_feed_recovery_deadline;
    let mut configured = configured.into_iter();
    while let Some(configured_source) = configured.next() {
        let source = configured_source.source;
        let Some(filter) = configured_source.filter else {
            let span = tracing::info_span!(
                "today.source_evaluation",
                organization_id = %scope.organization_id(),
                actor_id = %viewer,
                source_id = %source.list_id,
                filter_kinds = tracing::field::Empty,
                filter_clause_count = tracing::field::Empty,
                builtin_count = builtins.len(),
                builtin_truncated,
                outcome = tracing::field::Empty,
                duration_ms = tracing::field::Empty,
                membership_count = tracing::field::Empty,
                prefix_candidate_count = tracing::field::Empty,
            );
            span.record("filter_kinds", "unavailable");
            span.record("filter_clause_count", 0usize);
            span.record("duration_ms", 0u64);
            span.record("membership_count", 0usize);
            span.record("prefix_candidate_count", 0usize);
            span.record(
                "outcome",
                match source.filter_error {
                    Some(crate::domain::saved_list::SavedListFilterError::InvalidStage) => {
                        "invalid_stage"
                    }
                    Some(crate::domain::saved_list::SavedListFilterError::InvalidAssignee) => {
                        "invalid_assignee"
                    }
                    _ => "unsupported_filter",
                },
            );
            issues.push(TodaySourceIssue {
                list_id: source.list_id,
                name: source.name,
                revision: source.revision,
                error: match source.filter_error {
                    Some(crate::domain::saved_list::SavedListFilterError::InvalidStage) => {
                        TodaySourceIssueError::InvalidStage
                    }
                    Some(crate::domain::saved_list::SavedListFilterError::InvalidAssignee) => {
                        TodaySourceIssueError::InvalidAssignee
                    }
                    _ => TodaySourceIssueError::UnsupportedFilter,
                },
            });
            continue;
        };

        // This *one* deadline covers every reference probe, both source
        // reads and RELEASE. `statement_timeout` remains set locally too, so
        // the server cancels a stuck statement when the client remains alive.
        // A real new evaluation starts a fresh normal completion path. Pure
        // structural skips below do not: if they follow a recovered failure,
        // the final commit must still consume that same recovery grace.
        final_recovery_deadline = None;
        let source_started = Instant::now();
        let source_deadline = source_started + SOURCE_BUDGET;
        let source_filter_kinds_field = filter.kinds_field();
        let source_span = tracing::info_span!(
            "today.source_evaluation",
            organization_id = %scope.organization_id(),
            actor_id = %viewer,
            source_id = %source.list_id,
            filter_kinds = %source_filter_kinds_field,
            filter_clause_count = filter.clauses.len(),
            builtin_count = builtins.len(),
            builtin_truncated,
            outcome = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
            membership_count = tracing::field::Empty,
            prefix_candidate_count = tracing::field::Empty,
        );
        #[cfg(feature = "test-support")]
        let source_filter_kinds = filter
            .clauses
            .iter()
            .map(crate::domain::person::filter::Clause::kind_label)
            .collect::<Vec<_>>();
        let evaluated = async {
            tokio::time::timeout(
                SOURCE_BUDGET,
                evaluate_source(
                    &mut tx,
                    &filter,
                    SourceQueryContext {
                        source_id: source.list_id.as_uuid(),
                        organization_id: scope.organization_id(),
                        viewer,
                        now,
                        builtin_ids: &builtin_ids,
                        builtin_truncated,
                        builtin_len: builtins.len(),
                        deadline: source_deadline,
                    },
                ),
            )
            .await
        }
        .instrument(source_span.clone())
        .await;
        source_span.record("duration_ms", source_started.elapsed().as_millis() as u64);
        match evaluated {
            Ok(Ok(SourceEvaluation::Data { members, prefix })) => {
                source_span.record("outcome", "complete");
                source_span.record("membership_count", members.len());
                source_span.record("prefix_candidate_count", prefix.len());
                #[cfg(feature = "test-support")]
                test_support::record_source_evaluation(
                    test_support::SourceEvaluationOutcome::Complete,
                    source_started.elapsed(),
                    source_filter_kinds,
                );
                for member_id in members {
                    append_list_reason(&mut builtins, member_id, &source);
                }
                successful.push((source, prefix));
            }
            Ok(Ok(SourceEvaluation::InvalidStage)) => {
                source_span.record("outcome", "invalid_stage");
                source_span.record("membership_count", 0usize);
                source_span.record("prefix_candidate_count", 0usize);
                #[cfg(feature = "test-support")]
                test_support::record_source_evaluation(
                    test_support::SourceEvaluationOutcome::InvalidFilter,
                    source_started.elapsed(),
                    source_filter_kinds,
                );
                issues.push(TodaySourceIssue {
                    list_id: source.list_id,
                    name: source.name,
                    revision: source.revision,
                    error: TodaySourceIssueError::InvalidStage,
                });
            }
            Ok(Ok(SourceEvaluation::InvalidAssignee)) => {
                source_span.record("outcome", "invalid_assignee");
                source_span.record("membership_count", 0usize);
                source_span.record("prefix_candidate_count", 0usize);
                #[cfg(feature = "test-support")]
                test_support::record_source_evaluation(
                    test_support::SourceEvaluationOutcome::InvalidFilter,
                    source_started.elapsed(),
                    source_filter_kinds,
                );
                issues.push(TodaySourceIssue {
                    list_id: source.list_id,
                    name: source.name,
                    revision: source.revision,
                    error: TodaySourceIssueError::InvalidAssignee,
                });
            }
            Ok(Err(_)) => {
                source_span.record("outcome", "unavailable");
                source_span.record("membership_count", 0usize);
                source_span.record("prefix_candidate_count", 0usize);
                #[cfg(feature = "test-support")]
                test_support::record_source_evaluation(
                    test_support::SourceEvaluationOutcome::Unavailable,
                    source_started.elapsed(),
                    source_filter_kinds,
                );
                // Never merge an uncertain partial source. Roll back its
                // statement and release its savepoint under a single grace;
                // if that fails current and all unattempted sources are
                // explicitly unavailable and the owned guard discards this
                // connection after the captured snapshot is returned.
                issues.push(TodaySourceIssue {
                    list_id: source.list_id,
                    name: source.name,
                    revision: source.revision,
                    error: TodaySourceIssueError::Unavailable,
                });
                let recovery_started = Instant::now();
                let recovered = match recovery_remaining(recovery_started) {
                    Ok(remaining) => {
                        rollback_source_within(
                            &mut tx,
                            remaining,
                            source.list_id.as_uuid(),
                            recovery_started + RECOVERY_BUDGET,
                        )
                        .await
                    }
                    Err(_) => false,
                };
                if recovered {
                    final_recovery_deadline = Some(recovery_started + RECOVERY_BUDGET);
                    continue;
                }
                for pending in configured {
                    issues.push(TodaySourceIssue {
                        list_id: pending.source.list_id,
                        name: pending.source.name,
                        revision: pending.source.revision,
                        error: TodaySourceIssueError::Unavailable,
                    });
                }
                connection_healthy = false;
                break;
            }
            Err(_) => {
                source_span.record("outcome", "timed_out");
                source_span.record("membership_count", 0usize);
                source_span.record("prefix_candidate_count", 0usize);
                #[cfg(feature = "test-support")]
                test_support::record_source_evaluation(
                    test_support::SourceEvaluationOutcome::TimedOut,
                    source_started.elapsed(),
                    source_filter_kinds,
                );
                // Never merge an uncertain partial source. Roll back its
                // statement and release its savepoint under a single grace;
                // if that fails current and all unattempted sources are
                // explicitly unavailable and the owned guard discards this
                // connection after the captured snapshot is returned.
                issues.push(TodaySourceIssue {
                    list_id: source.list_id,
                    name: source.name,
                    revision: source.revision,
                    error: TodaySourceIssueError::Unavailable,
                });
                let recovery_started = Instant::now();
                let recovered = match recovery_remaining(recovery_started) {
                    Ok(remaining) => {
                        rollback_source_within(
                            &mut tx,
                            remaining,
                            source.list_id.as_uuid(),
                            recovery_started + RECOVERY_BUDGET,
                        )
                        .await
                    }
                    Err(_) => false,
                };
                if recovered {
                    final_recovery_deadline = Some(recovery_started + RECOVERY_BUDGET);
                    continue;
                }
                for pending in configured {
                    issues.push(TodaySourceIssue {
                        list_id: pending.source.list_id,
                        name: pending.source.name,
                        revision: pending.source.revision,
                        error: TodaySourceIssueError::Unavailable,
                    });
                }
                connection_healthy = false;
                break;
            }
        }
    }

    let successful_source_count = successful.len();
    let failed_source_count = issues.len();
    let k = 200usize.saturating_sub(builtins.len());
    let mut non_builtin = HashMap::<Uuid, (sources::SourceCandidate, Vec<TodayReason>)>::new();
    for (source, candidates) in successful {
        for candidate in candidates {
            let reason = TodayReason::ListMember {
                list_id: source.list_id,
                name: source.name.clone(),
            };
            non_builtin
                .entry(candidate.person.id.as_uuid())
                .and_modify(|(_, reasons)| {
                    if !reasons.iter().any(|r| matches!(r, TodayReason::ListMember { list_id, .. } if *list_id == source.list_id)) {
                        reasons.push(reason.clone());
                    }
                })
                .or_insert_with(|| (candidate, vec![reason]));
        }
    }
    let mut source_only = non_builtin.into_values().collect::<Vec<_>>();
    source_only.sort_by(|(left, _), (right, _)| {
        (
            left.last_contact_at.is_some(),
            left.last_contact_at,
            left.person.id.as_uuid(),
        )
            .cmp(&(
                right.last_contact_at.is_some(),
                right.last_contact_at,
                right.person.id.as_uuid(),
            ))
    });
    let list_candidate_count = source_only.len();
    let source_truncated = list_candidate_count > k;
    source_only.truncate(k);
    let list_items = source_only
        .into_iter()
        .map(|(candidate, mut reasons)| {
            reasons.sort_by_key(|reason| match reason {
                TodayReason::ListMember { list_id, .. } => list_id.as_uuid(),
                _ => Uuid::nil(),
            });
            let recommended_action = if candidate.person.primary_phone.is_some() {
                RecommendedAction::Call
            } else if candidate.person.primary_email.is_some() {
                RecommendedAction::Email
            } else {
                RecommendedAction::ReviewPerson
            };
            TodayItem {
                person: candidate.person,
                priority: TodayPriority::List,
                recommended_action,
                reasons,
                waiting_since: None,
                latest_inquiry: candidate.latest_inquiry,
                last_contact_attempt: candidate.last_contact_attempt,
            }
        })
        .collect::<Vec<_>>();
    let list_item_count = list_items.len();
    // Admission reserves every retained built-in. Display then places the
    // new list-only band between normal work and outcome-needed calls.
    let mut final_items = Vec::with_capacity(builtins.len() + list_items.len());
    for priority in [TodayPriority::High, TodayPriority::Normal] {
        final_items.extend(
            builtins
                .iter()
                .filter(|item| item.priority == priority)
                .cloned(),
        );
    }
    final_items.extend(list_items);
    final_items.extend(
        builtins
            .iter()
            .filter(|item| item.priority == TodayPriority::Low)
            .cloned(),
    );

    let status = if issues.is_empty() && system_feed_issues.is_empty() {
        TodaySourcesStatus::Complete
    } else {
        TodaySourcesStatus::Partial
    };
    let list = TodayList {
        generated_at: now,
        items: final_items,
        truncated: builtin_truncated || source_truncated,
        sources: TodaySources {
            status,
            issues,
            system_feed_issues,
        },
    };
    let commit_deadline =
        final_recovery_deadline.unwrap_or_else(|| Instant::now() + RECOVERY_BUDGET);
    let commit_budget = commit_deadline.checked_duration_since(Instant::now());
    let connection_healthy = connection_healthy
        && match commit_budget {
            Some(remaining) => commit_within(tx, remaining, commit_deadline).await,
            None => false,
        };
    Ok(QueryOutcome {
        list,
        connection_healthy,
        telemetry: QueryTelemetry {
            builtin_candidate_count,
            builtin_truncated,
            metadata_outcome: SourceMetadataOutcome::Complete,
            enabled_source_count: Some(enabled_source_count),
            successful_source_count: Some(successful_source_count),
            failed_source_count: Some(failed_source_count),
            list_candidate_count: Some(list_candidate_count),
            list_item_count: Some(list_item_count),
            list_truncated: Some(source_truncated),
            feed_statuses,
            person_state_candidate_count,
            call_candidate_count,
        },
    })
}

/// Production entry point: the caller transfers its only pooled connection
/// into Today. An outer Operator timeout or an unrecoverable source cleanup
/// therefore drops/detaches it instead of returning a transaction-poisoned
/// connection to the pool. Unit callers retain [`query`] over a bare
/// connection for focused query tests.
pub async fn query_owned(
    connection: PoolConnection<Postgres>,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    _now: DateTime<Utc>,
) -> Result<TodayList, sqlx::Error> {
    query_owned_with_clock(connection, scope, viewer, EvaluationClock::Database).await
}

/// Test-only owned-connection equivalent of [`query_at`]. It retains the
/// normal ownership guard so cancellation and cleanup tests cannot bypass
/// connection disposal while exercising a fixed fixture clock.
#[cfg(feature = "test-support")]
pub async fn query_owned_at(
    connection: PoolConnection<Postgres>,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    now: DateTime<Utc>,
) -> Result<TodayList, sqlx::Error> {
    query_owned_with_clock(connection, scope, viewer, EvaluationClock::Fixed(now)).await
}

async fn query_owned_with_clock(
    connection: PoolConnection<Postgres>,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    evaluation_clock: EvaluationClock,
) -> Result<TodayList, sqlx::Error> {
    struct Guard(Option<PoolConnection<Postgres>>);
    impl Guard {
        fn connection(&mut self) -> &mut PgConnection {
            self.0
                .as_deref_mut()
                .expect("today connection guard is armed")
        }
        fn disarm(&mut self) {
            let _ = self.0.take();
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some(connection) = self.0.take() {
                // `detach` removes the permit and drops the raw socket now;
                // `close_on_drop` has a five-second graceful timeout and is
                // not suitable for the source recovery bound.
                drop(connection.detach());
            }
        }
    }

    let mut guard = Guard(Some(connection));
    let result = query_inner(guard.connection(), scope, viewer, evaluation_clock).await;
    if matches!(
        &result,
        Ok(QueryOutcome {
            connection_healthy: true,
            ..
        })
    ) {
        guard.disarm();
    }
    result.map(|outcome| outcome.list)
}

const ENUMERATION_BUDGET: Duration = Duration::from_millis(250);
const SOURCE_BUDGET: Duration = Duration::from_millis(500);
const RECOVERY_BUDGET: Duration = Duration::from_millis(100);
// The wall-clock cap remains the contract boundary. Leave a small slice of
// it for PostgreSQL to deliver a server-side cancellation and for the
// savepoint rollback/release, rather than racing the outer Tokio timeout and
// unnecessarily discarding an otherwise recoverable owned connection.
const SERVER_CANCELLATION_HEADROOM: Duration = Duration::from_millis(10);

struct SourceQueryContext<'a> {
    source_id: Uuid,
    organization_id: crate::ids::OrganizationId,
    viewer: UserId,
    now: DateTime<Utc>,
    builtin_ids: &'a [Uuid],
    builtin_truncated: bool,
    builtin_len: usize,
    deadline: Instant,
}

enum SourceEvaluation {
    Data {
        members: Vec<Uuid>,
        prefix: Vec<sources::SourceCandidate>,
    },
    InvalidStage,
    InvalidAssignee,
}

async fn evaluate_source(
    conn: &mut PgConnection,
    filter: &crate::domain::person::filter::FilterDefinition,
    context: SourceQueryContext<'_>,
) -> Result<SourceEvaluation, sqlx::Error> {
    let SourceQueryContext {
        source_id,
        organization_id,
        viewer,
        now,
        builtin_ids,
        builtin_truncated,
        builtin_len,
        deadline,
    } = context;
    #[cfg(not(feature = "test-support"))]
    let _ = source_id;
    set_source_statement_timeout_until(conn, deadline).await?;
    sqlx::query("SAVEPOINT today_source")
        .execute(&mut *conn)
        .await?;
    #[cfg(feature = "test-support")]
    test_support::checkpoint(
        test_support::TodayQueryPhase::SourceAfterSavepoint,
        Some(source_id),
        Some(deadline),
        conn,
    )
    .await?;
    match filter
        .validate_references_until(conn, organization_id, deadline)
        .await
    {
        Ok(()) => {}
        Err(crate::domain::person::filter::FilterError::InvalidStage) => {
            set_source_statement_timeout_until(conn, deadline).await?;
            sqlx::query("RELEASE SAVEPOINT today_source")
                .execute(&mut *conn)
                .await?;
            return Ok(SourceEvaluation::InvalidStage);
        }
        Err(crate::domain::person::filter::FilterError::InvalidAssignee) => {
            set_source_statement_timeout_until(conn, deadline).await?;
            sqlx::query("RELEASE SAVEPOINT today_source")
                .execute(&mut *conn)
                .await?;
            return Ok(SourceEvaluation::InvalidAssignee);
        }
        Err(crate::domain::person::filter::FilterError::Database(error)) => return Err(error),
        Err(crate::domain::person::filter::FilterError::Malformed) => {
            return Err(sqlx::Error::Decode(
                "today source filter became malformed".into(),
            ))
        }
    }

    let params = filter.to_query_params(viewer);
    let members = if builtin_ids.is_empty() {
        Vec::new()
    } else {
        set_source_statement_timeout_until(conn, deadline).await?;
        sources::source_membership(conn, organization_id, &params, now, builtin_ids).await?
    };
    #[cfg(feature = "test-support")]
    test_support::checkpoint(
        test_support::TodayQueryPhase::SourceAfterMembership,
        Some(source_id),
        Some(deadline),
        conn,
    )
    .await?;
    let k = 200usize.saturating_sub(builtin_len);
    let prefix = if builtin_truncated {
        Vec::new()
    } else {
        set_source_statement_timeout_until(conn, deadline).await?;
        sources::source_candidates(
            conn,
            organization_id,
            &params,
            now,
            builtin_ids,
            false,
            i64::try_from(k.saturating_add(1)).unwrap_or(201),
        )
        .await?
    };
    #[cfg(feature = "test-support")]
    test_support::checkpoint(
        test_support::TodayQueryPhase::BeforeSourceRelease,
        Some(source_id),
        Some(deadline),
        conn,
    )
    .await?;
    set_source_statement_timeout_until(conn, deadline).await?;
    sqlx::query("RELEASE SAVEPOINT today_source")
        .execute(&mut *conn)
        .await?;
    Ok(SourceEvaluation::Data { members, prefix })
}

/// [`evaluate_feeds_builtins`]'s result: `(items, truncated,
/// candidate_count)`, plus any `system_feed_issues`, plus the two
/// call-feed recovery signals `query_inner_untraced` must fold into its own
/// connection-health/final-commit bookkeeping exactly as a list source's
/// recovery does.
struct FeedsBuiltins {
    items: Vec<TodayItem>,
    truncated: bool,
    candidate_count: usize,
    system_feed_issues: Vec<SystemFeedIssue>,
    /// `Some` iff the call feed failed and recovered — the shared 100 ms
    /// recovery grace's remaining deadline, to seed the caller's
    /// `final_recovery_deadline` (recoveries share one budget, never stack).
    call_feed_recovery_deadline: Option<Instant>,
    /// The call feed failed and its savepoint rollback ALSO could not
    /// complete inside the grace — the connection's state past that point
    /// is unknown. The caller must skip all list-source work and report
    /// the whole Today response unavailable (mirrors a source-metadata
    /// failure that itself fails to recover).
    call_feed_unrecoverable: bool,
    /// docs/specs/SLICE_011d.md §8: per-feed status for `today.query`'s
    /// span, in `system_feeds::ALL_FEED_KEYS` order — never the definition
    /// itself, only its classification.
    feed_statuses: Option<[(&'static str, &'static str); 3]>,
    /// The person-state statement's candidate count (spec §8/§9.9).
    person_state_candidate_count: Option<usize>,
    /// The call feed's evaluated candidate count (its `call_only`
    /// statement), present only when the call feed is enabled and its
    /// evaluation succeeded.
    call_candidate_count: Option<usize>,
}

/// The `Feeds` provider's builtins computation (docs/specs/SLICE_011d.md
/// §5): loads the three feed rows, evaluates the person-state statement,
/// ranks it with the UNCHANGED [`rank`] function, then evaluates the call
/// feed under its own savepoint with the 011c 500 ms whole-source budget —
/// the SAME connection-recovery mechanism as a list source (§4 of the
/// correction round): actual SQL cancellation via a tight
/// `statement_timeout`, a savepoint rollback under the shared 100 ms grace,
/// and (only if that recovery itself fails) an unrecoverable signal that
/// makes the caller skip list-source work and detach the connection rather
/// than pool it — never a poisoned connection returned to the pool or
/// corrupted later list-source work. A feed-load or person-state failure
/// still propagates as `sqlx::Error` — a 503 at the caller, exactly like a
/// built-in failure today (spec §5 step 2); only the call feed's own
/// failure is caught and reported as a `system_feed_issues` entry with
/// available work returned (D-047).
///
/// All-or-nothing like a list source's `evaluate_source`: the call feed's
/// two statements' results are collected locally and only merged into
/// `builtins` after BOTH succeed — never an uncertain partial call-feed
/// result (`call_membership` succeeding while `call_only` then fails must
/// not leave a half-applied set of `call_outcome_needed` reasons).
fn feed_status_label(feed: &system_feeds::ResolvedFeed) -> &'static str {
    if !feed.enabled {
        "disabled"
    } else if feed.fallback {
        "fallback"
    } else if feed.is_default {
        "default"
    } else {
        "customized"
    }
}

async fn evaluate_feeds_builtins(
    tx: &mut PgConnection,
    scope: &PersonVisibilityScope,
    viewer: UserId,
    now: DateTime<Utc>,
) -> Result<FeedsBuiltins, sqlx::Error> {
    let organization_id = scope.organization_id();
    let feeds = system_feeds::load_feed_rows(tx, organization_id).await?;

    // Spec §5: "a disabled feed contributes nothing and no issue" — this
    // covers fallback reporting too. A disabled feed's invalid stored rule
    // belongs on the admin page via `Feed.filter_error` (step 4), never here.
    let mut system_feed_issues = Vec::new();
    for feed in &feeds {
        if feed.fallback && feed.enabled {
            system_feed_issues.push(SystemFeedIssue {
                feed_key: feed.feed_key.as_str(),
                error: SystemFeedIssueError::InvalidDefinition,
                fallback: true,
            });
        }
    }

    // `system_feeds::ALL_FEED_KEYS` fixes this order: unanswered_inquiry,
    // client_replied, call_outcome_needed.
    let feed_unanswered = &feeds[0];
    let feed_replied = &feeds[1];
    let feed_call = &feeds[2];
    debug_assert_eq!(
        feed_unanswered.feed_key,
        system_feeds::FeedKey::UnansweredInquiry
    );
    debug_assert_eq!(feed_replied.feed_key, system_feeds::FeedKey::ClientReplied);
    debug_assert_eq!(feed_call.feed_key, system_feeds::FeedKey::CallOutcomeNeeded);

    // docs/specs/SLICE_011d.md §8/§9.9: a per-feed CLASSIFICATION only
    // (never the definition itself) for `today.query`'s span.
    let feed_statuses: [(&'static str, &'static str); 3] = [
        (
            feed_unanswered.feed_key.as_str(),
            feed_status_label(feed_unanswered),
        ),
        (
            feed_replied.feed_key.as_str(),
            feed_status_label(feed_replied),
        ),
        (feed_call.feed_key.as_str(), feed_status_label(feed_call)),
    ];

    let (candidates, truncated_p) = system_feeds::evaluate::person_state_candidates(
        tx,
        organization_id,
        viewer,
        now,
        feed_unanswered,
        feed_replied,
    )
    .await?;
    let builtin_candidate_count = candidates.len();
    let mut builtins = rank(candidates, now);
    let retained_ids: Vec<Uuid> = builtins
        .iter()
        .map(|item| item.person.id.as_uuid())
        .collect();

    let mut truncated_call = false;
    let mut call_feed_recovery_deadline = None;
    let mut call_candidate_count: Option<usize> = None;
    if feed_call.enabled {
        // The savepoint's own creation must be inside the SAME recoverable
        // region as every statement after it: `evaluate_source` follows the
        // identical shape (its first statement, also a SAVEPOINT, runs
        // inside the `tokio::time::timeout`-wrapped call). A failure at any
        // point — including here — must surface as `Ok(Err(_))` from this
        // block, never propagate a raw `sqlx::Error` straight out of
        // `evaluate_feeds_builtins` past the recovery logic below.
        let call_deadline = Instant::now() + SOURCE_BUDGET;
        let outcome = tokio::time::timeout(SOURCE_BUDGET, async {
            sqlx::query("SAVEPOINT today_call_feed")
                .execute(&mut *tx)
                .await?;
            #[cfg(feature = "test-support")]
            test_support::checkpoint(
                test_support::TodayQueryPhase::CallFeedAfterSavepoint,
                None,
                Some(call_deadline),
                tx,
            )
            .await?;
            set_source_statement_timeout_until(tx, call_deadline).await?;
            let membership = system_feeds::evaluate::call_membership(
                tx,
                organization_id,
                viewer,
                feed_call,
                &retained_ids,
                now,
            )
            .await?;
            #[cfg(feature = "test-support")]
            test_support::checkpoint(
                test_support::TodayQueryPhase::CallFeedAfterMembership,
                None,
                Some(call_deadline),
                tx,
            )
            .await?;
            // Only if the person-state statement was NOT truncated (spec
            // §5 step 4b) — never admit a discarded person-state row.
            let call_only = if truncated_p {
                Vec::new()
            } else {
                let k = 200usize.saturating_sub(builtins.len());
                let limit = i64::try_from(k.saturating_add(1)).unwrap_or(201);
                set_source_statement_timeout_until(tx, call_deadline).await?;
                system_feeds::evaluate::call_only_candidates(
                    tx,
                    organization_id,
                    viewer,
                    feed_call,
                    &retained_ids,
                    limit,
                    now,
                )
                .await?
            };
            #[cfg(feature = "test-support")]
            test_support::checkpoint(
                test_support::TodayQueryPhase::BeforeCallFeedRelease,
                None,
                Some(call_deadline),
                tx,
            )
            .await?;
            set_source_statement_timeout_until(tx, call_deadline).await?;
            sqlx::query("RELEASE SAVEPOINT today_call_feed")
                .execute(&mut *tx)
                .await?;
            Ok::<_, sqlx::Error>((membership, call_only))
        })
        .await;

        match outcome {
            Ok(Ok((membership, call_only))) => {
                for (person_id, call_id, ended_at) in membership {
                    system_feeds::evaluate::append_call_outcome_reason(
                        &mut builtins,
                        person_id,
                        call_id,
                        ended_at,
                    );
                }
                if !truncated_p {
                    call_candidate_count = Some(call_only.len());
                    let k = 200usize.saturating_sub(builtins.len());
                    truncated_call = call_only.len() > k;
                    let mut call_only_items = rank(call_only, now);
                    call_only_items.truncate(k);
                    builtins.extend(call_only_items);
                }
                // else: the call-only statement never ran (spec §5 step 4b —
                // the person-state statement was already truncated), so
                // `call_candidate_count` stays `None` rather than falsely
                // claiming zero candidates.
            }
            Ok(Err(_)) | Err(_) => {
                // Never merge an uncertain partial call-feed result (see the
                // function doc): nothing above mutated `builtins`, so a
                // failure here — at any point in the call feed's two
                // statements — leaves the person-state items exactly as
                // `rank()` produced them. Roll back the savepoint under the
                // shared 100 ms recovery grace; an unrecoverable rollback
                // marks the connection unhealthy and stops all further work
                // for this request, exactly like list-source recovery.
                system_feed_issues.push(SystemFeedIssue {
                    feed_key: feed_call.feed_key.as_str(),
                    error: SystemFeedIssueError::Unavailable,
                    fallback: false,
                });
                let recovery_started = Instant::now();
                let recovered = match recovery_remaining(recovery_started) {
                    Ok(remaining) => {
                        rollback_call_feed_within(tx, remaining, recovery_started + RECOVERY_BUDGET)
                            .await
                    }
                    Err(_) => false,
                };
                if recovered {
                    call_feed_recovery_deadline = Some(recovery_started + RECOVERY_BUDGET);
                } else {
                    return Ok(FeedsBuiltins {
                        items: builtins,
                        truncated: truncated_p,
                        candidate_count: builtin_candidate_count,
                        system_feed_issues,
                        call_feed_recovery_deadline: None,
                        call_feed_unrecoverable: true,
                        feed_statuses: Some(feed_statuses),
                        person_state_candidate_count: Some(builtin_candidate_count),
                        call_candidate_count: None,
                    });
                }
            }
        }
    }

    Ok(FeedsBuiltins {
        items: builtins,
        truncated: truncated_p || truncated_call,
        candidate_count: builtin_candidate_count,
        system_feed_issues,
        call_feed_recovery_deadline,
        call_feed_unrecoverable: false,
        feed_statuses: Some(feed_statuses),
        person_state_candidate_count: Some(builtin_candidate_count),
        call_candidate_count,
    })
}

async fn rollback_call_feed_within(
    conn: &mut PgConnection,
    budget: Duration,
    deadline: Instant,
) -> bool {
    #[cfg(not(feature = "test-support"))]
    let _ = deadline;
    matches!(
        tokio::time::timeout(budget, async {
            sqlx::query("ROLLBACK TO SAVEPOINT today_call_feed")
                .execute(&mut *conn)
                .await?;
            #[cfg(feature = "test-support")]
            test_support::checkpoint(
                test_support::TodayQueryPhase::RecoveryAfterRollback,
                None,
                Some(deadline),
                conn,
            )
            .await?;
            sqlx::query("RELEASE SAVEPOINT today_call_feed")
                .execute(&mut *conn)
                .await?;
            Ok::<(), sqlx::Error>(())
        })
        .await,
        Ok(Ok(()))
    )
}

async fn rollback_source_within(
    conn: &mut PgConnection,
    budget: Duration,
    source_id: Uuid,
    deadline: Instant,
) -> bool {
    #[cfg(not(feature = "test-support"))]
    let _ = (source_id, deadline);
    matches!(
        tokio::time::timeout(budget, async {
            sqlx::query("ROLLBACK TO SAVEPOINT today_source")
                .execute(&mut *conn)
                .await?;
            #[cfg(feature = "test-support")]
            test_support::checkpoint(
                test_support::TodayQueryPhase::RecoveryAfterRollback,
                Some(source_id),
                Some(deadline),
                conn,
            )
            .await?;
            sqlx::query("RELEASE SAVEPOINT today_source")
                .execute(&mut *conn)
                .await?;
            Ok::<(), sqlx::Error>(())
        })
        .await,
        Ok(Ok(()))
    )
}

async fn rollback_metadata_within(conn: &mut PgConnection, budget: Duration) -> bool {
    matches!(
        tokio::time::timeout(budget, async {
            sqlx::query("ROLLBACK TO SAVEPOINT today_source_metadata")
                .execute(&mut *conn)
                .await?;
            sqlx::query("RELEASE SAVEPOINT today_source_metadata")
                .execute(&mut *conn)
                .await?;
            Ok::<(), sqlx::Error>(())
        })
        .await,
        Ok(Ok(()))
    )
}

fn recovery_remaining(started: Instant) -> Result<Duration, sqlx::Error> {
    RECOVERY_BUDGET
        .checked_sub(started.elapsed())
        .ok_or_else(|| sqlx::Error::PoolTimedOut)
}

async fn commit_within(
    tx: sqlx::Transaction<'_, Postgres>,
    budget: Duration,
    deadline: Instant,
) -> bool {
    #[cfg(not(feature = "test-support"))]
    let _ = deadline;
    matches!(
        tokio::time::timeout(budget, async {
            #[allow(unused_mut)]
            let mut tx = tx;
            #[cfg(feature = "test-support")]
            test_support::checkpoint(
                test_support::TodayQueryPhase::BeforeFinalCommit,
                None,
                Some(deadline),
                &mut tx,
            )
            .await?;
            tx.commit().await
        })
        .await,
        Ok(Ok(()))
    )
}

async fn set_source_statement_timeout_until(
    conn: &mut PgConnection,
    deadline: Instant,
) -> Result<(), sqlx::Error> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(sqlx::Error::PoolTimedOut)?;
    let milliseconds = remaining
        .saturating_sub(SERVER_CANCELLATION_HEADROOM)
        .as_millis()
        .clamp(1, 500)
        .to_string();
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(milliseconds)
        .execute(conn)
        .await?;
    Ok(())
}

fn sources_status_label(status: TodaySourcesStatus) -> &'static str {
    match status {
        TodaySourcesStatus::Complete => "complete",
        TodaySourcesStatus::Partial => "partial",
        TodaySourcesStatus::Unavailable => "unavailable",
    }
}

fn source_metadata_outcome_label(outcome: SourceMetadataOutcome) -> &'static str {
    match outcome {
        SourceMetadataOutcome::Complete => "complete",
        SourceMetadataOutcome::Unavailable => "unavailable",
        SourceMetadataOutcome::TimedOut => "timed_out",
    }
}

fn record_optional_usize(span: &tracing::Span, field: &'static str, value: Option<usize>) {
    if let Some(value) = value {
        span.record(field, value);
    }
}

fn append_list_reason(items: &mut [TodayItem], person_id: Uuid, source: &sources::TodaySource) {
    let Some(item) = items
        .iter_mut()
        .find(|item| item.person.id.as_uuid() == person_id)
    else {
        return;
    };
    if item.reasons.iter().any(|reason| matches!(reason, TodayReason::ListMember { list_id, .. } if *list_id == source.list_id)) {
        return;
    }
    let reason = TodayReason::ListMember {
        list_id: source.list_id,
        name: source.name.clone(),
    };
    let position = item
        .reasons
        .iter()
        .position(|reason| matches!(reason, TodayReason::CallOutcomeNeeded { .. }))
        .unwrap_or(item.reasons.len());
    item.reasons.insert(position, reason);
}
