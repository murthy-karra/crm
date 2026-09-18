//! Opt-in diagnostic events. SQLx execution counts are not wire round trips:
//! BEGIN/savepoint creation and drop-triggered rollback bypass QueryLogger.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tracing::{
    field::{Field, Visit},
    span::{Attributes, Id},
    Event, Subscriber,
};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

pub(super) struct SqlProfile;
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

struct Request {
    id: u64,
    method: String,
    route: String,
    started_ms: u64,
    start: Instant,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn emit(value: serde_json::Value) {
    // A single line, including when multiple Tokio workers finish together.
    eprintln!("CRM_SQL_PROFILE {value}");
}

#[derive(Default)]
struct Fields {
    method: String,
    route: String,
    category: &'static str,
    elapsed_secs: Option<f64>,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "method" => self.method = value.to_owned(),
            "profile_route" => self.route = value.to_owned(),
            "summary" => self.category = category(value),
            _ => {} // Never retain SQL, parameters, URIs, or other event fields.
        }
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.elapsed_secs = Some(value);
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // SQLx records summary through tracing's string Value implementation.
        // Only the known HTTP method field needs the Display/Debug fallback.
        if field.name() == "method" {
            self.method = format!("{value:?}");
        }
    }
}

fn category(summary: &str) -> &'static str {
    if summary.contains("set_config(") {
        return "session_setup";
    }
    if summary.contains("crm_workspace_") {
        return "workspace_check";
    }
    match summary
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
        .as_str()
    {
        "SELECT" => "select",
        "INSERT" => "insert",
        "UPDATE" => "update",
        "DELETE" => "delete",
        "COMMIT" | "ROLLBACK" | "RELEASE" | "SAVEPOINT" | "BEGIN" => "transaction_control",
        "WITH" => "cte",
        _ => "other",
    }
}

impl<S> Layer<S> for SqlProfile
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if attrs.metadata().name() != "request" || attrs.metadata().target() != "crm_api" {
            return;
        }
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Request {
                id: NEXT_REQUEST.fetch_add(1, Ordering::Relaxed),
                method: fields.method,
                route: fields.route,
                started_ms: now_ms(),
                start: Instant::now(),
            });
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let request_id = ctx.event_scope(event).and_then(|scope| {
            scope
                .from_root()
                .find_map(|span| span.extensions().get::<Request>().map(|r| r.id))
        });
        emit(serde_json::json!({"kind": "query", "at_ms": now_ms(),
            "request_id": request_id, "category": fields.category,
            "elapsed_ms": fields.elapsed_secs.map(|seconds| seconds * 1000.0)}));
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(&id) {
            if let Some(request) = span.extensions().get::<Request>() {
                emit(
                    serde_json::json!({"kind": "request", "request_id": request.id,
                    "method": request.method, "route": request.route,
                    "started_ms": request.started_ms, "finished_ms": now_ms(),
                    "elapsed_ms": request.start.elapsed().as_secs_f64() * 1000.0}),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_fields_keep_only_categories_and_numeric_timing() {
        struct Check;
        impl<S: Subscriber> Layer<S> for Check {
            fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
                let mut fields = Fields::default();
                event.record(&mut fields);
                assert_eq!(fields.category, "select");
                assert_eq!(fields.elapsed_secs, Some(0.25));
                assert!(fields.method.is_empty());
                assert!(fields.route.is_empty());
            }
        }
        use tracing_subscriber::prelude::*;
        tracing::subscriber::with_default(tracing_subscriber::registry().with(Check), || {
            tracing::debug!(target: "sqlx::query", summary = "SELECT secret FROM sensitive",
                db.statement = "SELECT 'never retain this'", password = "never retain this",
                elapsed_secs = 0.25);
        });
        assert_eq!(
            category("SELECT set_config('secret', 'value')"),
            "session_setup"
        );
    }
}
