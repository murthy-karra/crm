//! Frozen pre-switch text (see `README.md`) for two of the fourteen Slice
//! 012 statements: `source_membership` and `source_candidates`
//! (`crm-app/src/domain/today/sources.rs`). Copied byte-for-byte at this
//! lane's branch point `61b08ac` (identical to `b45b04f`); only Rust
//! import paths and the `.sql` file paths were adjusted. `SourceCandidate`
//! itself is `pub(crate)` in the live crate (inaccessible from this
//! `crm-api` test binary), so this module declares its own identically
//! shaped `pub` copy — every field and the live `PersonSummary`/
//! `InquiryRef`/`ContactAttemptRef` types it is built from are otherwise
//! the SAME live types `sources.rs` uses.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crm_api::domain::commands::{ContactAttemptRef, ContactChannel, ContactOutcome};
use crm_api::domain::person::filter::PersonFilterParams;
use crm_api::domain::person::model::{compute_display_name, PersonSummary, StageRef, UserRef};
use crm_api::domain::today::InquiryRef;
use crm_api::ids::{InquiryId, OrganizationId, PersonId, StageId, UserId};

#[derive(Debug, Clone)]
pub struct FrozenSourceCandidate {
    pub person: PersonSummary,
    pub latest_inquiry: Option<InquiryRef>,
    pub last_contact_attempt: Option<ContactAttemptRef>,
    pub last_contact_at: Option<DateTime<Utc>>,
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
                sqlx::Error::Decode("frozen today source candidate has an invalid channel".into())
            })?;
            let outcome = ContactOutcome::decode(&outcome).ok_or_else(|| {
                sqlx::Error::Decode("frozen today source candidate has an invalid outcome".into())
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
            "frozen today source candidate contact columns must be all null or all set".into(),
        )),
    }
}

struct SourceCandidateRow {
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
    last_contact_at: Option<DateTime<Utc>>,
}

#[allow(clippy::too_many_arguments)]
pub async fn source_candidates(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    params: &PersonFilterParams,
    now: DateTime<Utc>,
    builtin_ids: &[Uuid],
    members_only: bool,
    limit: i64,
) -> Result<Vec<FrozenSourceCandidate>, sqlx::Error> {
    let rows = sqlx::query_file_as!(
        SourceCandidateRow,
        "tests/fixtures/statements_b45b04f/sql/source_candidates.sql",
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
        now,
        builtin_ids,
        members_only,
        limit,
        params.awaiting_response,
        params.client_replied_unanswered,
        params.awaiting_call_outcome,
        params.viewer_id,
        params.tag_ids_any.as_deref(),
        params.tag_ids_none.as_deref(),
    )
    .fetch_all(&mut *conn)
    .await?;

    rows.into_iter()
        .map(|row| {
            let id = row.id;
            let first_name = row.first_name;
            let last_name = row.last_name;
            let primary_email = row.primary_email;
            let primary_phone = row.primary_phone;
            let assigned_id = row.assigned_user_id;
            let assigned_name = row.assigned_user_display_name;
            let person = PersonSummary {
                id: PersonId::new(id),
                first_name: first_name.clone(),
                last_name: last_name.clone(),
                display_name: compute_display_name(
                    first_name.as_deref(),
                    last_name.as_deref(),
                    primary_email.as_deref(),
                    primary_phone.as_deref(),
                ),
                stage: StageRef {
                    id: StageId::new(row.stage_id),
                    name: row.stage_name,
                },
                assigned_user: match (assigned_id, assigned_name) {
                    (Some(id), Some(display_name)) => Some(UserRef {
                        id: UserId::new(id),
                        display_name,
                    }),
                    _ => None,
                },
                primary_email,
                primary_phone,
                inquiry_count: row.inquiry_count,
                last_inquiry_at: row.latest_inquiry_received_at,
                created_at: row.created_at,
            };
            let latest_inquiry = match (
                row.latest_inquiry_id,
                row.latest_inquiry_source,
                row.latest_inquiry_received_at,
            ) {
                (Some(id), Some(source), Some(received_at)) => Some(InquiryRef {
                    id: InquiryId::new(id),
                    source,
                    received_at,
                }),
                (None, None, None) => None,
                _ => {
                    return Err(sqlx::Error::Decode(
                        "frozen today source candidate latest inquiry columns must agree".into(),
                    ))
                }
            };
            Ok(FrozenSourceCandidate {
                person,
                latest_inquiry,
                last_contact_attempt: decode_contact(
                    row.last_attempt_id,
                    row.last_attempt_channel,
                    row.last_attempt_outcome,
                    row.last_attempt_occurred_at,
                )?,
                last_contact_at: row.last_contact_at,
            })
        })
        .collect()
}

pub async fn source_membership(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    params: &PersonFilterParams,
    now: DateTime<Utc>,
    builtin_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query_file!(
        "tests/fixtures/statements_b45b04f/sql/source_membership.sql",
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
        now,
        builtin_ids,
        params.awaiting_response,
        params.client_replied_unanswered,
        params.awaiting_call_outcome,
        params.viewer_id,
        params.tag_ids_any.as_deref(),
        params.tag_ids_none.as_deref(),
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|row| row.id).collect())
}
