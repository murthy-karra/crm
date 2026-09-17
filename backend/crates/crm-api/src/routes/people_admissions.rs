//! D-078 current-admin People admission API. Values remain in bounded encrypted plan rows.
use crate::{
    auth::OrgAdminContext,
    domain::{
        envelope::CommandContext,
        migration::{people_admission as h, people_recovery as recovery, MigrationError},
    },
    error::ApiError,
    state::AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        DefaultBodyLimit, Path, Query, State,
    },
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
fn body<T>(v: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn path<T>(v: Result<Path<T>, PathRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn query<T>(v: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    v.map(|v| v.0).map_err(|_| ApiError::MalformedRequest)
}
fn error(e: MigrationError) -> ApiError {
    match e {
        MigrationError::SourceNotEligible | MigrationError::SourceAccountMismatch => {
            ApiError::ImportError("import_conflict")
        }
        e => e.into(),
    }
}
fn response(status: StatusCode, v: Value) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(v)).into_response()
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/migrations/fub/people-admissions",
            get(list).post(prepare),
        )
        .route(
            "/api/migrations/fub/people-admissions/recoveries",
            post(prepare_recovery),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/recovery-mappings",
            get(recovery_mappings).post(edit_recovery_mapping),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/recovery-mappings/{key}/field",
            get(recovery_mapping_field),
        )
        .route("/api/migrations/fub/people-admissions/{id}", get(detail))
        .route(
            "/api/migrations/fub/people-admissions/{id}/items",
            get(items),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/items/{item}",
            get(item),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/items/{item}/contacts",
            get(contacts),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/plans",
            post(repreview),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/confirm",
            post(confirm),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/retry",
            post(retry),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/cancel",
            post(cancel),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/results",
            get(results),
        )
        .route(
            "/api/migrations/fub/people-admissions/{id}/items/{item}/fields/{field}",
            get(field),
        )
        .route(
            "/api/people/{person}/admission-provenance",
            get(admission_provenance),
        )
        .route(
            "/api/people/{person}/admission-provenance/contacts",
            get(admission_provenance_contacts),
        )
        .route(
            "/api/people/{person}/admission-provenance/fields/{field}",
            get(admission_provenance_field),
        )
        .layer(DefaultBodyLimit::max(8192))
}
async fn prepare(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<h::PreparePeopleAdmission>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::CREATED,
        h::prepare(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
            body(b)?,
            release.as_deref(),
        )
        .await
        .map_err(error)?,
    ))
}
async fn list(
    State(s): State<AppState>,
    a: OrgAdminContext,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::list(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn detail(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::detail(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn items(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::items(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn item(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
) -> Result<Response, ApiError> {
    let (id, item) = path(p)?;
    Ok(response(
        StatusCode::OK,
        h::item(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            item,
        )
        .await
        .map_err(error)?,
    ))
}
async fn contacts(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, item) = path(p)?;
    Ok(response(
        StatusCode::OK,
        h::contacts(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            item,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn repreview(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<RepreviewWire>, JsonRejection>,
) -> Result<Response, ApiError> {
    let cmd = body(b)?;
    let id = path(p)?;
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    let value = match (cmd.expected_plan_revision, cmd.expected_draft_revision) {
        (Some(revision), None) => {
            h::repreview(
                pool,
                &s.raw_payload_key,
                &ctx,
                id,
                h::RepreviewPeopleAdmission {
                    request_id: cmd.request_id,
                    expected_plan_revision: decimal(&revision, true)?,
                },
            )
            .await
        }
        (None, Some(revision)) => {
            recovery::seal_choices(
                pool,
                &s.raw_payload_key,
                &s.snapshot_policy,
                &ctx,
                id,
                recovery::SealRecoveryChoices {
                    request_id: cmd.request_id,
                    expected_draft_revision: decimal(&revision, false)?,
                },
            )
            .await
        }
        _ => return Err(ApiError::MalformedRequest),
    }
    .map_err(error)?;
    Ok(response(StatusCode::ACCEPTED, value))
}
async fn confirm(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<ConfirmWire>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    let mut wire = body(b)?;
    let ack = wire
        .recovery
        .take()
        .map(RecoveryAckWire::command)
        .transpose()?;
    let cmd = wire.command()?;
    let id = path(p)?;
    let pool = s.db.as_ref().ok_or(ApiError::Unavailable)?;
    let ctx = CommandContext::from_auth(&a.auth);
    let value = if let Some(ack) = ack {
        recovery::confirm(
            pool,
            &s.raw_payload_key,
            &ctx,
            id,
            cmd,
            ack,
            release.as_deref(),
        )
        .await
    } else {
        h::confirm(pool, &s.raw_payload_key, &ctx, id, cmd, release.as_deref()).await
    }
    .map_err(error)?;
    Ok(response(StatusCode::ACCEPTED, value))
}
async fn retry(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<LifecycleWire>, JsonRejection>,
) -> Result<Response, ApiError> {
    let release = s.current_import_release().await;
    Ok(response(
        StatusCode::ACCEPTED,
        h::retry(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?.command()?,
            release.as_deref(),
        )
        .await
        .map_err(error)?,
    ))
}
async fn cancel(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<LifecycleWire>, JsonRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::cancel(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            body(b)?.command()?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn results(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::results(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}

async fn field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid, String)>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, item, field_name) = path(p)?;
    Ok(response(
        StatusCode::OK,
        h::field(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            (id, item, "proposed".into(), field_name),
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}

async fn admission_provenance(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::admission_provenance(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn admission_provenance_contacts(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        h::admission_provenance_contacts(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}
async fn admission_provenance_field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, String)>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (person, field_name) = path(p)?;
    Ok(response(
        StatusCode::OK,
        h::admission_provenance_field(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            person,
            field_name,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}

fn decimal(value: &str, positive: bool) -> Result<i64, ApiError> {
    if value.is_empty()
        || value.len() > 19
        || !value.bytes().all(|b| b.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(ApiError::MalformedRequest);
    }
    let parsed = value
        .parse::<i64>()
        .map_err(|_| ApiError::MalformedRequest)?;
    if positive && parsed == 0 {
        return Err(ApiError::MalformedRequest);
    }
    Ok(parsed)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepreviewWire {
    request_id: Uuid,
    expected_plan_revision: Option<String>,
    expected_draft_revision: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LifecycleWire {
    request_id: Uuid,
    expected_lifecycle_revision: String,
}
impl LifecycleWire {
    fn command(self) -> Result<h::LifecyclePeopleAdmission, ApiError> {
        Ok(h::LifecyclePeopleAdmission {
            request_id: self.request_id,
            expected_lifecycle_revision: decimal(&self.expected_lifecycle_revision, true)?,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmWire {
    recovery: Option<RecoveryAckWire>,
    request_id: Uuid,
    plan_id: Uuid,
    plan_revision: String,
    plan_digest: String,
    eligible_count: String,
    acknowledged_coverage: bool,
    acknowledged_mappings: bool,
    acknowledged_distinct_contacts: bool,
    acknowledged_review_hold: bool,
}
impl ConfirmWire {
    fn command(self) -> Result<h::ConfirmPeopleAdmission, ApiError> {
        Ok(h::ConfirmPeopleAdmission {
            request_id: self.request_id,
            plan_id: self.plan_id,
            plan_revision: decimal(&self.plan_revision, true)?,
            plan_digest: self.plan_digest,
            eligible_count: decimal(&self.eligible_count, false)?,
            acknowledged_coverage: self.acknowledged_coverage,
            acknowledged_mappings: self.acknowledged_mappings,
            acknowledged_distinct_contacts: self.acknowledged_distinct_contacts,
            acknowledged_review_hold: self.acknowledged_review_hold,
        })
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn admission_wire_revisions_never_round_through_json_numbers() {
        assert!(matches!(
            super::decimal("9007199254740993", true),
            Ok(9007199254740993)
        ));
        for bad in ["01", "+1", "1.0", "-1", "9223372036854775808", " 1"] {
            assert!(super::decimal(bad, true).is_err());
        }
        let id = uuid::Uuid::new_v4();
        assert!(serde_json::from_value::<super::LifecycleWire>(
            serde_json::json!({"request_id":id,"expected_lifecycle_revision":1})
        )
        .is_err());
        assert!(serde_json::from_value::<super::LifecycleWire>(
            serde_json::json!({"request_id":id,"expected_lifecycle_revision":"1"})
        )
        .is_ok());
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryPrepareWire {
    request_id: Uuid,
    report_id: Uuid,
    anchor: recovery::RecoveryAnchor,
    expected_anchor_revision: String,
}
async fn prepare_recovery(
    State(s): State<AppState>,
    a: OrgAdminContext,
    b: Result<Json<RecoveryPrepareWire>, JsonRejection>,
) -> Result<Response, ApiError> {
    let wire = body(b)?;
    let release = s.current_import_release().await;
    let value = recovery::prepare(
        s.db.as_ref().ok_or(ApiError::Unavailable)?,
        &s.raw_payload_key,
        &s.snapshot_policy,
        &CommandContext::from_auth(&a.auth),
        recovery::PrepareRecovery {
            request_id: wire.request_id,
            report_id: wire.report_id,
            anchor: wire.anchor,
            expected_anchor_revision: decimal(&wire.expected_anchor_revision, true)?,
        },
        release.as_deref(),
    )
    .await
    .map_err(error)?;
    Ok(response(StatusCode::CREATED, value))
}
async fn recovery_mappings(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    q: Result<Query<recovery::RecoveryMappingPage>, QueryRejection>,
) -> Result<Response, ApiError> {
    Ok(response(
        StatusCode::OK,
        recovery::mapping_page(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryEditWire {
    request_id: Uuid,
    expected_draft_revision: String,
    key_id: Uuid,
    disposition: String,
    target_id: Option<Uuid>,
}
async fn edit_recovery_mapping(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<Uuid>, PathRejection>,
    b: Result<Json<RecoveryEditWire>, JsonRejection>,
) -> Result<Response, ApiError> {
    let wire = body(b)?;
    Ok(response(
        StatusCode::OK,
        recovery::edit_mapping(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &s.snapshot_policy,
            &CommandContext::from_auth(&a.auth),
            path(p)?,
            recovery::EditRecoveryMapping {
                request_id: wire.request_id,
                expected_draft_revision: decimal(&wire.expected_draft_revision, false)?,
                key_id: wire.key_id,
                disposition: wire.disposition,
                target_id: wire.target_id,
            },
        )
        .await
        .map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryAckWire {
    mapping_digest: Vec<u8>,
    candidate_count: String,
    contact_count: String,
    unassigned_count: String,
    acknowledged_creation: bool,
}
impl RecoveryAckWire {
    fn command(self) -> Result<recovery::RecoveryAcknowledgement, ApiError> {
        Ok(recovery::RecoveryAcknowledgement {
            mapping_digest: self.mapping_digest,
            candidate_count: decimal(&self.candidate_count, false)?,
            contact_count: decimal(&self.contact_count, false)?,
            unassigned_count: decimal(&self.unassigned_count, false)?,
            acknowledged_creation: self.acknowledged_creation,
        })
    }
}

async fn recovery_mapping_field(
    State(s): State<AppState>,
    a: OrgAdminContext,
    p: Result<Path<(Uuid, Uuid)>, PathRejection>,
    q: Result<Query<h::Page>, QueryRejection>,
) -> Result<Response, ApiError> {
    let (id, key) = path(p)?;
    Ok(response(
        StatusCode::OK,
        recovery::mapping_field(
            s.db.as_ref().ok_or(ApiError::Unavailable)?,
            &s.raw_payload_key,
            &CommandContext::from_auth(&a.auth),
            id,
            key,
            query(q)?,
        )
        .await
        .map_err(error)?,
    ))
}
