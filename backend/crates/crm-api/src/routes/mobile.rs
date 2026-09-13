//! Native HTTP adapter; all authority and commands live in crm-app.
use crate::{
    auth::AuthContext,
    domain::mobile::{self, MobileError},
    state::AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        DefaultBodyLimit, Path, Query, State,
    },
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/mobile/v1/bootstrap", post(bootstrap))
        .route(
            "/api/mobile/v1/operations",
            post(operation).layer(DefaultBodyLimit::max(128 * 1024)),
        )
        .route("/api/mobile/v1/operations/{id}", get(receipt))
        .route("/api/mobile/v1/reconciliations", post(reconcile))
        .route(
            "/api/mobile/v1/reconciliations/{id}/manifest",
            get(manifest),
        )
        .route(
            "/api/mobile/v1/reconciliations/{id}/people/{person}/{section}",
            get(component),
        )
        .route("/api/mobile/v1/reconciliations/{id}/seal", post(seal))
        .layer(middleware::from_fn(no_store))
}
async fn no_store(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}
struct Error(MobileError);
impl From<MobileError> for Error {
    fn from(e: MobileError) -> Self {
        Self(e)
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, code) = self.0.code();
        tracing::warn!(error_kind = code, "mobile request failed");
        let mut body = json!({"error":code});
        if let Some(changes) = self.0.changes() {
            body["changes"] = changes;
        }
        let mut response = (
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(body),
        )
            .into_response();
        if status == 429 {
            response
                .headers_mut()
                .insert("retry-after", HeaderValue::from_static("30"));
        }
        response
    }
}
fn context(headers: &HeaderMap) -> Result<Uuid, Error> {
    headers
        .get("x-mobile-context")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or(Error(MobileError::Code(401, "unauthenticated")))
}
fn dependencies(state: &AppState) -> Result<(&sqlx::PgPool, &mobile::ReceiptKeys), Error> {
    Ok((
        state
            .db
            .as_ref()
            .ok_or(Error(MobileError::Code(503, "unavailable")))?,
        state
            .mobile_receipt_keys
            .as_deref()
            .ok_or(Error(MobileError::Code(503, "mobile_unavailable")))?,
    ))
}
fn body<T>(value: Result<Json<T>, JsonRejection>) -> Result<T, Error> {
    value
        .map(|v| v.0)
        .map_err(|_| Error(MobileError::Code(400, "malformed_request")))
}
async fn bootstrap(
    State(state): State<AppState>,
    auth: AuthContext,
    input: Result<Json<mobile::BootstrapRequest>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let (pool, _) = dependencies(&state)?;
    Ok(Json(mobile::bootstrap(pool, &auth, body(input)?).await?))
}
async fn operation(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    input: Result<Json<mobile::Operation>, JsonRejection>,
) -> Result<Json<mobile::Receipt>, Error> {
    let (pool, keys) = dependencies(&state)?;
    Ok(Json(
        mobile::execute(
            pool,
            &state.publisher,
            keys,
            &auth,
            context(&headers)?,
            body(input)?,
        )
        .await?,
    ))
}
async fn receipt(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<mobile::Receipt>, Error> {
    let Path(id) = path.map_err(|_| Error(MobileError::Code(400, "malformed_request")))?;
    let (pool, _) = dependencies(&state)?;
    Ok(Json(
        mobile::lookup_receipt(pool, &auth, context(&headers)?, id).await?,
    ))
}
async fn reconcile(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    input: Result<Json<mobile::ReconciliationRequest>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let (pool, keys) = dependencies(&state)?;
    Ok(Json(
        mobile::create_generation(pool, keys, &auth, context(&headers)?, body(input)?).await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorQuery {
    cursor: Option<String>,
}
async fn manifest(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<CursorQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let Path(id) = path.map_err(|_| Error(MobileError::Code(400, "malformed_request")))?;
    let Query(query) = query.map_err(|_| Error(MobileError::Code(400, "malformed_request")))?;
    let (pool, keys) = dependencies(&state)?;
    Ok(Json(
        mobile::manifest(
            pool,
            keys,
            &auth,
            context(&headers)?,
            id,
            query.cursor.as_deref(),
        )
        .await?,
    ))
}
async fn component(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    path: Result<Path<(Uuid, Uuid, String)>, PathRejection>,
    query: Result<Query<CursorQuery>, QueryRejection>,
) -> Result<Json<Value>, Error> {
    let Path((id, person, section)) =
        path.map_err(|_| Error(MobileError::Code(400, "malformed_request")))?;
    let Query(query) = query.map_err(|_| Error(MobileError::Code(400, "malformed_request")))?;
    let (pool, keys) = dependencies(&state)?;
    Ok(Json(
        mobile::component(
            pool,
            keys,
            &auth,
            context(&headers)?,
            id,
            person,
            &section,
            query.cursor.as_deref(),
        )
        .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
async fn seal(
    State(state): State<AppState>,
    auth: AuthContext,
    headers: HeaderMap,
    path: Result<Path<Uuid>, PathRejection>,
    input: Result<Json<Empty>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let Path(id) = path.map_err(|_| Error(MobileError::Code(400, "malformed_request")))?;
    body(input)?;
    let (pool, _) = dependencies(&state)?;
    Ok(Json(
        mobile::seal(pool, &auth, context(&headers)?, id).await?,
    ))
}
