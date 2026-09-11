//! Frozen pre-switch text (see `README.md`) for nine of the fourteen
//! Slice 012 statements: `filtered_summaries` (the canonical statement and
//! its seven sorted copies — eight of the fourteen) and
//! `count_filtered_matches` (the ninth). Copied byte-for-byte from
//! `crm-app/src/domain/person/queries.rs` and its `sql/` siblings at this
//! lane's branch point `6bad52a`. Only Rust import paths (`crm_api::`
//! instead of `crate::`) and the `.sql` file paths (now relative to this
//! `crm-api` test binary's own crate root, not `crm-app`'s) were adjusted.
//! Fixed `include_str!` statements plus runtime SQLx bindings keep the
//! fixture out of production's offline macro metadata; every bind position
//! and decoded result remains the `6bad52a` behavior.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crm_api::domain::person::filter::PersonFilterParams;
use crm_api::domain::person::model::{compute_display_name, PersonSummary, StageRef, UserRef};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::ids::{PersonId, StageId, UserId};

#[derive(sqlx::FromRow)]
struct PersonSummaryRow {
    id: Uuid,
    first_name: Option<String>,
    last_name: Option<String>,
    created_at: DateTime<Utc>,
    stage_id: Uuid,
    stage_name: String,
    #[sqlx(rename = "assigned_user_id?")]
    assigned_user_id: Option<Uuid>,
    #[sqlx(rename = "assigned_user_display_name?")]
    assigned_user_display_name: Option<String>,
    #[sqlx(rename = "primary_email?")]
    primary_email: Option<String>,
    #[sqlx(rename = "primary_phone?")]
    primary_phone: Option<String>,
    #[sqlx(rename = "inquiry_count!")]
    inquiry_count: i64,
    #[sqlx(rename = "last_inquiry_at?")]
    last_inquiry_at: Option<DateTime<Utc>>,
}

impl From<PersonSummaryRow> for PersonSummary {
    fn from(row: PersonSummaryRow) -> Self {
        let display_name = compute_display_name(
            row.first_name.as_deref(),
            row.last_name.as_deref(),
            row.primary_email.as_deref(),
            row.primary_phone.as_deref(),
        );
        let assigned_user = match (row.assigned_user_id, row.assigned_user_display_name) {
            (Some(id), Some(display_name)) => Some(UserRef {
                id: UserId::new(id),
                display_name,
            }),
            _ => None,
        };
        PersonSummary {
            id: PersonId::new(row.id),
            first_name: row.first_name,
            last_name: row.last_name,
            display_name,
            stage: StageRef {
                id: StageId::new(row.stage_id),
                name: row.stage_name,
            },
            assigned_user,
            primary_email: row.primary_email,
            primary_phone: row.primary_phone,
            inquiry_count: row.inquiry_count,
            last_inquiry_at: row.last_inquiry_at,
            created_at: row.created_at,
        }
    }
}

/// Which of the eight frozen `filtered_summaries*` statements to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortedStatement {
    /// The canonical, unsorted-request statement (`created.desc`).
    Canonical,
    CreatedAsc,
    NameAsc,
    NameDesc,
    StageAsc,
    StageDesc,
    AssigneeAsc,
    AssigneeDesc,
}

/// Binds the full 27-parameter matrix, matching the `6bad52a` statements
/// exactly. The SQL text remains a fixed compile-time include, while runtime
/// binds avoid introducing test-only SQLx macro metadata.
async fn run(
    conn: &mut PgConnection,
    which: SortedStatement,
    organization_id: Uuid,
    params: &PersonFilterParams,
    reference_now: Option<DateTime<Utc>>,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    macro_rules! bind {
        ($sql:expr) => {
            sqlx::query_as::<_, PersonSummaryRow>($sql)
                .bind(organization_id)
                .bind(params.stage_ids.as_deref())
                .bind(params.assigned_user_ids.as_deref())
                .bind(params.assigned_include_unassigned)
                .bind(params.sources.as_deref())
                .bind(params.created_within_days)
                .bind(params.created_not_within_days)
                .bind(params.created_never)
                .bind(params.last_inquiry_within_days)
                .bind(params.last_inquiry_not_within_days)
                .bind(params.last_inquiry_never)
                .bind(params.last_contact_within_days)
                .bind(params.last_contact_not_within_days)
                .bind(params.last_contact_never)
                .bind(params.last_inbound_within_days)
                .bind(params.last_inbound_not_within_days)
                .bind(params.last_inbound_never)
                .bind(params.has_replied)
                .bind(params.has_phone)
                .bind(params.has_email)
                .bind(reference_now)
                .bind(params.awaiting_response)
                .bind(params.client_replied_unanswered)
                .bind(params.awaiting_call_outcome)
                .bind(params.viewer_id)
                .bind(params.tag_ids_any.as_deref())
                .bind(params.tag_ids_none.as_deref())
                .fetch_all(&mut *conn)
                .await?
        };
    }

    let mut rows = match which {
        SortedStatement::Canonical => {
            bind!(include_str!("sql/filtered_summaries.sql"))
        }
        SortedStatement::CreatedAsc => {
            bind!(include_str!("sql/filtered_summaries_created_asc.sql"))
        }
        SortedStatement::NameAsc => {
            bind!(include_str!("sql/filtered_summaries_name_asc.sql"))
        }
        SortedStatement::NameDesc => {
            bind!(include_str!("sql/filtered_summaries_name_desc.sql"))
        }
        SortedStatement::StageAsc => {
            bind!(include_str!("sql/filtered_summaries_stage_asc.sql"))
        }
        SortedStatement::StageDesc => {
            bind!(include_str!("sql/filtered_summaries_stage_desc.sql"))
        }
        SortedStatement::AssigneeAsc => {
            bind!(include_str!("sql/filtered_summaries_assignee_asc.sql"))
        }
        SortedStatement::AssigneeDesc => {
            bind!(include_str!("sql/filtered_summaries_assignee_desc.sql"))
        }
    };

    let truncated = rows.len() > 500;
    rows.truncate(500);
    Ok((
        rows.into_iter().map(PersonSummary::from).collect(),
        truncated,
    ))
}

/// Frozen `filtered_summaries` (no `sort` — the canonical, unsorted-request
/// statement); `reference_now` mirrors the live test-only common-clock
/// seam (`filtered_summaries_at`).
pub async fn filtered_summaries(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    params: &PersonFilterParams,
    reference_now: Option<DateTime<Utc>>,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    run(
        conn,
        SortedStatement::Canonical,
        scope.organization_id().0,
        params,
        reference_now,
    )
    .await
}

/// Frozen `filtered_summaries_sorted` for one of the seven named sorts
/// (`created.desc` is [`filtered_summaries`] above — spec §4e).
pub async fn filtered_summaries_sorted(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    params: &PersonFilterParams,
    which: SortedStatement,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    run(conn, which, scope.organization_id().0, params, None).await
}

/// Frozen `count_filtered_matches` (`person/queries.rs`): the same
/// NULL-guarded 26-parameter matrix (no `reference_now` — this statement
/// always uses `now()`), capped at 501 in a subquery. The fixture keeps the
/// verified `6bad52a` text in a fixed `include_str!` file and uses runtime
/// SQLx bindings so this test-only macro does not affect production metadata.
pub async fn count_filtered_matches(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    params: &PersonFilterParams,
) -> Result<(i64, bool), sqlx::Error> {
    let organization_id = scope.organization_id();
    let count = sqlx::query_scalar::<_, i64>(include_str!("sql/count_filtered_matches.sql"))
        .bind(organization_id.0)
        .bind(params.stage_ids.as_deref())
        .bind(params.assigned_user_ids.as_deref())
        .bind(params.assigned_include_unassigned)
        .bind(params.sources.as_deref())
        .bind(params.created_within_days)
        .bind(params.created_not_within_days)
        .bind(params.created_never)
        .bind(params.last_inquiry_within_days)
        .bind(params.last_inquiry_not_within_days)
        .bind(params.last_inquiry_never)
        .bind(params.last_contact_within_days)
        .bind(params.last_contact_not_within_days)
        .bind(params.last_contact_never)
        .bind(params.last_inbound_within_days)
        .bind(params.last_inbound_not_within_days)
        .bind(params.last_inbound_never)
        .bind(params.has_replied)
        .bind(params.has_phone)
        .bind(params.has_email)
        .bind(params.awaiting_response)
        .bind(params.client_replied_unanswered)
        .bind(params.awaiting_call_outcome)
        .bind(params.viewer_id)
        .bind(params.tag_ids_any.as_deref())
        .bind(params.tag_ids_none.as_deref())
        .fetch_one(&mut *conn)
        .await?;

    Ok((count.min(500), count > 500))
}
