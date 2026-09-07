use std::{
    cmp::Ordering,
    collections::HashMap,
    env, fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use crm_app::{
    domain::person::filter::{FilterDefinition, FilterError, PersonFilterParams},
    ids::{OrganizationId, UserId},
};
use serde::Serialize;
use sqlx::{
    postgres::{PgArguments, PgPoolOptions},
    query::Query,
    types::Json,
    Acquire, Postgres, Row,
};
use tokio::sync::Barrier;
use uuid::Uuid;

const ORGANIZATION: &str = "11111111-1111-4111-8111-111111111111";
const CONCENTRATED: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1";
const REPRESENTATIVE: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2";
const PARTIAL: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa3";
const EMPTY: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa4";
const SOURCE_BUDGET: Duration = Duration::from_millis(500);
const METADATA_BUDGET: Duration = Duration::from_millis(250);
const ACQUIRE_BUDGET: Duration = Duration::from_secs(2);

const BUILTIN_SQL: &str = include_str!("../sql/builtin.sql");
const SOURCE_MEMBERSHIP_SQL: &str = include_str!("../sql/source_membership.sql");
const SOURCE_PREFIX_SQL: &str = include_str!("../sql/source_prefix.sql");
const METADATA_SQL: &str = include_str!("../sql/metadata.sql");

#[derive(Clone)]
struct Filter {
    label: String,
    stage_ids: Option<Vec<Uuid>>,
    assigned_user_ids: Option<Vec<Uuid>>,
    assigned_include_unassigned: Option<bool>,
    sources: Option<Vec<String>>,
    created_within_days: Option<i32>,
    created_not_within_days: Option<i32>,
    created_never: Option<bool>,
    last_inquiry_within_days: Option<i32>,
    last_inquiry_not_within_days: Option<i32>,
    last_inquiry_never: Option<bool>,
    last_contact_within_days: Option<i32>,
    last_contact_not_within_days: Option<i32>,
    last_contact_never: Option<bool>,
    last_inbound_within_days: Option<i32>,
    last_inbound_not_within_days: Option<i32>,
    last_inbound_never: Option<bool>,
    has_replied: Option<bool>,
    has_phone: Option<bool>,
    has_email: Option<bool>,
}

impl Filter {
    fn empty(label: &'static str) -> Self {
        Self {
            label: label.to_string(),
            stage_ids: None,
            assigned_user_ids: None,
            assigned_include_unassigned: None,
            sources: None,
            created_within_days: None,
            created_not_within_days: None,
            created_never: None,
            last_inquiry_within_days: None,
            last_inquiry_not_within_days: None,
            last_inquiry_never: None,
            last_contact_within_days: None,
            last_contact_not_within_days: None,
            last_contact_never: None,
            last_inbound_within_days: None,
            last_inbound_not_within_days: None,
            last_inbound_never: None,
            has_replied: None,
            has_phone: None,
            has_email: None,
        }
    }
    fn dense() -> Self {
        let mut f = Self::empty("dense_latest_source_zillow");
        f.sources = Some(vec!["zillow".to_string()]);
        f
    }
    fn absence_contact() -> Self {
        let mut f = Self::empty("absence_last_contact_never");
        f.last_contact_never = Some(true);
        f
    }
    fn absence_phone() -> Self {
        let mut f = Self::empty("absence_has_phone_false");
        f.has_phone = Some(false);
        f
    }
    fn absence_inquiry() -> Self {
        let mut f = Self::empty("absence_last_inquiry_never");
        f.last_inquiry_never = Some(true);
        f
    }
    fn absence_inbound() -> Self {
        let mut f = Self::empty("absence_last_inbound_never");
        f.last_inbound_never = Some(true);
        f
    }
    fn old_contact() -> Self {
        let mut f = Self::empty("absence_last_contact_not_within_10_days");
        f.last_contact_not_within_days = Some(10);
        f
    }
    fn dense_phone() -> Self {
        let mut f = Self::dense();
        f.label = "overlap_zillow_has_phone".to_string();
        f.has_phone = Some(true);
        f
    }
    fn dense_absence_contact() -> Self {
        let mut f = Self::dense();
        f.label = "overlap_zillow_last_contact_never".to_string();
        f.last_contact_never = Some(true);
        f
    }

    fn from_saved(label: String, params: PersonFilterParams) -> Self {
        Self {
            label,
            stage_ids: params.stage_ids,
            assigned_user_ids: params.assigned_user_ids,
            assigned_include_unassigned: params.assigned_include_unassigned,
            sources: params.sources,
            created_within_days: params.created_within_days,
            created_not_within_days: params.created_not_within_days,
            created_never: params.created_never,
            last_inquiry_within_days: params.last_inquiry_within_days,
            last_inquiry_not_within_days: params.last_inquiry_not_within_days,
            last_inquiry_never: params.last_inquiry_never,
            last_contact_within_days: params.last_contact_within_days,
            last_contact_not_within_days: params.last_contact_not_within_days,
            last_contact_never: params.last_contact_never,
            last_inbound_within_days: params.last_inbound_within_days,
            last_inbound_not_within_days: params.last_inbound_not_within_days,
            last_inbound_never: params.last_inbound_never,
            has_replied: params.has_replied,
            has_phone: params.has_phone,
            has_email: params.has_email,
        }
    }
}

#[derive(Clone)]
struct Case {
    label: String,
    viewer: Uuid,
    source_case: &'static str,
    source_count: usize,
}

#[derive(Clone)]
struct StoredSource {
    position: i32,
    filter: serde_json::Value,
}

#[derive(Serialize, Clone)]
struct SourceMetric {
    label: String,
    completed: bool,
    failure: Option<String>,
    total_ms: f64,
    validation_ms: Option<f64>,
    membership_ms: Option<f64>,
    prefix_with_hydration_ms: Option<f64>,
    matched_b: usize,
    hydrated_prefix_rows: usize,
}

#[derive(Serialize, Clone)]
struct RequestMetric {
    case: String,
    viewer_kind: String,
    source_count: usize,
    status: String,
    failure: Option<String>,
    request_ms: f64,
    pool_wait_ms: f64,
    builtins_ms: Option<f64>,
    metadata_ms: Option<f64>,
    b_rows: usize,
    builtins_truncated: bool,
    k: usize,
    list_candidates_before_cap: usize,
    source_metrics: Vec<SourceMetric>,
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn org() -> Uuid {
    Uuid::parse_str(ORGANIZATION).unwrap()
}

fn viewer_kind(viewer: Uuid) -> &'static str {
    if viewer == Uuid::parse_str(CONCENTRATED).unwrap() {
        "concentrated_spare_B"
    } else if viewer == Uuid::parse_str(REPRESENTATIVE).unwrap() {
        "representative_full_B"
    } else if viewer == Uuid::parse_str(PARTIAL).unwrap() {
        "partial_B"
    } else {
        "empty_B"
    }
}

fn bind_filter<'q>(
    q: Query<'q, Postgres, PgArguments>,
    filter: &Filter,
    now: DateTime<Utc>,
    builtin_ids: &[Uuid],
) -> Query<'q, Postgres, PgArguments> {
    q.persistent(true)
        .bind(org())
        .bind(filter.stage_ids.clone())
        .bind(filter.assigned_user_ids.clone())
        .bind(filter.assigned_include_unassigned)
        .bind(filter.sources.clone())
        .bind(filter.created_within_days)
        .bind(filter.created_not_within_days)
        .bind(filter.created_never)
        .bind(filter.last_inquiry_within_days)
        .bind(filter.last_inquiry_not_within_days)
        .bind(filter.last_inquiry_never)
        .bind(filter.last_contact_within_days)
        .bind(filter.last_contact_not_within_days)
        .bind(filter.last_contact_never)
        .bind(filter.last_inbound_within_days)
        .bind(filter.last_inbound_not_within_days)
        .bind(filter.last_inbound_never)
        .bind(filter.has_replied)
        .bind(filter.has_phone)
        .bind(filter.has_email)
        .bind(now)
        .bind(builtin_ids.to_vec())
}

async fn set_timeout(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    remaining: Duration,
) -> Result<(), sqlx::Error> {
    let millis = remaining.as_millis().max(1);
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .persistent(true)
        .bind(format!("{millis}ms"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn rollback_source(tx: &mut sqlx::Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query("ROLLBACK TO SAVEPOINT source_attempt")
        .execute(&mut **tx)
        .await?;
    sqlx::query("RELEASE SAVEPOINT source_attempt")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn error_kind(error: &sqlx::Error) -> &'static str {
    if error
        .as_database_error()
        .and_then(|database| database.code())
        .is_some_and(|code| code == "57014")
    {
        "statement_timeout"
    } else if matches!(error, sqlx::Error::PoolTimedOut) {
        "pool_timeout"
    } else {
        "source_error"
    }
}

async fn failed_source(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    label: String,
    started: Instant,
    failure: String,
) -> Result<(SourceMetric, Vec<(Uuid, Option<DateTime<Utc>>)>), sqlx::Error> {
    rollback_source(tx).await?;
    Ok((
        SourceMetric {
            label,
            completed: false,
            failure: Some(failure),
            total_ms: elapsed_ms(started),
            validation_ms: None,
            membership_ms: None,
            prefix_with_hydration_ms: None,
            matched_b: 0,
            hydrated_prefix_rows: 0,
        },
        Vec::new(),
    ))
}

async fn run_one_source(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    stored: &StoredSource,
    viewer: Uuid,
    now: DateTime<Utc>,
    builtin_ids: &[Uuid],
    prefix_limit: Option<i64>,
    force_timeout: bool,
) -> Result<(SourceMetric, Vec<(Uuid, Option<DateTime<Utc>>)>), sqlx::Error> {
    let started = Instant::now();
    let fallback_label = format!("source_{}", stored.position);
    sqlx::query("SAVEPOINT source_attempt")
        .execute(&mut **tx)
        .await?;
    if let Err(error) = set_timeout(tx, SOURCE_BUDGET).await {
        return failed_source(tx, fallback_label, started, error_kind(&error).to_string()).await;
    }

    let validation_started = Instant::now();
    let definition = match serde_json::from_value::<FilterDefinition>(stored.filter.clone()) {
        Ok(definition) if definition.validate().is_ok() => definition,
        _ => return failed_source(tx, fallback_label, started, "invalid_filter".to_string()).await,
    };
    if let Err(error) = definition
        .validate_references(&mut **tx, OrganizationId::new(org()))
        .await
    {
        let failure = match error {
            FilterError::InvalidStage => "invalid_stage".to_string(),
            FilterError::InvalidAssignee => "invalid_assignee".to_string(),
            FilterError::Database(error) => error_kind(&error).to_string(),
            FilterError::Malformed => "invalid_filter".to_string(),
        };
        return failed_source(tx, fallback_label, started, failure).await;
    }
    let filter = Filter::from_saved(
        definition.kinds_field(),
        definition.to_query_params(UserId::new(viewer)),
    );
    let validation_ms = elapsed_ms(validation_started);

    if force_timeout {
        if let Err(error) = set_timeout(tx, SOURCE_BUDGET).await {
            return failed_source(
                tx,
                filter.label.clone(),
                started,
                error_kind(&error).to_string(),
            )
            .await;
        }
        if let Err(error) = sqlx::query("SELECT pg_sleep(0.75)")
            .persistent(true)
            .execute(&mut **tx)
            .await
        {
            let failure = if error_kind(&error) == "statement_timeout" {
                "forced_statement_timeout".to_string()
            } else {
                error_kind(&error).to_string()
            };
            return failed_source(tx, filter.label.clone(), started, failure).await;
        }
    }

    let membership_started = Instant::now();
    let matched_b = if builtin_ids.is_empty() {
        Vec::new()
    } else {
        let remaining = SOURCE_BUDGET.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return failed_source(
                tx,
                filter.label.clone(),
                started,
                "source_budget_exhausted".to_string(),
            )
            .await;
        }
        if let Err(error) = set_timeout(tx, remaining).await {
            return failed_source(
                tx,
                filter.label.clone(),
                started,
                error_kind(&error).to_string(),
            )
            .await;
        }
        match bind_filter(
            sqlx::query(SOURCE_MEMBERSHIP_SQL),
            &filter,
            now,
            builtin_ids,
        )
        .fetch_all(&mut **tx)
        .await
        {
            Ok(rows) => rows,
            Err(error) => {
                return failed_source(
                    tx,
                    filter.label.clone(),
                    started,
                    error_kind(&error).to_string(),
                )
                .await
            }
        }
    };
    let membership_ms = (!builtin_ids.is_empty()).then(|| elapsed_ms(membership_started));

    let prefix_started = Instant::now();
    let prefix = if let Some(limit) = prefix_limit {
        let remaining = SOURCE_BUDGET.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return failed_source(
                tx,
                filter.label.clone(),
                started,
                "source_budget_exhausted".to_string(),
            )
            .await;
        }
        if let Err(error) = set_timeout(tx, remaining).await {
            return failed_source(
                tx,
                filter.label.clone(),
                started,
                error_kind(&error).to_string(),
            )
            .await;
        }
        match bind_filter(sqlx::query(SOURCE_PREFIX_SQL), &filter, now, builtin_ids)
            .bind(limit)
            .fetch_all(&mut **tx)
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                return failed_source(
                    tx,
                    filter.label.clone(),
                    started,
                    error_kind(&error).to_string(),
                )
                .await
            }
        }
    } else {
        Vec::new()
    };
    let prefix_ms = prefix_limit.map(|_| elapsed_ms(prefix_started));
    let values = prefix
        .into_iter()
        .map(|row| {
            Ok::<_, sqlx::Error>((
                row.try_get::<Uuid, _>("id")?,
                row.try_get::<Option<DateTime<Utc>>, _>("last_contact_at")?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;

    sqlx::query("RELEASE SAVEPOINT source_attempt")
        .execute(&mut **tx)
        .await?;
    Ok((
        SourceMetric {
            label: filter.label.clone(),
            completed: true,
            failure: None,
            total_ms: elapsed_ms(started),
            validation_ms: Some(validation_ms),
            membership_ms,
            prefix_with_hydration_ms: prefix_ms,
            matched_b: matched_b.len(),
            hydrated_prefix_rows: values.len(),
        },
        values,
    ))
}

fn request_failure(
    case: &Case,
    started: Instant,
    pool_wait_ms: f64,
    failure: &str,
) -> RequestMetric {
    RequestMetric {
        case: case.label.clone(),
        viewer_kind: viewer_kind(case.viewer).to_string(),
        source_count: case.source_count,
        status: "failed".to_string(),
        failure: Some(failure.to_string()),
        request_ms: elapsed_ms(started),
        pool_wait_ms,
        builtins_ms: None,
        metadata_ms: None,
        b_rows: 0,
        builtins_truncated: false,
        k: 0,
        list_candidates_before_cap: 0,
        source_metrics: Vec::new(),
    }
}

async fn run_request(pool: &sqlx::PgPool, case: &Case, force_first_timeout: bool) -> RequestMetric {
    let started = Instant::now();
    let acquired = Instant::now();
    let mut conn = match pool.acquire().await {
        Ok(conn) => conn,
        Err(error) => {
            return request_failure(case, started, elapsed_ms(acquired), error_kind(&error))
        }
    };
    let pool_wait_ms = elapsed_ms(acquired);
    let mut tx = match conn.begin().await {
        Ok(tx) => tx,
        Err(_) => return request_failure(case, started, pool_wait_ms, "transaction_start_failed"),
    };
    if sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .is_err()
    {
        return request_failure(case, started, pool_wait_ms, "transaction_setup_failed");
    }
    if env::var("PERF_JIT_OFF").ok().as_deref() == Some("1") {
        if sqlx::query("SET LOCAL jit = off")
            .execute(&mut *tx)
            .await
            .is_err()
        {
            return request_failure(case, started, pool_wait_ms, "jit_setup_failed");
        }
    }
    let now: DateTime<Utc> = match sqlx::query_scalar("SELECT transaction_timestamp()")
        .persistent(true)
        .fetch_one(&mut *tx)
        .await
    {
        Ok(now) => now,
        Err(_) => return request_failure(case, started, pool_wait_ms, "clock_failed"),
    };

    let builtin_started = Instant::now();
    let builtin_rows = match sqlx::query(BUILTIN_SQL)
        .persistent(true)
        .bind(org())
        .bind(case.viewer)
        .bind(now)
        .fetch_all(&mut *tx)
        .await
    {
        Ok(rows) => rows,
        Err(_) => return request_failure(case, started, pool_wait_ms, "builtin_failed"),
    };
    let builtins_ms = elapsed_ms(builtin_started);
    let builtins_truncated = builtin_rows.len() > 200;
    let mut builtin_ids = match builtin_rows
        .iter()
        .take(200)
        .map(|row| row.try_get::<Uuid, _>("id"))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(ids) => ids,
        Err(_) => return request_failure(case, started, pool_wait_ms, "builtin_decode_failed"),
    };
    builtin_ids.sort_unstable();
    let k = 200usize.saturating_sub(builtin_ids.len());

    let metadata_started = Instant::now();
    if let Err(_) = set_timeout(&mut tx, METADATA_BUDGET).await {
        return request_failure(case, started, pool_wait_ms, "metadata_timeout_setup_failed");
    }
    let metadata_rows = match sqlx::query(METADATA_SQL)
        .persistent(true)
        .bind(org())
        .bind(case.viewer)
        .bind(case.source_case)
        .fetch_all(&mut *tx)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            let _ = tx.commit().await;
            return RequestMetric {
                case: case.label.clone(),
                viewer_kind: viewer_kind(case.viewer).to_string(),
                source_count: case.source_count,
                status: "unavailable".to_string(),
                failure: Some(error_kind(&error).to_string()),
                request_ms: elapsed_ms(started),
                pool_wait_ms,
                builtins_ms: Some(builtins_ms),
                metadata_ms: Some(elapsed_ms(metadata_started)),
                b_rows: builtin_ids.len(),
                builtins_truncated,
                k,
                list_candidates_before_cap: 0,
                source_metrics: Vec::new(),
            };
        }
    };
    let metadata_ms = elapsed_ms(metadata_started);
    let decoded_sources = metadata_rows
        .into_iter()
        .map(|row| {
            Ok::<_, sqlx::Error>(StoredSource {
                position: row.try_get("position")?,
                filter: row.try_get::<Json<serde_json::Value>, _>("filter")?.0,
            })
        })
        .collect::<Result<Vec<_>, _>>();
    let stored_sources = match decoded_sources {
        Ok(sources) if sources.len() == case.source_count => sources,
        Ok(sources) => {
            eprintln!(
                "metadata row count {} expected {}",
                sources.len(),
                case.source_count
            );
            return request_failure(case, started, pool_wait_ms, "metadata_mismatch");
        }
        Err(error) => {
            let _ = error;
            return request_failure(case, started, pool_wait_ms, "metadata_mismatch");
        }
    };
    let prefix_limit = if k > 0 {
        Some((k + 1) as i64)
    } else if !builtins_truncated {
        Some(1)
    } else {
        None
    };

    let mut source_metrics = Vec::with_capacity(stored_sources.len());
    let mut candidates: HashMap<Uuid, Option<DateTime<Utc>>> = HashMap::new();
    for (index, stored) in stored_sources.iter().enumerate() {
        match run_one_source(
            &mut tx,
            stored,
            case.viewer,
            now,
            &builtin_ids,
            prefix_limit,
            force_first_timeout && index == 0,
        )
        .await
        {
            Ok((metric, prefix)) => {
                if metric.completed {
                    for (id, contact) in prefix {
                        candidates.entry(id).or_insert(contact);
                    }
                }
                source_metrics.push(metric);
            }
            Err(_) => {
                return request_failure(case, started, pool_wait_ms, "source_recovery_failed")
            }
        }
    }
    let mut ordered = candidates.into_iter().collect::<Vec<_>>();
    ordered.sort_by(
        |(left_id, left_at), (right_id, right_at)| match (left_at, right_at) {
            (None, None) => left_id.cmp(right_id),
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(left), Some(right)) => left.cmp(right).then_with(|| left_id.cmp(right_id)),
        },
    );
    let source_failed = source_metrics.iter().any(|metric| !metric.completed);
    let list_candidates_before_cap = ordered.len();
    if tx.commit().await.is_err() {
        return request_failure(case, started, pool_wait_ms, "commit_failed");
    }
    RequestMetric {
        case: case.label.clone(),
        viewer_kind: viewer_kind(case.viewer).to_string(),
        source_count: case.source_count,
        status: if source_failed { "partial" } else { "complete" }.to_string(),
        failure: None,
        request_ms: elapsed_ms(started),
        pool_wait_ms,
        builtins_ms: Some(builtins_ms),
        metadata_ms: Some(metadata_ms),
        b_rows: builtin_ids.len(),
        builtins_truncated,
        k,
        list_candidates_before_cap,
        source_metrics,
    }
}

#[derive(Serialize)]
struct Summary {
    case: String,
    viewer_kind: String,
    source_count: usize,
    concurrency: usize,
    requested: usize,
    complete: usize,
    partial: usize,
    unavailable: usize,
    failed: usize,
    request_p50_ms: Option<f64>,
    request_p95_ms: Option<f64>,
    pool_wait_p50_ms: Option<f64>,
    pool_wait_p95_ms: Option<f64>,
    builtins_p50_ms: Option<f64>,
    builtins_p95_ms: Option<f64>,
    source_p50_ms: Option<f64>,
    source_p95_ms: Option<f64>,
    source_timeouts: usize,
    metadata_p95_ms: Option<f64>,
}

#[derive(Serialize)]
struct Report {
    prototype: &'static str,
    mode: String,
    pool_max_connections: usize,
    source_budget_ms: u64,
    metadata_budget_ms: u64,
    acquire_budget_ms: u64,
    summaries: Vec<Summary>,
    forced_timeout_recovery: RequestMetric,
}

fn percentile(mut values: Vec<f64>, p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let index = (((values.len() - 1) as f64) * p).ceil() as usize;
    values.get(index).copied()
}

fn summary(metrics: &[RequestMetric], concurrency: usize) -> Summary {
    let source_times = metrics
        .iter()
        .flat_map(|metric| metric.source_metrics.iter())
        .map(|source| source.total_ms)
        .collect::<Vec<_>>();
    Summary {
        case: metrics
            .first()
            .map(|metric| metric.case.clone())
            .unwrap_or_default(),
        viewer_kind: metrics
            .first()
            .map(|metric| metric.viewer_kind.clone())
            .unwrap_or_default(),
        source_count: metrics
            .first()
            .map(|metric| metric.source_count)
            .unwrap_or(0),
        concurrency,
        requested: metrics.len(),
        complete: metrics
            .iter()
            .filter(|metric| metric.status == "complete")
            .count(),
        partial: metrics
            .iter()
            .filter(|metric| metric.status == "partial")
            .count(),
        unavailable: metrics
            .iter()
            .filter(|metric| metric.status == "unavailable")
            .count(),
        failed: metrics
            .iter()
            .filter(|metric| metric.status == "failed")
            .count(),
        request_p50_ms: percentile(
            metrics.iter().map(|metric| metric.request_ms).collect(),
            0.50,
        ),
        request_p95_ms: percentile(
            metrics.iter().map(|metric| metric.request_ms).collect(),
            0.95,
        ),
        pool_wait_p50_ms: percentile(
            metrics.iter().map(|metric| metric.pool_wait_ms).collect(),
            0.50,
        ),
        pool_wait_p95_ms: percentile(
            metrics.iter().map(|metric| metric.pool_wait_ms).collect(),
            0.95,
        ),
        builtins_p50_ms: percentile(
            metrics
                .iter()
                .filter_map(|metric| metric.builtins_ms)
                .collect(),
            0.50,
        ),
        builtins_p95_ms: percentile(
            metrics
                .iter()
                .filter_map(|metric| metric.builtins_ms)
                .collect(),
            0.95,
        ),
        source_p50_ms: percentile(source_times.clone(), 0.50),
        source_p95_ms: percentile(source_times, 0.95),
        source_timeouts: metrics
            .iter()
            .flat_map(|metric| metric.source_metrics.iter())
            .filter(|source| {
                source.failure.as_deref().is_some_and(|failure| {
                    failure.contains("timeout") || failure.contains("budget")
                })
            })
            .count(),
        metadata_p95_ms: percentile(
            metrics
                .iter()
                .filter_map(|metric| metric.metadata_ms)
                .collect(),
            0.95,
        ),
    }
}

async fn wave(pool: sqlx::PgPool, case: Case, concurrency: usize) -> Vec<RequestMetric> {
    let gate = Arc::new(Barrier::new(concurrency));
    let mut tasks = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let pool = pool.clone();
        let case = case.clone();
        let gate = gate.clone();
        tasks.push(tokio::spawn(async move {
            gate.wait().await;
            run_request(&pool, &case, false).await
        }));
    }
    let mut metrics = Vec::with_capacity(concurrency);
    for task in tasks {
        if let Ok(metric) = task.await {
            metrics.push(metric);
        }
    }
    metrics
}

async fn warm(pool: sqlx::PgPool, case: Case) {
    // sqlx keeps prepared statements per connection. PostgreSQL's auto plan
    // selection considers a generic plan after five custom executions, so use
    // six waves to warm each of the ten pool connections before recording it.
    for _ in 0..6 {
        let _ = wave(pool.clone(), case.clone(), 10).await;
    }
}

fn case(label: &str, viewer: Uuid, source_case: &'static str, source_count: usize) -> Case {
    Case {
        label: label.to_string(),
        viewer,
        source_case,
        source_count,
    }
}

fn all_cases() -> Vec<Case> {
    let concentrated = Uuid::parse_str(CONCENTRATED).unwrap();
    let representative = Uuid::parse_str(REPRESENTATIVE).unwrap();
    let partial = Uuid::parse_str(PARTIAL).unwrap();
    let empty = Uuid::parse_str(EMPTY).unwrap();
    vec![
        case("zero_sources_concentrated_spare_B", concentrated, "zero", 0),
        case(
            "one_dense_concentrated_spare_B",
            concentrated,
            "one_dense",
            1,
        ),
        case(
            "one_absence_concentrated_spare_B",
            concentrated,
            "one_absence",
            1,
        ),
        case(
            "five_overlap_concentrated_spare_B",
            concentrated,
            "five_overlap",
            5,
        ),
        case(
            "zero_sources_representative_full_B",
            representative,
            "zero",
            0,
        ),
        case(
            "one_dense_representative_full_B",
            representative,
            "one_dense",
            1,
        ),
        case(
            "one_absence_representative_full_B",
            representative,
            "one_absence",
            1,
        ),
        case(
            "five_overlap_representative_full_B",
            representative,
            "five_overlap",
            5,
        ),
        case("five_overlap_partial_B", partial, "five_overlap", 5),
        case("five_overlap_empty_B", empty, "five_overlap", 5),
    ]
}

async fn run_mode(pool: sqlx::PgPool, mode: &str) -> Vec<Summary> {
    let cases = all_cases();
    let critical = cases.iter().take(4).cloned().collect::<Vec<_>>();
    let selected = if mode == "critical" { critical } else { cases };
    let mut summaries = Vec::new();
    for current in selected {
        warm(pool.clone(), current.clone()).await;
        let sequential = if mode == "critical" { 8 } else { 5 };
        let mut serial = Vec::new();
        for _ in 0..sequential {
            serial.push(run_request(&pool, &current, false).await);
        }
        summaries.push(summary(&serial, 1));
        let waves = if mode == "critical" { 2 } else { 1 };
        for concurrency in [10, 20] {
            let mut concurrent = Vec::new();
            for _ in 0..waves {
                concurrent.extend(wave(pool.clone(), current.clone(), concurrency).await);
            }
            summaries.push(summary(&concurrent, concurrency));
        }
    }
    summaries
}

async fn capture_plans(
    pool: &sqlx::PgPool,
    artifact_dir: &PathBuf,
    disable_jit: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = pool.acquire().await?;
    let mut tx = conn.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    if disable_jit {
        sqlx::query("SET LOCAL jit = off").execute(&mut *tx).await?;
    }
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT transaction_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    let builtin_rows = sqlx::query(BUILTIN_SQL)
        .persistent(true)
        .bind(org())
        .bind(Uuid::parse_str(CONCENTRATED)?)
        .bind(now)
        .fetch_all(&mut *tx)
        .await?;
    let mut builtin_ids = builtin_rows
        .iter()
        .take(200)
        .map(|row| row.try_get::<Uuid, _>("id"))
        .collect::<Result<Vec<_>, _>>()?;
    builtin_ids.sort_unstable();

    let builtin_explain = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {BUILTIN_SQL}");
    let builtin_plan: Json<serde_json::Value> = sqlx::query(&builtin_explain)
        .bind(org())
        .bind(Uuid::parse_str(CONCENTRATED)?)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?
        .try_get(0)?;
    let suffix = if disable_jit { "jit_off" } else { "default" };
    fs::write(
        artifact_dir
            .join("plans")
            .join(format!("builtin_concentrated_{suffix}.json")),
        serde_json::to_vec_pretty(&builtin_plan.0)?,
    )?;

    for (case_key, filename) in [
        ("one_dense", "source_dense_non_B"),
        ("one_absence", "source_absence_non_B"),
    ] {
        let source_row = sqlx::query(METADATA_SQL)
            .bind(org())
            .bind(Uuid::parse_str(CONCENTRATED)?)
            .bind(case_key)
            .fetch_one(&mut *tx)
            .await?;
        let value: Json<serde_json::Value> = source_row.try_get("filter")?;
        let definition = serde_json::from_value::<FilterDefinition>(value.0)?;
        let filter = Filter::from_saved(
            definition.kinds_field(),
            definition.to_query_params(UserId::new(Uuid::parse_str(CONCENTRATED)?)),
        );
        let explain = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {SOURCE_PREFIX_SQL}");
        let plan: Json<serde_json::Value> =
            bind_filter(sqlx::query(&explain), &filter, now, &builtin_ids)
                .bind(101_i64)
                .fetch_one(&mut *tx)
                .await?
                .try_get(0)?;
        fs::write(
            artifact_dir
                .join("plans")
                .join(format!("{filename}_{suffix}.json")),
            serde_json::to_vec_pretty(&plan.0)?,
        )?;
    }
    tx.commit().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = env::var("PERF_DATABASE_URL").map_err(|_| "PERF_DATABASE_URL is required")?;
    let artifact_dir = PathBuf::from(
        env::var("PERF_ARTIFACT_DIR").unwrap_or_else(|_| "/tmp/crm-011c-perf".to_string()),
    );
    let mode = env::var("PERF_MODE").unwrap_or_else(|_| "smoke".to_string());
    let artifact_label = env::var("PERF_LABEL").unwrap_or_else(|_| mode.clone());
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(ACQUIRE_BUDGET)
        .connect(&url)
        .await?;

    if mode == "plans" || mode == "plans_jit_off" {
        capture_plans(&pool, &artifact_dir, mode == "plans_jit_off").await?;
        println!("plans captured");
        pool.close().await;
        return Ok(());
    }

    if mode == "smoke" {
        let smoke = case(
            "smoke_five_overlap_concentrated_spare_B",
            Uuid::parse_str(CONCENTRATED)?,
            "five_overlap",
            5,
        );
        let metric = run_request(&pool, &smoke, false).await;
        fs::write(
            artifact_dir.join("metrics").join("smoke.json"),
            serde_json::to_vec_pretty(&metric)?,
        )?;
        println!("{}", serde_json::to_string(&metric)?);
        pool.close().await;
        return Ok(());
    }

    let forced_case = case(
        "forced_timeout_recovery",
        Uuid::parse_str(EMPTY)?,
        "five_overlap",
        5,
    );
    let report = Report {
        prototype: "isolated SQL prototype; not HTTP/API evidence",
        mode: mode.clone(),
        pool_max_connections: 10,
        source_budget_ms: SOURCE_BUDGET.as_millis() as u64,
        metadata_budget_ms: METADATA_BUDGET.as_millis() as u64,
        acquire_budget_ms: ACQUIRE_BUDGET.as_millis() as u64,
        summaries: run_mode(pool.clone(), &mode).await,
        forced_timeout_recovery: run_request(&pool, &forced_case, true).await,
    };
    fs::write(
        artifact_dir
            .join("metrics")
            .join(format!("report-{artifact_label}.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string(&report)?);
    pool.close().await;
    Ok(())
}
