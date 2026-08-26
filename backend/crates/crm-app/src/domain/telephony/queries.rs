//! `call` persistence and the `CallView` read model
//! (docs/specs/SLICE_006.md §3, §5). Every query is Organization-scoped;
//! a foreign call is simply `None` (404). Reads work with telephony
//! disabled — nothing here touches the provider.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::person::model::UserRef;
use crate::domain::telephony::{CallStatus, EndReason, FailureReason};
use crate::ids::OrganizationId;

/// One `call` row as the application sees it. `status`/reasons are
/// decoded enums; a value the CHECK constraints allow but the application
/// does not know is a decode error, never a panic.
#[derive(Debug, Clone)]
pub struct CallRow {
    pub id: Uuid,
    pub organization_id: OrganizationId,
    pub person_id: Uuid,
    pub contact_method_id: Uuid,
    pub caller_user_id: Uuid,
    pub origin: String,
    pub correlation_id: Uuid,
    pub status: CallStatus,
    pub failure_reason: Option<FailureReason>,
    pub end_reason: Option<EndReason>,
    pub provider: String,
    pub provider_room: String,
    pub provider_call_ref: Option<String>,
    pub placed_at: DateTime<Utc>,
    pub dial_requested_at: Option<DateTime<Utc>>,
    pub ringing_at: Option<DateTime<Utc>>,
    pub answered_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl CallRow {
    /// `ended_at - answered_at` in whole seconds when the call was
    /// answered and has ended (docs/specs/SLICE_006.md §2).
    pub fn talk_seconds(&self) -> Option<i32> {
        let answered_at = self.answered_at?;
        let ended_at = self.ended_at?;
        let secs = (ended_at - answered_at).num_seconds().max(0);
        Some(i32::try_from(secs).unwrap_or(i32::MAX))
    }
}

struct RawCallRow {
    id: Uuid,
    organization_id: Uuid,
    person_id: Uuid,
    contact_method_id: Uuid,
    caller_user_id: Uuid,
    origin: String,
    correlation_id: Uuid,
    status: String,
    failure_reason: Option<String>,
    end_reason: Option<String>,
    provider: String,
    provider_room: String,
    provider_call_ref: Option<String>,
    placed_at: DateTime<Utc>,
    dial_requested_at: Option<DateTime<Utc>>,
    ringing_at: Option<DateTime<Utc>>,
    answered_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
}

fn corrupt(column: &str, value: &str) -> sqlx::Error {
    sqlx::Error::Decode(format!("call.{column}: unknown value {value:?}").into())
}

impl TryFrom<RawCallRow> for CallRow {
    type Error = sqlx::Error;

    fn try_from(row: RawCallRow) -> Result<Self, sqlx::Error> {
        let status =
            CallStatus::decode(&row.status).ok_or_else(|| corrupt("status", &row.status))?;
        let failure_reason = row
            .failure_reason
            .as_deref()
            .map(|v| FailureReason::decode(v).ok_or_else(|| corrupt("failure_reason", v)))
            .transpose()?;
        let end_reason = row
            .end_reason
            .as_deref()
            .map(|v| EndReason::decode(v).ok_or_else(|| corrupt("end_reason", v)))
            .transpose()?;
        Ok(CallRow {
            id: row.id,
            organization_id: OrganizationId::new(row.organization_id),
            person_id: row.person_id,
            contact_method_id: row.contact_method_id,
            caller_user_id: row.caller_user_id,
            origin: row.origin,
            correlation_id: row.correlation_id,
            status,
            failure_reason,
            end_reason,
            provider: row.provider,
            provider_room: row.provider_room,
            provider_call_ref: row.provider_call_ref,
            placed_at: row.placed_at,
            dial_requested_at: row.dial_requested_at,
            ringing_at: row.ringing_at,
            answered_at: row.answered_at,
            ended_at: row.ended_at,
        })
    }
}

/// `call_by_id(conn, organization_id, id)` (docs/specs/SLICE_006.md §3).
pub async fn call_by_id(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    id: Uuid,
) -> Result<Option<CallRow>, sqlx::Error> {
    let row = sqlx::query_as!(
        RawCallRow,
        r#"SELECT id, organization_id, person_id, contact_method_id, caller_user_id, origin,
                  correlation_id, status, failure_reason, end_reason, provider, provider_room,
                  provider_call_ref, placed_at, dial_requested_at, ringing_at, answered_at, ended_at
           FROM call WHERE organization_id = $1 AND id = $2"#,
        organization_id.0,
        id,
    )
    .fetch_optional(conn)
    .await?;
    row.map(CallRow::try_from).transpose()
}

/// `SELECT … FOR UPDATE` on one call — `settle`'s lock
/// (docs/specs/SLICE_006.md §2, §3). Held only across the transition's
/// own statements.
pub async fn lock_call(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    id: Uuid,
) -> Result<Option<CallRow>, sqlx::Error> {
    let row = sqlx::query_as!(
        RawCallRow,
        r#"SELECT id, organization_id, person_id, contact_method_id, caller_user_id, origin,
                  correlation_id, status, failure_reason, end_reason, provider, provider_room,
                  provider_call_ref, placed_at, dial_requested_at, ringing_at, answered_at, ended_at
           FROM call WHERE organization_id = $1 AND id = $2 FOR UPDATE"#,
        organization_id.0,
        id,
    )
    .fetch_optional(conn)
    .await?;
    row.map(CallRow::try_from).transpose()
}

/// The webhook's room → call resolution (docs/specs/SLICE_006.md §7): the
/// Organization comes from the row, never from the request.
pub async fn call_by_room(
    conn: &mut PgConnection,
    provider_room: &str,
) -> Result<Option<CallRow>, sqlx::Error> {
    let row = sqlx::query_as!(
        RawCallRow,
        r#"SELECT id, organization_id, person_id, contact_method_id, caller_user_id, origin,
                  correlation_id, status, failure_reason, end_reason, provider, provider_room,
                  provider_call_ref, placed_at, dial_requested_at, ringing_at, answered_at, ended_at
           FROM call WHERE provider_room = $1
           ORDER BY placed_at DESC LIMIT 1"#,
        provider_room,
    )
    .fetch_optional(conn)
    .await?;
    row.map(CallRow::try_from).transpose()
}

/// The caller's active call, if any — the `call_id` in the 409
/// `call_in_progress` body (docs/specs/SLICE_006.md §3, §5). At most one
/// exists by the partial unique index.
pub async fn active_call_for_user(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    caller_user_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT id FROM call
           WHERE organization_id = $1 AND caller_user_id = $2
             AND status IN ('placing', 'ringing', 'answered')"#,
        organization_id.0,
        caller_user_id,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| r.id))
}

pub struct NewCall<'a> {
    pub id: Uuid,
    pub organization_id: OrganizationId,
    pub person_id: Uuid,
    pub contact_method_id: Uuid,
    pub caller_user_id: Uuid,
    pub origin: &'a str,
    pub correlation_id: Uuid,
    pub provider: &'a str,
    pub provider_room: &'a str,
    pub placed_at: DateTime<Utc>,
}

/// Inserts a `placing` call. A `23505` here is the partial unique index:
/// the caller already has an active call (docs/specs/SLICE_006.md §7).
pub async fn insert_placing(conn: &mut PgConnection, call: NewCall<'_>) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO call
            (id, organization_id, person_id, contact_method_id, caller_user_id, origin,
             correlation_id, status, provider, provider_room, placed_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, 'placing', $8, $9, $10)"#,
        call.id,
        call.organization_id.0,
        call.person_id,
        call.contact_method_id,
        call.caller_user_id,
        call.origin,
        call.correlation_id,
        call.provider,
        call.provider_room,
        call.placed_at,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// The guarded `dial` UPDATE (docs/specs/SLICE_006.md §3): sets
/// `dial_requested_at` exactly once while still `placing`; `false` when
/// already requested or not `placing` (409 `invalid_call_state`).
pub async fn mark_dial_requested(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    id: Uuid,
    now: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"UPDATE call SET dial_requested_at = $3, updated_at = $3
           WHERE organization_id = $1 AND id = $2
             AND dial_requested_at IS NULL AND status = 'placing'"#,
        organization_id.0,
        id,
        now,
    )
    .execute(conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// `(phone_number, normalized)` for the dial: the contact method resolved
/// by `(id, person_id, organization_id, kind = 'phone')` — the client never
/// supplies a number (docs/specs/SLICE_006.md §7). Returns only
/// `normalized_value`; `value` is never used for dialing.
pub async fn phone_contact_method_normalized(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: Uuid,
    contact_method_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT normalized_value FROM contact_method
           WHERE id = $1 AND person_id = $2 AND organization_id = $3 AND kind = 'phone'"#,
        contact_method_id,
        person_id,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|r| r.normalized_value))
}

/// `start_call`'s contact-method check (docs/specs/SLICE_006.md §3):
/// `(id, person_id, organization_id, kind = 'phone')` — nonexistent,
/// foreign, another Person's, or an email are all simply `false`. Only
/// existence is read; the number itself stays in the database until the
/// dial task needs it.
pub async fn phone_contact_method_exists(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    person_id: Uuid,
    contact_method_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT 1 AS "one!" FROM contact_method
           WHERE id = $1 AND person_id = $2 AND organization_id = $3 AND kind = 'phone'"#,
        contact_method_id,
        person_id,
        organization_id.0,
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

/// `CallView` (docs/specs/SLICE_006.md §5), PII-free.
#[derive(Debug, Clone, Serialize)]
pub struct CallView {
    pub id: Uuid,
    pub person_id: Uuid,
    pub contact_method_id: Uuid,
    pub caller: UserRef,
    pub status: CallStatus,
    pub failure_reason: Option<FailureReason>,
    pub end_reason: Option<EndReason>,
    pub placed_at: DateTime<Utc>,
    pub ringing_at: Option<DateTime<Utc>>,
    pub answered_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub talk_seconds: Option<i32>,
}

impl CallView {
    pub fn from_row(row: &CallRow, caller_display_name: String) -> Self {
        CallView {
            id: row.id,
            person_id: row.person_id,
            contact_method_id: row.contact_method_id,
            caller: UserRef {
                id: row.caller_user_id,
                display_name: caller_display_name,
            },
            status: row.status,
            failure_reason: row.failure_reason,
            end_reason: row.end_reason,
            placed_at: row.placed_at,
            ringing_at: row.ringing_at,
            answered_at: row.answered_at,
            ended_at: row.ended_at,
            talk_seconds: row.talk_seconds(),
        }
    }
}

/// The caller's display name for `CallView.caller`.
pub async fn caller_display_name(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<String, sqlx::Error> {
    let row = sqlx::query!("SELECT display_name FROM app_user WHERE id = $1", user_id)
        .fetch_one(conn)
        .await?;
    Ok(row.display_name)
}

/// `GET /api/calls/{id}` (docs/specs/SLICE_006.md §5): any member of the
/// Organization; foreign → `None`.
pub async fn call_view_by_id(
    conn: &mut PgConnection,
    organization_id: OrganizationId,
    id: Uuid,
) -> Result<Option<CallView>, sqlx::Error> {
    let Some(row) = call_by_id(conn, organization_id, id).await? else {
        return Ok(None);
    };
    let display_name = caller_display_name(conn, row.caller_user_id).await?;
    Ok(Some(CallView::from_row(&row, display_name)))
}
