pub mod auth;
pub mod config;
pub mod error;
pub mod extraction;
pub mod operator;
pub mod routes;
pub mod state;
pub mod telemetry;

// The application layer moved to `crm-app` (docs/specs/SLICE_006a.md);
// these shims keep every existing `crm_api::…`/`crate::…` path valid.
pub use crm_app::{domain, ids, realtime, telephony};

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, Method};
#[cfg(feature = "test-support")]
use axum::middleware::{self, Next};
#[cfg(feature = "test-support")]
use axum::response::Response;
use axum::Router;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{Level, Span};

use config::Config;
use state::AppState;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub fn build_app(state: AppState) -> Router {
    build_app_with_today_router_inner(state, routes::today::router())
}

/// Test-support-only application builder for the Phase B frozen-baseline
/// comparison. It preserves the normal request-id, trace, session and auth
/// stack while substituting only `GET /api/today`; no production caller can
/// select a baseline or test clock.
#[cfg(feature = "test-support")]
pub fn build_app_with_today_router(state: AppState, today_router: Router<AppState>) -> Router {
    build_app_with_today_router_inner(state, today_router)
}

/// Test-only Phase B wrapper. The numeric header is accepted only by this
/// feature-gated builder and scopes safe timing collection around the entire
/// request, including auth extraction and the Today handler. It has no
/// production route, input, logging, or persistence effect.
#[cfg(feature = "test-support")]
pub fn build_app_with_today_router_and_perf_collector(
    state: AppState,
    today_router: Router<AppState>,
    collector: crate::domain::today::test_support::HttpPerfCollector,
) -> Router {
    build_app_with_today_router_inner(state, today_router).layer(middleware::from_fn_with_state(
        collector,
        perf_capture_middleware,
    ))
}

#[cfg(feature = "test-support")]
async fn perf_capture_middleware(
    axum::extract::State(collector): axum::extract::State<
        crate::domain::today::test_support::HttpPerfCollector,
    >,
    request: Request,
    next: Next,
) -> Response {
    let capture_id = request
        .headers()
        .get("x-crm-perf-capture")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    match capture_id {
        Some(capture_id) => collector.scope(capture_id, next.run(request)).await,
        None => next.run(request).await,
    }
}

fn build_app_with_today_router_inner(state: AppState, today_router: Router<AppState>) -> Router {
    // Read before `state` moves into `.with_state` below.
    let cors_allowed_origin = state.cors_allowed_origin.clone();

    // `POST /webhooks/livekit` and `POST /inbound/email` are built as their
    // own routers, outside the CORS layer (docs/specs/SLICE_006.md §5, §7;
    // docs/specs/SLICE_007b.md §5): both are server-to-server calls with
    // their own auth scheme, not browser routes. They still get the
    // request-id/trace layers.
    let webhook = with_request_tracing(routes::livekit_webhook::router().with_state(state.clone()));
    let inbound_email =
        with_request_tracing(routes::inbound_email::router().with_state(state.clone()));

    let app = with_request_tracing(
        Router::new()
            .merge(routes::health::router())
            .merge(routes::session::router())
            .merge(routes::organization::router())
            .merge(routes::people::router())
            .merge(routes::saved_lists::router())
            .merge(routes::today_feeds::router())
            .merge(routes::inquiry_sources::router())
            .merge(routes::intake::router())
            .merge(routes::stages::router())
            .merge(routes::realtime::router())
            .merge(today_router)
            .merge(routes::invitations::router())
            .merge(routes::platform::router())
            .merge(routes::operator::router())
            .merge(routes::calls::router())
            .merge(routes::capture::router())
            .merge(routes::tags::router())
            .merge(routes::notes::router())
            .merge(routes::tasks::router())
            .merge(routes::custom_fields::router())
            .merge(routes::migrations::router())
            .merge(routes::migration_imports::router())
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::workspace_http::guard,
            ))
            .with_state(state),
    );

    // Only present for the cross-subdomain tunnel case (config::Config::
    // cors_allowed_origin doc comment); same-origin loopback dev adds no
    // CORS layer at all — no behavior change, not even OPTIONS-preflight
    // interception — from Slice 001's original posture.
    let app = match cors_allowed_origin {
        Some(origin) => {
            let origin_value = HeaderValue::from_str(&origin)
                .expect("cors_allowed_origin was already validated by Config::from_source");
            // AllowOrigin::list (not a bare HeaderValue, which uses
            // AllowOrigin::exact and echoes the configured origin back
            // unconditionally) so the header is present only when the
            // request's own Origin actually matches.
            let cors_layer = CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::list([origin_value]))
                .allow_credentials(true)
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([axum::http::header::CONTENT_TYPE]);
            app.layer(cors_layer)
        }
        None => app,
    };

    app.merge(webhook).merge(inbound_email)
}

/// Declared slice-owned trace-layer change (docs/specs/SLICE_011a.md §7,
/// review fix F1). tower-http's `DefaultMakeSpan` records the FULL URI —
/// query string included — so a `GET /api/people?filter=<JSON>` request
/// would land the filter JSON in every request span verbatim, violating
/// §7's "no clause values, ids, sources, or day counts in spans" posture.
/// This replaces it: PATH ONLY (`request.uri().path()`, query stripped),
/// for ALL routes — every other recorded field (`method`, `version`) stays
/// exactly as `DefaultMakeSpan` recorded it. `filter_kinds`/
/// `filter_clause_count` are declared here as `Empty` so `routes::people`
/// can `Span::current().record(...)` them without every other route
/// paying for unused fields being anything but absent from their output.
/// `sort` (docs/specs/SLICE_011b_SORT.md §6) is declared alongside them for
/// the same reason: the static sort token only, never a clause value.
fn make_span_with(request: &Request) -> Span {
    tracing::span!(
        Level::INFO,
        "request",
        method = %request.method(),
        uri = %request.uri().path(),
        version = ?request.version(),
        filter_kinds = tracing::field::Empty,
        filter_clause_count = tracing::field::Empty,
        sort = tracing::field::Empty,
    )
}

/// The request-id + trace layer stack every route gets.
fn with_request_tracing(router: Router) -> Router {
    let request_id_header = HeaderName::from_static("x-request-id");

    // tower-http's span/on_request/on_response levels default to DEBUG,
    // which the default "info,sqlx=warn" filter (telemetry.rs) silently
    // drops — raise them to INFO so per-request logging is visible
    // without a RUST_LOG override. on_failure is left at its default
    // (already ERROR-level, already unfiltered).
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(make_span_with)
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    router.layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::new(
                request_id_header.clone(),
                MakeRequestUuid,
            ))
            .layer(trace_layer)
            .layer(PropagateRequestIdLayer::new(request_id_header)),
    )
}

pub async fn run(config: Config) -> Result<(), BoxError> {
    telemetry::init();

    let mut state = AppState::new(&config)?;
    auth::workspace::artifact_fingerprint().await?;
    if let Some(pool) = &state.db {
        auth::workspace::startup_compatible(&mut *pool.acquire().await?).await?;
        if let Some(path) = std::env::var_os("CRM_MIGRATION_RELEASE_REPORT") {
            state.import_release_path = Some(std::path::PathBuf::from(path));
        }
    }

    // The call sweep (docs/specs/SLICE_006.md §3): in-process, only when
    // calling is enabled and a database is configured. Never started by
    // `build_app`, so the test router is sweep-free.
    let _sweep = match (&state.db, &state.telephony) {
        (Some(pool), Some(telephony)) => Some(domain::telephony::sweep::spawn(
            pool.clone(),
            state.publisher.clone(),
            telephony.clone(),
        )),
        _ => None,
    };

    // The extraction worker (docs/specs/SLICE_007f.md §4): in-process,
    // only when a database is configured AND GROQ_API_KEY is set — the
    // same gate that enables the Operator. Unset key: rows simply wait,
    // externally identical to provider-down. Never started by
    // `build_app`, so the test router is worker-free.
    let _extraction = match (
        &state.db,
        extraction::GroqLeadExtractor::from_config(&config, &config.extraction),
    ) {
        (Some(pool), Some(extractor)) => Some(domain::intake::extraction::worker::spawn(
            pool.clone(),
            config.raw_payload_key.clone(),
            state.publisher.clone(),
            std::sync::Arc::new(extractor),
            config.extraction.poll_interval,
        )),
        (Some(_), None) => {
            tracing::info!("extraction worker disabled: GROQ_API_KEY is not set");
            None
        }
        _ => None,
    };

    // Slice 010a's bounded assessment worker is independent of the Operator
    // and begins only when a database is configured. It holds no connection
    // while making a FUB request.
    let _migration_worker = state.db.as_ref().map(|pool| {
        domain::migration::worker::spawn(
            pool.clone(),
            config.raw_payload_key.clone(),
            state.migration_reader.clone(),
        )
    });

    let _snapshot_worker = state.db.as_ref().map(|pool| {
        domain::migration::snapshot_worker::spawn(
            pool.clone(),
            config.raw_payload_key.clone(),
            state.migration_reader.clone(),
            state.snapshot_policy.clone(),
        )
    });
    let _people_import_worker = state.db.as_ref().map(|pool| {
        domain::migration::import_worker::spawn(
            pool.clone(),
            config.raw_payload_key.clone(),
            config.snapshot_policy.clone(),
        )
    });
    let app = build_app(state);

    let listener = TcpListener::bind(config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}
