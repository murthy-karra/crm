//! Telephony provider seam (docs/specs/SLICE_006.md §3; D-002): the
//! `TelephonyProvider` trait is to calling what `Publisher` is to
//! realtime — LiveKit in production/dev, scripted in tests. Telnyx is not
//! a provider the application talks to: it is a LiveKit SIP outbound
//! trunk, configured once (§11). Nothing here stores or logs a phone
//! number, a join token, or the API secret (§2 PII rule, §7).

pub mod livekit;
pub mod token;
pub mod webhook;

#[cfg(any(test, feature = "test-support"))]
pub mod scripted;

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::config::Config;

pub use livekit::LiveKitProvider;
pub use token::{JoinGrant, JoinToken, JoinTokenSigner};
pub use webhook::{WebhookVerifier, WebhookVerifyError};

#[cfg(any(test, feature = "test-support"))]
pub use scripted::{RecordedCall, ScriptedProvider};

/// How long the dial task waits for `agent:<user_id>` to be present in
/// the room before settling `failed{agent_not_joined}`
/// (docs/specs/SLICE_006.md §3).
pub const DEFAULT_AGENT_JOIN_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound on the post-dial settle work (the presence re-check and the
/// settle itself) in the dial task's `10 s + ring_timeout + 10 s` budget.
pub const DIAL_SETTLE_GRACE: Duration = Duration::from_secs(10);
/// Presence polling cadence while waiting for the agent to join.
pub const DEFAULT_PRESENCE_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// An E.164-style number read from `contact_method.normalized_value` at
/// dial time (docs/specs/SLICE_006.md §2 PII rule). `Debug` is redacted so
/// a `{:?}` of a `DialRequest` — or of anything holding one — can never
/// put the number in a span or log line.
#[derive(Clone, PartialEq, Eq)]
pub struct PhoneNumber(String);

impl PhoneNumber {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The only way to read the number back; used by the LiveKit provider
    /// to build the `CreateSIPParticipant` request and by nothing else.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PhoneNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhoneNumber(REDACTED)")
    }
}

/// Provider failures (docs/specs/SLICE_006.md §3). `Timeout` and
/// `Unavailable` are "LiveKit could not be reached / did not answer in
/// time"; `Rejected` is "LiveKit answered with an error". The payloads are
/// short, PII-free descriptions fit for a span field (never a response
/// body or a participant attribute).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    Timeout,
    Unavailable(String),
    Rejected(String),
}

impl ProviderError {
    /// Stable tag for the `outcome` span field.
    pub fn kind(&self) -> &'static str {
        match self {
            ProviderError::Timeout => "timeout",
            ProviderError::Unavailable(_) => "unavailable",
            ProviderError::Rejected(_) => "rejected",
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Timeout => write!(f, "provider timeout"),
            ProviderError::Unavailable(detail) => write!(f, "provider unavailable: {detail}"),
            ProviderError::Rejected(detail) => write!(f, "provider rejected: {detail}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// How a SIP dial failed (docs/specs/SLICE_006.md §3). `Other` carries the
/// SIP status code so the dial task can record `sip_status_class`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SipFailure {
    Busy,
    Declined,
    NoAnswer,
    RingTimeout,
    Other(u16),
}

impl SipFailure {
    /// Maps a SIP status code the way §3 prescribes: 486 → busy, 603 →
    /// declined, 480/408 → no answer, anything else → `Other(code)`.
    pub fn from_sip_status(code: u16) -> Self {
        match code {
            486 => SipFailure::Busy,
            603 => SipFailure::Declined,
            480 | 408 => SipFailure::NoAnswer,
            other => SipFailure::Other(other),
        }
    }

    /// `1xx`…`6xx` for the `sip_status_class` span field; `ring_timeout`
    /// has no SIP status.
    pub fn status_class(self) -> Option<&'static str> {
        let code = match self {
            SipFailure::Busy => 486,
            SipFailure::Declined => 603,
            SipFailure::NoAnswer => 480,
            SipFailure::RingTimeout => return None,
            SipFailure::Other(code) => code,
        };
        Some(match code / 100 {
            1 => "1xx",
            2 => "2xx",
            3 => "3xx",
            4 => "4xx",
            5 => "5xx",
            6 => "6xx",
            _ => "unknown",
        })
    }
}

/// One outbound PSTN leg (docs/specs/SLICE_006.md §3).
#[derive(Debug, Clone)]
pub struct DialRequest {
    pub room: String,
    pub to_number: PhoneNumber,
    pub participant_identity: String,
    pub ring_timeout: Duration,
    pub max_call: Duration,
}

/// What `TelephonyProvider::dial` resolves to once the callee answers or
/// the leg fails (docs/specs/SLICE_006.md §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialOutcome {
    /// `call_ref` is the provider's own id for the leg (LiveKit
    /// `sip_call_id`), not PII.
    Answered {
        call_ref: Option<String>,
    },
    Failed(SipFailure),
}

/// The provider seam (docs/specs/SLICE_006.md §3). `dial` blocks until the
/// callee answers or the leg fails (up to `ring_timeout`); `hangup`
/// deletes the room and treats not-found as `Ok`.
#[async_trait]
pub trait TelephonyProvider: Send + Sync {
    async fn create_room(&self, room: &str, max_call: Duration) -> Result<(), ProviderError>;
    async fn participant_present(&self, room: &str, identity: &str) -> Result<bool, ProviderError>;
    async fn dial(&self, req: DialRequest) -> Result<DialOutcome, ProviderError>;
    async fn hangup(&self, room: &str) -> Result<(), ProviderError>;
}

/// The per-deployment limits (docs/specs/SLICE_006.md §11, §3): the three
/// configured values plus the fixed dial-task constants, overridable only
/// through `Telephony::for_tests`.
#[derive(Debug, Clone)]
pub struct TelephonyLimits {
    pub ring_timeout: Duration,
    pub max_call: Duration,
    pub join_ttl: Duration,
    pub agent_join_timeout: Duration,
    pub presence_poll_interval: Duration,
}

impl TelephonyLimits {
    pub fn from_config(config: &Config) -> Self {
        Self {
            ring_timeout: config.telephony.ring_timeout,
            max_call: config.telephony.max_call,
            join_ttl: config.telephony.join_ttl,
            agent_join_timeout: DEFAULT_AGENT_JOIN_TIMEOUT,
            presence_poll_interval: DEFAULT_PRESENCE_POLL_INTERVAL,
        }
    }
}

/// What `AppState.telephony` holds when calling is enabled
/// (docs/specs/SLICE_006.md §3).
pub struct Telephony {
    pub provider: Arc<dyn TelephonyProvider>,
    pub signer: JoinTokenSigner,
    pub webhook: WebhookVerifier,
    pub limits: TelephonyLimits,
    /// `'livekit' | 'scripted'` — the `call.provider` column.
    pub provider_name: &'static str,
    /// The browser signaling URL returned in the join grant.
    pub join_url: String,
}

impl Telephony {
    pub fn new(
        provider: Arc<dyn TelephonyProvider>,
        provider_name: &'static str,
        join_url: String,
        api_key: &str,
        api_secret: &[u8],
        limits: TelephonyLimits,
    ) -> Self {
        Self {
            provider,
            signer: JoinTokenSigner::new(api_key, api_secret),
            webhook: WebhookVerifier::new(api_key, api_secret),
            limits,
            provider_name,
            join_url,
        }
    }

    /// Test-support: a `Telephony` over any provider with explicit limits
    /// and a fixed key pair, mirroring `OperatorRuntime::with_provider`.
    pub fn with_provider(
        provider: Arc<dyn TelephonyProvider>,
        provider_name: &'static str,
        api_key: &str,
        api_secret: &[u8],
        limits: TelephonyLimits,
    ) -> Self {
        Self::new(
            provider,
            provider_name,
            "ws://127.0.0.1:7880".to_string(),
            api_key,
            api_secret,
            limits,
        )
    }

    /// `None` when `LIVEKIT_API_KEY` is unset (docs/specs/SLICE_006.md
    /// §9); otherwise the real `LiveKitProvider` over `LIVEKIT_API_URL`.
    pub fn from_config(config: &Config) -> Option<Self> {
        let livekit = config.telephony.livekit.as_ref()?;
        Some(Self::new(
            Arc::new(LiveKitProvider::new(livekit)),
            "livekit",
            livekit.url.clone(),
            &livekit.api_key,
            livekit.api_secret.as_bytes(),
            TelephonyLimits::from_config(config),
        ))
    }

    pub fn room_for(call_id: uuid::Uuid) -> String {
        format!("call:{call_id}")
    }

    pub fn agent_identity(user_id: uuid::Uuid) -> String {
        format!("agent:{user_id}")
    }

    pub fn sip_identity(call_id: uuid::Uuid) -> String {
        format!("sip:{call_id}")
    }
}

impl fmt::Debug for Telephony {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Telephony")
            .field("provider_name", &self.provider_name)
            .field("limits", &self.limits)
            .field("join_url", &self.join_url)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_number_debug_is_redacted() {
        let req = DialRequest {
            room: "call:x".into(),
            to_number: PhoneNumber::new("+15555550100".into()),
            participant_identity: "sip:x".into(),
            ring_timeout: Duration::from_secs(45),
            max_call: Duration::from_secs(3600),
        };
        let debug = format!("{req:?}");
        assert!(!debug.contains("5550100"));
        assert!(debug.contains("PhoneNumber(REDACTED)"));
        assert_eq!(req.to_number.expose(), "+15555550100");
    }

    #[test]
    fn sip_failure_mapping_from_status() {
        assert_eq!(SipFailure::from_sip_status(486), SipFailure::Busy);
        assert_eq!(SipFailure::from_sip_status(603), SipFailure::Declined);
        assert_eq!(SipFailure::from_sip_status(480), SipFailure::NoAnswer);
        assert_eq!(SipFailure::from_sip_status(408), SipFailure::NoAnswer);
        assert_eq!(SipFailure::from_sip_status(503), SipFailure::Other(503));
        assert_eq!(SipFailure::Other(503).status_class(), Some("5xx"));
        assert_eq!(SipFailure::Busy.status_class(), Some("4xx"));
        assert_eq!(SipFailure::Declined.status_class(), Some("6xx"));
        assert_eq!(SipFailure::RingTimeout.status_class(), None);
    }

    #[test]
    fn identities_and_room_names() {
        let id = uuid::Uuid::nil();
        assert_eq!(
            Telephony::room_for(id),
            "call:00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(
            Telephony::agent_identity(id),
            "agent:00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(
            Telephony::sip_identity(id),
            "sip:00000000-0000-0000-0000-000000000000"
        );
    }
}
