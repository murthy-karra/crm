use tracing_subscriber::EnvFilter;

/// Console-only tracing setup. This is the single choke point where an OTLP
/// exporter attaches once a collector exists (D-016 §9) — nothing else in
/// this crate should call `tracing_subscriber` directly.
pub fn init() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();

    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "crm-api".to_string());
    tracing::info!(service.name = %service_name, "telemetry initialized");
}
