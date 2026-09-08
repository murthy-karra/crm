//! Frozen pre-switch text (see `README.md`) for the last three of the
//! fourteen Slice 012 statements: `person_state`, `call_membership` and
//! `call_only` (`crm-app/src/domain/today/system_feeds/evaluate.rs`).
//! Copied byte-for-byte at this lane's branch point `61b08ac` (identical
//! to `b45b04f`); only Rust import paths and the `.sql` file paths were
//! adjusted. `PersonStateRow`/`CallMembershipRow`/`CallOnlyRow` and their
//! `TryFrom<_> for TodayCandidate` conversions are otherwise verbatim —
//! `TodayCandidate` itself, and every model type it is built from, are
//! the SAME live types `evaluate.rs` uses today.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crm_api::domain::commands::{ContactAttemptRef, ContactChannel, ContactOutcome};
use crm_api::domain::person::filter::PersonFilterParams;
use crm_api::domain::person::model::{compute_display_name, PersonSummary, StageRef, UserRef};
use crm_api::domain::today::model::{InquiryRef, OutcomeNeededCall, TodayCandidate};
use crm_api::ids::{InquiryId, OrganizationId, PersonId, StageId, UserId};

fn required<T>(value: Option<T>, column: &'static str) -> Result<T, sqlx::Error> {
    value.ok_or_else(|| {
        sqlx::Error::Decode(
            format!("frozen system feed query: expected {column} to be non-null").into(),
        )
    })
}

fn decode_contact(
    id: Option<Uuid>,
    channel: Option<String>,
    outcome: Option<String>,
    occurred_at: Option<DateTime<Utc>>,
) -> Result<Option<ContactAttemptRef>, sqlx::Error> {
    match (id, channel, outcome, occurred_at) {
        (Some(id), Some(channel), Some(outcome), Some(occurred_at)) => {
            let channel = ContactChannel::decode(&channel).ok_or_else(|| {
                sqlx::Error::Decode("frozen system feed query: invalid contact channel".into())
            })?;
            let outcome = ContactOutcome::decode(&outcome).ok_or_else(|| {
                sqlx::Error::Decode("frozen system feed query: invalid contact outcome".into())
            })?;
            Ok(Some(ContactAttemptRef {
                id,
                channel,
                outcome,
                occurred_at,
            }))
        }
        (None, None, None, None) => Ok(None),
        _ => Err(sqlx::Error::Decode(
            "frozen system feed query: contact columns must be all null or all set".into(),
        )),
    }
}

// --- Person-state statement ------------------------------------------------

struct PersonStateRow {
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
    by_inquiry: bool,
    by_reply: bool,
    fresh: bool,
    order_key: Option<DateTime<Utc>>,
}

impl TryFrom<PersonStateRow> for TodayCandidate {
    type Error = sqlx::Error;

    fn try_from(row: PersonStateRow) -> Result<Self, sqlx::Error> {
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
        let latest_inquiry_received_at =
            required(row.latest_inquiry_received_at, "latest_inquiry.received_at")?;
        let person = PersonSummary {
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
            last_inquiry_at: Some(latest_inquiry_received_at),
            created_at: row.created_at,
        };
        let latest_inquiry = InquiryRef {
            id: InquiryId::new(required(row.latest_inquiry_id, "latest_inquiry.id")?),
            source: required(row.latest_inquiry_source, "latest_inquiry.source")?,
            received_at: latest_inquiry_received_at,
        };
        let last_contact_attempt = decode_contact(
            row.last_attempt_id,
            row.last_attempt_channel,
            row.last_attempt_outcome,
            row.last_attempt_occurred_at,
        )?;
        let waiting_since = required(row.order_key, "order_key")?;
        let client_replied = if row.by_reply {
            Some(waiting_since)
        } else {
            None
        };
        if !row.by_inquiry && !row.by_reply {
            return Err(sqlx::Error::Decode(
                "frozen system feed person-state query: a candidate must qualify by inquiry or reply"
                    .into(),
            ));
        }
        Ok(TodayCandidate {
            person,
            latest_inquiry,
            last_contact_attempt,
            waiting_since,
            inquiry_count: row.inquiry_count,
            fresh: row.fresh,
            by_inquiry: row.by_inquiry,
            outcome_needed: None,
            client_replied,
        })
    }
}

/// Frozen `person_state_candidates`. `feed_a` is `unanswered_inquiry`,
/// `feed_b` is `client_replied`. Takes the already-resolved
/// `PersonFilterParams` for each feed directly (rather than a
/// `ResolvedFeed`), so a test can bind two independently constructed
/// filters without needing the live feed-storage/evaluation machinery.
#[allow(clippy::too_many_arguments)]
pub async fn person_state_candidates(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    viewer: UserId,
    now: DateTime<Utc>,
    params_a: &PersonFilterParams,
    enabled_a: bool,
    fresh_hours_a: i32,
    params_b: &PersonFilterParams,
    enabled_b: bool,
    fresh_hours_b: i32,
) -> Result<(Vec<TodayCandidate>, bool), sqlx::Error> {
    let mut rows = sqlx::query_file_as!(
        PersonStateRow,
        "tests/fixtures/statements_b45b04f/sql/person_state.sql",
        organization_id.0,
        params_a.stage_ids.as_deref(),
        params_a.assigned_user_ids.as_deref(),
        params_a.assigned_include_unassigned,
        params_a.sources.as_deref(),
        params_a.created_within_days,
        params_a.created_not_within_days,
        params_a.created_never,
        params_a.last_inquiry_within_days,
        params_a.last_inquiry_not_within_days,
        params_a.last_inquiry_never,
        params_a.last_contact_within_days,
        params_a.last_contact_not_within_days,
        params_a.last_contact_never,
        params_a.last_inbound_within_days,
        params_a.last_inbound_not_within_days,
        params_a.last_inbound_never,
        params_a.has_replied,
        params_a.has_phone,
        params_a.has_email,
        params_a.awaiting_response,
        params_a.client_replied_unanswered,
        params_a.awaiting_call_outcome,
        params_b.stage_ids.as_deref(),
        params_b.assigned_user_ids.as_deref(),
        params_b.assigned_include_unassigned,
        params_b.sources.as_deref(),
        params_b.created_within_days,
        params_b.created_not_within_days,
        params_b.created_never,
        params_b.last_inquiry_within_days,
        params_b.last_inquiry_not_within_days,
        params_b.last_inquiry_never,
        params_b.last_contact_within_days,
        params_b.last_contact_not_within_days,
        params_b.last_contact_never,
        params_b.last_inbound_within_days,
        params_b.last_inbound_not_within_days,
        params_b.last_inbound_never,
        params_b.has_replied,
        params_b.has_phone,
        params_b.has_email,
        params_b.awaiting_response,
        params_b.client_replied_unanswered,
        params_b.awaiting_call_outcome,
        now,
        viewer.0,
        enabled_a,
        enabled_b,
        fresh_hours_a,
        fresh_hours_b,
        params_a.tag_ids_any.as_deref(),
        params_a.tag_ids_none.as_deref(),
        params_b.tag_ids_any.as_deref(),
        params_b.tag_ids_none.as_deref(),
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

// --- Call feed --------------------------------------------------------------

struct CallMembershipRow {
    person_id: Uuid,
    call_id: Uuid,
    ended_at: DateTime<Utc>,
}

pub async fn call_membership(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    viewer: UserId,
    params: &PersonFilterParams,
    retained_ids: &[Uuid],
    now: DateTime<Utc>,
) -> Result<Vec<(Uuid, Uuid, DateTime<Utc>)>, sqlx::Error> {
    if retained_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_file_as!(
        CallMembershipRow,
        "tests/fixtures/statements_b45b04f/sql/call_membership.sql",
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
        viewer.0,
        retained_ids,
        now,
        params.tag_ids_any.as_deref(),
        params.tag_ids_none.as_deref(),
    )
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.person_id, r.call_id, r.ended_at))
        .collect())
}

struct CallOnlyRow {
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
    call_id: Uuid,
    ended_at: DateTime<Utc>,
}

impl TryFrom<CallOnlyRow> for TodayCandidate {
    type Error = sqlx::Error;

    fn try_from(row: CallOnlyRow) -> Result<Self, sqlx::Error> {
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
        let latest_inquiry_received_at =
            required(row.latest_inquiry_received_at, "latest_inquiry.received_at")?;
        let person = PersonSummary {
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
            last_inquiry_at: Some(latest_inquiry_received_at),
            created_at: row.created_at,
        };
        let latest_inquiry = InquiryRef {
            id: InquiryId::new(required(row.latest_inquiry_id, "latest_inquiry.id")?),
            source: required(row.latest_inquiry_source, "latest_inquiry.source")?,
            received_at: latest_inquiry_received_at,
        };
        let last_contact_attempt = decode_contact(
            row.last_attempt_id,
            row.last_attempt_channel,
            row.last_attempt_outcome,
            row.last_attempt_occurred_at,
        )?;
        Ok(TodayCandidate {
            person,
            latest_inquiry,
            last_contact_attempt,
            waiting_since: row.ended_at,
            inquiry_count: row.inquiry_count,
            fresh: false,
            by_inquiry: false,
            outcome_needed: Some(OutcomeNeededCall {
                call_id: row.call_id,
                ended_at: row.ended_at,
            }),
            client_replied: None,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn call_only_candidates(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    viewer: UserId,
    params: &PersonFilterParams,
    retained_ids: &[Uuid],
    limit: i64,
    now: DateTime<Utc>,
) -> Result<Vec<TodayCandidate>, sqlx::Error> {
    let rows = sqlx::query_file_as!(
        CallOnlyRow,
        "tests/fixtures/statements_b45b04f/sql/call_only.sql",
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
        viewer.0,
        retained_ids,
        limit,
        now,
        params.tag_ids_any.as_deref(),
        params.tag_ids_none.as_deref(),
    )
    .fetch_all(conn)
    .await?;
    rows.into_iter().map(TodayCandidate::try_from).collect()
}
