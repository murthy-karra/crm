//! Frozen pre-switch text (see `README.md`) for nine of the fourteen
//! Slice 012 statements: `filtered_summaries` (the canonical statement and
//! its seven sorted copies — eight of the fourteen) and
//! `count_filtered_matches` (the ninth). Copied byte-for-byte from
//! `crm-app/src/domain/person/queries.rs` and its `sql/` siblings at this
//! lane's branch point `61b08ac` (identical to `b45b04f` — see
//! `README.md`'s provenance table). Only Rust import paths (`crm_api::`
//! instead of `crate::`) and the `.sql` file paths (now relative to this
//! `crm-api` test binary's own crate root, not `crm-app`'s) were adjusted;
//! `PersonSummaryRow`, the row-to-`PersonSummary` conversion, the
//! `query_file_as!`/`query!` macros themselves, and every bound
//! parameter's position are otherwise verbatim.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crm_api::domain::person::filter::PersonFilterParams;
use crm_api::domain::person::model::{compute_display_name, PersonSummary, StageRef, UserRef};
use crm_api::domain::person::visibility::PersonVisibilityScope;
use crm_api::ids::{PersonId, StageId, UserId};

struct PersonSummaryRow {
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

/// Binds the full 27-parameter matrix, matching the live `query_file_as!`
/// call in `filtered_summaries_with_reference_now`/`filtered_summaries_sorted`
/// exactly — one macro call site per literal `.sql` path, mirroring the
/// live `run_sorted!` macro (`query_file_as!`'s path argument must be a
/// compile-time literal, so this cannot be a single runtime-path
/// function).
async fn run(
    conn: &mut PgConnection,
    which: SortedStatement,
    organization_id: Uuid,
    params: &PersonFilterParams,
    reference_now: Option<DateTime<Utc>>,
) -> Result<(Vec<PersonSummary>, bool), sqlx::Error> {
    macro_rules! bind {
        ($path:literal) => {
            sqlx::query_file_as!(
                PersonSummaryRow,
                $path,
                organization_id,
                params.stage_ids.as_deref(),
                params.assigned_user_ids.as_deref(),
                params.assigned_include_unassigned,
                params.sources.as_deref(),
                params.created_within_days,
                params.created_not_within_days,
                params.created_never,
                params.last_inquiry_within_days,
                params.last_inquiry_not_within_days,
                params.last_inquiry_never,
                params.last_contact_within_days,
                params.last_contact_not_within_days,
                params.last_contact_never,
                params.last_inbound_within_days,
                params.last_inbound_not_within_days,
                params.last_inbound_never,
                params.has_replied,
                params.has_phone,
                params.has_email,
                reference_now,
                params.awaiting_response,
                params.client_replied_unanswered,
                params.awaiting_call_outcome,
                params.viewer_id,
                params.tag_ids_any.as_deref(),
                params.tag_ids_none.as_deref(),
            )
            .fetch_all(&mut *conn)
            .await?
        };
    }

    let mut rows = match which {
        SortedStatement::Canonical => {
            bind!("tests/fixtures/statements_b45b04f/sql/filtered_summaries.sql")
        }
        SortedStatement::CreatedAsc => {
            bind!("tests/fixtures/statements_b45b04f/sql/filtered_summaries_created_asc.sql")
        }
        SortedStatement::NameAsc => {
            bind!("tests/fixtures/statements_b45b04f/sql/filtered_summaries_name_asc.sql")
        }
        SortedStatement::NameDesc => {
            bind!("tests/fixtures/statements_b45b04f/sql/filtered_summaries_name_desc.sql")
        }
        SortedStatement::StageAsc => {
            bind!("tests/fixtures/statements_b45b04f/sql/filtered_summaries_stage_asc.sql")
        }
        SortedStatement::StageDesc => {
            bind!("tests/fixtures/statements_b45b04f/sql/filtered_summaries_stage_desc.sql")
        }
        SortedStatement::AssigneeAsc => {
            bind!("tests/fixtures/statements_b45b04f/sql/filtered_summaries_assignee_asc.sql")
        }
        SortedStatement::AssigneeDesc => {
            bind!("tests/fixtures/statements_b45b04f/sql/filtered_summaries_assignee_desc.sql")
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
/// always uses `now()`), capped at 501 in a subquery. `count_filtered_matches`
/// has no separate `.sql` file live either (an inline `query!` string), so
/// its frozen copy is inline too — text copied verbatim.
pub async fn count_filtered_matches(
    conn: &mut PgConnection,
    scope: &PersonVisibilityScope,
    params: &PersonFilterParams,
) -> Result<(i64, bool), sqlx::Error> {
    let organization_id = scope.organization_id();
    let row = sqlx::query!(
        r#"SELECT count(*) as "count!"
           FROM (
             SELECT p.id
             FROM person p
             JOIN stage s ON s.id = p.stage_id
             LEFT JOIN LATERAL (
                 SELECT i2.source
                 FROM inquiry i2
                 WHERE i2.person_id = p.id AND i2.organization_id = p.organization_id
                 ORDER BY i2.received_at DESC, i2.id DESC
                 LIMIT 1
             ) latest_src ON true
             LEFT JOIN LATERAL (
                 SELECT max(i3.received_at) as ts
                 FROM inquiry i3
                 WHERE i3.person_id = p.id AND i3.organization_id = p.organization_id
             ) last_inquiry_ts ON true
             LEFT JOIN LATERAL (
                 SELECT max(ca.occurred_at) as ts
                 FROM contact_attempted ca
                 WHERE ca.person_id = p.id AND ca.organization_id = p.organization_id
             ) last_contact_ts ON true
             LEFT JOIN LATERAL (
                 SELECT max(cc.occurred_at) as ts
                 FROM correspondence_captured cc
                 WHERE cc.person_id = p.id AND cc.organization_id = p.organization_id
                   AND cc.direction = 'inbound'
             ) last_inbound_ts ON true
             LEFT JOIN LATERAL (
                 SELECT max(cc5.occurred_at) as ts
                 FROM correspondence_captured cc5
                 WHERE cc5.person_id = p.id AND cc5.organization_id = p.organization_id
                   AND cc5.direction = 'outbound'
             ) last_outbound_ts ON true
             WHERE p.organization_id = $1
               AND ($2::uuid[] IS NULL OR p.stage_id = ANY($2))
               AND ($3::uuid[] IS NULL OR p.assigned_user_id = ANY($3)
                    OR ($4::boolean AND p.assigned_user_id IS NULL))
               AND ($5::text[] IS NULL OR latest_src.source = ANY($5))
               AND ($6::int IS NULL
                    OR COALESCE(p.created_at, '-infinity'::timestamptz) > now() - make_interval(days => $6))
               AND ($7::int IS NULL
                    OR COALESCE(p.created_at, '-infinity'::timestamptz) <= now() - make_interval(days => $7))
               AND ($8::boolean IS NULL OR (p.created_at IS NULL) = $8)
               AND ($9::int IS NULL
                    OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => $9))
               AND ($10::int IS NULL
                    OR COALESCE(last_inquiry_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => $10))
               AND ($11::boolean IS NULL OR (last_inquiry_ts.ts IS NULL) = $11)
               AND ($12::int IS NULL
                    OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => $12))
               AND ($13::int IS NULL
                    OR COALESCE(last_contact_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => $13))
               AND ($14::boolean IS NULL OR (last_contact_ts.ts IS NULL) = $14)
               AND ($15::int IS NULL
                    OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) > now() - make_interval(days => $15))
               AND ($16::int IS NULL
                    OR COALESCE(last_inbound_ts.ts, '-infinity'::timestamptz) <= now() - make_interval(days => $16))
               AND ($17::boolean IS NULL OR (last_inbound_ts.ts IS NULL) = $17)
               AND ($18::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM correspondence_captured cc2
                     WHERE cc2.person_id = p.id AND cc2.organization_id = p.organization_id
                       AND cc2.direction = 'inbound'
                   )) = $18)
               AND ($19::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM contact_method cm3
                     WHERE cm3.person_id = p.id AND cm3.organization_id = p.organization_id
                       AND cm3.kind = 'phone'
                   )) = $19)
               AND ($20::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM contact_method cm4
                     WHERE cm4.person_id = p.id AND cm4.organization_id = p.organization_id
                       AND cm4.kind = 'email'
                   )) = $20)
               AND ($21::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM inquiry ia
                     WHERE ia.person_id = p.id AND ia.organization_id = p.organization_id
                       AND ia.received_at > COALESCE(last_contact_ts.ts, '-infinity'::timestamptz)
                   )) = $21)
               AND ($22::boolean IS NULL OR (
                     last_inbound_ts.ts IS NOT NULL
                     AND last_inbound_ts.ts > COALESCE(last_contact_ts.ts, '-infinity'::timestamptz)
                     AND last_inbound_ts.ts > COALESCE(last_outbound_ts.ts, '-infinity'::timestamptz)
                   ) = $22)
               AND ($23::boolean IS NULL OR (EXISTS (
                     SELECT 1 FROM call c
                     WHERE c.organization_id = p.organization_id
                       AND c.person_id = p.id
                       AND c.caller_user_id = $24
                       AND c.status IN ('ended', 'failed')
                       AND c.ended_at IS NOT NULL
                       AND EXISTS (
                           SELECT 1 FROM contact_attempted root
                           WHERE root.organization_id = c.organization_id
                             AND root.causation_id = c.id
                             AND root.corrects_id IS NULL
                             AND NOT EXISTS (SELECT 1 FROM contact_attempted x WHERE x.corrects_id = root.id)
                       )
                   )) = $23)
               AND ($25::uuid[] IS NULL OR EXISTS (
                     SELECT 1 FROM person_tag pt
                     WHERE pt.organization_id = $1
                       AND pt.person_id = p.id AND pt.tag_id = ANY($25)))
               AND ($26::uuid[] IS NULL OR NOT EXISTS (
                     SELECT 1 FROM person_tag pt2
                     WHERE pt2.organization_id = $1
                       AND pt2.person_id = p.id AND pt2.tag_id = ANY($26)))
             LIMIT 501
           ) capped"#,
        organization_id.0,
        params.stage_ids.as_deref(),
        params.assigned_user_ids.as_deref(),
        params.assigned_include_unassigned,
        params.sources.as_deref(),
        params.created_within_days,
        params.created_not_within_days,
        params.created_never,
        params.last_inquiry_within_days,
        params.last_inquiry_not_within_days,
        params.last_inquiry_never,
        params.last_contact_within_days,
        params.last_contact_not_within_days,
        params.last_contact_never,
        params.last_inbound_within_days,
        params.last_inbound_not_within_days,
        params.last_inbound_never,
        params.has_replied,
        params.has_phone,
        params.has_email,
        params.awaiting_response,
        params.client_replied_unanswered,
        params.awaiting_call_outcome,
        params.viewer_id,
        params.tag_ids_any.as_deref(),
        params.tag_ids_none.as_deref(),
    )
    .fetch_one(conn)
    .await?;

    Ok((row.count.min(500), row.count > 500))
}
