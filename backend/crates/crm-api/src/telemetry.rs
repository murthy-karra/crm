use tracing_subscriber::EnvFilter;

mod sql_profile;

/// Console-only tracing setup. This is the single choke point where an OTLP
/// exporter attaches once a collector exists (D-016 §9) — nothing else in
/// this crate should call `tracing_subscriber` directly.
pub fn init() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));

    if std::env::var("CRM_SQL_PROFILE").as_deref() == Ok("1") {
        use tracing_subscriber::{
            filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt, Layer,
        };
        // SQLx's normal formatter includes SQL text. The profiling layer only
        // retains an allowlist of numeric fields and statement categories.
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(true)
                    .with_filter(filter_fn(|m| m.target() != "sqlx::query"))
                    .with_filter(filter),
            )
            .with(sql_profile::SqlProfile.with_filter(filter_fn(|m| {
                m.target() == "sqlx::query" || (m.name() == "request" && m.target() == "crm_api")
            })))
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .init();
    }

    let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "crm-api".to_string());
    tracing::info!(service.name = %service_name, "telemetry initialized");
}
