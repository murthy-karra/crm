pub mod auth;
pub mod config;
pub mod error;
pub mod operator;
pub mod routes;
pub mod state;
pub mod telemetry;

// The application layer moved to `crm-app` (docs/specs/SLICE_006a.md);
// these shims keep every existing `crm_api::…`/`crate::…` path valid.
pub use crm_app::{domain, realtime, telephony};

use axum::http::{HeaderName, HeaderValue, Method};
use axum::Router;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use config::Config;
use state::AppState;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub fn build_app(state: AppState) -> Router {
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
            .merge(routes::intake::router())
            .merge(routes::stages::router())
            .merge(routes::realtime::router())
            .merge(routes::today::router())
            .merge(routes::invitations::router())
            .merge(routes::platform::router())
            .merge(routes::operator::router())
            .merge(routes::calls::router())
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

/// The request-id + trace layer stack every route gets.
fn with_request_tracing(router: Router) -> Router {
    let request_id_header = HeaderName::from_static("x-request-id");

    // tower-http's span/on_request/on_response levels default to DEBUG,
    // which the default "info,sqlx=warn" filter (telemetry.rs) silently
    // drops — raise them to INFO so per-request logging is visible
    // without a RUST_LOG override. on_failure is left at its default
    // (already ERROR-level, already unfiltered).
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
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

    let state = AppState::new(&config)?;

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
