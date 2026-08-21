pub mod auth;
pub mod config;
pub mod error;
pub mod routes;
pub mod state;
pub mod telemetry;

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

    // Read before `state` moves into `.with_state` below.
    let cors_allowed_origin = state.cors_allowed_origin.clone();

    let app = Router::new()
        .merge(routes::health::router())
        .merge(routes::session::router())
        .merge(routes::organization::router())
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::new(
                    request_id_header.clone(),
                    MakeRequestUuid,
                ))
                .layer(trace_layer)
                .layer(PropagateRequestIdLayer::new(request_id_header)),
        );

    // Only present for the cross-subdomain tunnel case (config::Config::
    // cors_allowed_origin doc comment); same-origin loopback dev adds no
    // CORS layer at all — no behavior change, not even OPTIONS-preflight
    // interception — from Slice 001's original posture.
    match cors_allowed_origin {
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
                .allow_methods([Method::GET, Method::POST, Method::DELETE])
                .allow_headers([axum::http::header::CONTENT_TYPE]);
            app.layer(cors_layer)
        }
        None => app,
    }
}

pub async fn run(config: Config) -> Result<(), BoxError> {
    telemetry::init();

    let state = AppState::new(&config)?;
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
