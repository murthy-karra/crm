//! Best-effort Centrifugo publisher (docs/specs/SLICE_003.md §6, §9;
//! D-023 item 3). `publish_after_commit` runs only after the triggering
//! command's own transaction has already committed: PostgreSQL is
//! authoritative by construction, and a failed publish is logged, never a
//! failed command (delivery is at-most-once; correctness never depends on
//! it).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::Instrument;

use crate::config::CentrifugoApiKey;
use crate::ids::UserId;
use crate::realtime::events::Publication;

/// Connect timeout for the publish HTTP call (docs/specs/SLICE_003.md
/// §9).
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
/// Total wall-clock budget for the publish attempt, including connect
/// (docs/specs/SLICE_003.md §9).
const TOTAL_BUDGET: Duration = Duration::from_secs(2);

/// A real Centrifugo HTTP API transport.
#[derive(Clone)]
pub struct CentrifugoTransport {
    http: reqwest::Client,
    /// `CRM_CENTRIFUGO_API_URL`, e.g. `http://127.0.0.1:8000/api` — no
    /// trailing slash (`Config` validates this).
    api_url: String,
    api_key: String,
}

impl CentrifugoTransport {
    pub fn new(api_url: String, api_key: &CentrifugoApiKey) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("reqwest client builds with static, valid configuration");
        Self {
            http,
            api_url,
            api_key: api_key.as_str().to_string(),
        }
    }

    /// Test-support constructor: a transport with no connect-timeout
    /// override, pointed at an arbitrary URL — used to build a transport
    /// against a closed loopback port or a hung listener
    /// (docs/specs/SLICE_003.md §13 criterion 6).
    pub fn for_tests(api_url: String, api_key: &str) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .expect("reqwest client builds with static, valid configuration");
        Self {
            http,
            api_url,
            api_key: api_key.to_string(),
        }
    }
}

/// A stable, PII-free outcome tag for the `outcome` span field
/// (docs/specs/SLICE_003.md §8): `published | timeout | transport_error |
/// api_error:<code>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    Published,
    Timeout,
    TransportError,
    ApiError(String),
}

impl PublishOutcome {
    pub fn as_tag(&self) -> String {
        match self {
            PublishOutcome::Published => "published".to_string(),
            PublishOutcome::Timeout => "timeout".to_string(),
            PublishOutcome::TransportError => "transport_error".to_string(),
            PublishOutcome::ApiError(code) => format!("api_error:{code}"),
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, PublishOutcome::Published)
    }
}

/// `Recording`'s captured wire form: `(channel, serde_json::Value)` exactly
/// as sent, not a Rust struct, so a test pins the contract Lane B consumes
/// (docs/specs/SLICE_003.md §14a).
pub type RecordedPublication = (String, serde_json::Value);

/// A recorded `disconnect_user` call — `Recording`'s capture of
/// `Publisher::disconnect_user` (docs/specs/SLICE_004.md §13 criterion 8).
pub type RecordedDisconnect = UserId;

/// `Centrifugo`: publishes over HTTP. `Recording`: captures publications
/// in memory for DB-backed tests that don't need a running Centrifugo
/// (docs/specs/SLICE_003.md §4). `Disabled`: no-op, for the `crm-admin`
/// CLI (docs/specs/SLICE_003.md §14a; docs/specs/SLICE_004.md §6, §11) —
/// CLI subcommands run offline from Centrifugo and never need realtime.
#[derive(Clone)]
pub enum Publisher {
    Centrifugo(CentrifugoTransport),
    Recording(
        Arc<Mutex<Vec<RecordedPublication>>>,
        Arc<Mutex<Vec<RecordedDisconnect>>>,
    ),
    Disabled,
}

impl Publisher {
    pub fn recording() -> Self {
        Publisher::Recording(
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
        )
    }

    /// Publishes now, awaiting the outcome. This is the spawned body of
    /// `publish_after_commit`'s `Centrifugo` branch and is used directly by
    /// failure-path tests (docs/specs/SLICE_003.md §13 criterion 6, §4).
    /// `Recording` is synchronous by construction (spec §6 module layout).
    pub async fn publish_now(&self, publication: &Publication) -> PublishOutcome {
        let value = serde_json::to_value(publication.event())
            .expect("RealtimeEvent serialization to JSON cannot fail");
        match self {
            Publisher::Recording(recorded, _) => {
                recorded
                    .lock()
                    .await
                    .push((publication.channel().to_string(), value));
                PublishOutcome::Published
            }
            Publisher::Centrifugo(transport) => {
                publish_via_http(transport, publication.channel(), &value).await
            }
            Publisher::Disabled => PublishOutcome::Published,
        }
    }

    /// Publishes off the request path after the triggering command's
    /// transaction has committed (docs/specs/SLICE_003.md §9): `Recording`
    /// runs synchronously (so DB-backed tests can assert immediately after
    /// the command returns); `Centrifugo` spawns a detached task, with the
    /// tracing span it was called under preserved via `.instrument` so
    /// `correlation_id` reaches the eventual `warn!`/`debug!`
    /// (docs/specs/SLICE_003.md §8, §14a).
    pub async fn publish_after_commit(&self, publication: Publication) {
        match self {
            Publisher::Recording(..) | Publisher::Disabled => {
                self.publish_now(&publication).await;
            }
            Publisher::Centrifugo(_) => {
                let publisher = self.clone();
                let channel = publication.channel().to_string();
                let event_type = publication.event().type_tag();
                let organization_id = publication.event().organization_id();
                let correlation_id = publication.event().correlation_id();
                let span = tracing::info_span!(
                    "realtime_publish",
                    channel = %channel,
                    event_type = event_type,
                    organization_id = %organization_id,
                    correlation_id = %correlation_id,
                    outcome = tracing::field::Empty,
                );
                tokio::spawn(
                    async move {
                        let outcome = publisher.publish_now(&publication).await;
                        tracing::Span::current().record("outcome", outcome.as_tag().as_str());
                        if outcome.is_success() {
                            tracing::debug!("realtime publish succeeded");
                        } else {
                            tracing::warn!("realtime publish failed");
                        }
                    }
                    .instrument(span),
                );
            }
        }
    }

    /// Disconnects `user_id`'s realtime connection via Centrifugo's
    /// `disconnect` HTTP API, keyed by the `sub` claim (= user id) minted
    /// into every connection token (docs/specs/SLICE_004.md §6). Called
    /// after commit by `SetMemberStatus(inactive)`; best-effort — a
    /// failure is a `warn`, never propagated, because the session is
    /// already revoked and the client's next token refresh (≤ the
    /// connection-token TTL) cuts the connection anyway
    /// (docs/specs/SLICE_003.md §7). `Recording` captures the call for
    /// tests instead of making one; `Disabled` no-ops (the CLI never has a
    /// live Centrifugo to disconnect from).
    pub async fn disconnect_user(&self, user_id: UserId) {
        match self {
            Publisher::Recording(_, disconnects) => {
                disconnects.lock().await.push(user_id);
            }
            Publisher::Disabled => {}
            Publisher::Centrifugo(transport) => {
                let url = format!("{}/disconnect", transport.api_url);
                let body = serde_json::json!({ "user": user_id.to_string() });
                let attempt = async {
                    transport
                        .http
                        .post(&url)
                        .header("X-API-Key", &transport.api_key)
                        .json(&body)
                        .send()
                        .await
                };
                match tokio::time::timeout(TOTAL_BUDGET, attempt).await {
                    Ok(Ok(response)) if response.status().is_success() => {
                        tracing::debug!(%user_id, "realtime disconnect succeeded");
                    }
                    Ok(Ok(response)) => {
                        tracing::warn!(
                            %user_id,
                            status = response.status().as_u16(),
                            "realtime disconnect failed: api error"
                        );
                    }
                    Ok(Err(_)) => {
                        tracing::warn!(%user_id, "realtime disconnect failed: transport error");
                    }
                    Err(_) => {
                        tracing::warn!(%user_id, "realtime disconnect failed: timeout");
                    }
                }
            }
        }
    }
}

async fn publish_via_http(
    transport: &CentrifugoTransport,
    channel: &str,
    data: &serde_json::Value,
) -> PublishOutcome {
    let url = format!("{}/publish", transport.api_url);
    let body = serde_json::json!({ "channel": channel, "data": data });

    let attempt = async {
        transport
            .http
            .post(&url)
            .header("X-API-Key", &transport.api_key)
            .json(&body)
            .send()
            .await
    };

    match tokio::time::timeout(TOTAL_BUDGET, attempt).await {
        Ok(Ok(response)) if response.status().is_success() => PublishOutcome::Published,
        Ok(Ok(response)) => PublishOutcome::ApiError(response.status().as_u16().to_string()),
        Ok(Err(_)) => PublishOutcome::TransportError,
        Err(_) => PublishOutcome::Timeout,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::OrganizationId;
    use crate::realtime::events::RealtimeEvent;
    use uuid::Uuid;

    fn sample_publication() -> Publication {
        let event = RealtimeEvent::intake_unresolved_changed(
            OrganizationId::new(Uuid::new_v4()),
            chrono::Utc::now(),
            crate::ids::CorrelationId::new(Uuid::new_v4()),
            crate::ids::RawPayloadId::new(Uuid::new_v4()),
        );
        Publication::for_event(event)
    }

    #[tokio::test]
    async fn recording_publisher_captures_channel_and_exact_wire_value() {
        let publisher = Publisher::recording();
        let publication = sample_publication();
        let expected_channel = publication.channel().to_string();
        let expected_value = serde_json::to_value(publication.event()).unwrap();

        let outcome = publisher.publish_now(&publication).await;
        assert_eq!(outcome, PublishOutcome::Published);

        let Publisher::Recording(recorded, _) = &publisher else {
            unreachable!()
        };
        let recorded = recorded.lock().await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, expected_channel);
        assert_eq!(recorded[0].1, expected_value);
    }

    #[tokio::test]
    async fn recording_publisher_is_synchronous_via_publish_after_commit() {
        let publisher = Publisher::recording();
        publisher.publish_after_commit(sample_publication()).await;

        let Publisher::Recording(recorded, _) = &publisher else {
            unreachable!()
        };
        // No await/sleep needed: publish_after_commit already awaited the
        // synchronous Recording path before returning.
        assert_eq!(recorded.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn publish_now_against_a_closed_port_returns_transport_error_within_budget() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // now nothing is listening: connection refused

        let transport = CentrifugoTransport::for_tests(format!("http://{addr}"), "unused-test-key");
        let publisher = Publisher::Centrifugo(transport);
        let publication = sample_publication();

        let started = std::time::Instant::now();
        let outcome = publisher.publish_now(&publication).await;
        assert!(started.elapsed() < TOTAL_BUDGET + Duration::from_secs(1));
        assert_eq!(outcome, PublishOutcome::TransportError);
    }

    #[tokio::test]
    async fn publish_now_against_a_hung_listener_times_out_within_budget() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Accept connections but never write a response, so the client's
        // read hangs until our own budget expires.
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                // Keep the connection open without responding.
                std::mem::forget(socket);
            }
        });

        let transport = CentrifugoTransport::for_tests(format!("http://{addr}"), "unused-test-key");
        let publisher = Publisher::Centrifugo(transport);
        let publication = sample_publication();

        let started = std::time::Instant::now();
        let outcome = publisher.publish_now(&publication).await;
        let elapsed = started.elapsed();
        assert!(
            elapsed < TOTAL_BUDGET + Duration::from_secs(1),
            "took {elapsed:?}, expected to fail within the {TOTAL_BUDGET:?} budget"
        );
        assert!(matches!(
            outcome,
            PublishOutcome::Timeout | PublishOutcome::TransportError
        ));
    }

    #[tokio::test]
    async fn recording_publisher_captures_disconnect_user() {
        let publisher = Publisher::recording();
        let user_id = UserId::new(Uuid::new_v4());
        publisher.disconnect_user(user_id).await;

        let Publisher::Recording(_, disconnects) = &publisher else {
            unreachable!()
        };
        assert_eq!(disconnects.lock().await.clone(), vec![user_id]);
    }

    #[tokio::test]
    async fn disabled_publisher_disconnect_user_is_a_noop() {
        let publisher = Publisher::Disabled;
        // Must not panic or block.
        publisher.disconnect_user(UserId::new(Uuid::new_v4())).await;
    }

    #[test]
    fn outcome_tags_match_spec_vocabulary() {
        assert_eq!(PublishOutcome::Published.as_tag(), "published");
        assert_eq!(PublishOutcome::Timeout.as_tag(), "timeout");
        assert_eq!(PublishOutcome::TransportError.as_tag(), "transport_error");
        assert_eq!(
            PublishOutcome::ApiError("500".to_string()).as_tag(),
            "api_error:500"
        );
    }
}
