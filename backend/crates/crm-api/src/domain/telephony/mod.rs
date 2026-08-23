//! The `call` aggregate (docs/specs/SLICE_006.md §2, §3): live state with
//! a pure state machine (`transitions`), one write path (`settle`) that
//! every signal source goes through, and the read queries. The provider
//! seam itself lives in `crate::telephony`.

pub mod dial_task;
pub mod queries;
pub mod settle;
pub mod sweep;
pub mod transitions;

use serde::Serialize;

pub use queries::{CallRow, CallView};
pub use settle::{settle, settle_in_tx, SettleOutcome};
pub use transitions::{apply, DialFailure, Signal, Transition};

/// `call.status` (docs/specs/SLICE_006.md §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallStatus {
    Placing,
    Ringing,
    Answered,
    Ended,
    Failed,
}

impl CallStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CallStatus::Placing => "placing",
            CallStatus::Ringing => "ringing",
            CallStatus::Answered => "answered",
            CallStatus::Ended => "ended",
            CallStatus::Failed => "failed",
        }
    }

    /// A read path fails closed on an unknown value (`None`), never panics.
    pub fn decode(s: &str) -> Option<Self> {
        match s {
            "placing" => Some(CallStatus::Placing),
            "ringing" => Some(CallStatus::Ringing),
            "answered" => Some(CallStatus::Answered),
            "ended" => Some(CallStatus::Ended),
            "failed" => Some(CallStatus::Failed),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, CallStatus::Ended | CallStatus::Failed)
    }
}

/// `call.failure_reason` / `call_completed.outcome` for a failed call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    NoAnswer,
    Busy,
    Declined,
    Cancelled,
    RingTimeout,
    AgentNotJoined,
    ProviderError,
    Expired,
}

impl FailureReason {
    pub fn as_str(self) -> &'static str {
        match self {
            FailureReason::NoAnswer => "no_answer",
            FailureReason::Busy => "busy",
            FailureReason::Declined => "declined",
            FailureReason::Cancelled => "cancelled",
            FailureReason::RingTimeout => "ring_timeout",
            FailureReason::AgentNotJoined => "agent_not_joined",
            FailureReason::ProviderError => "provider_error",
            FailureReason::Expired => "expired",
        }
    }

    pub fn decode(s: &str) -> Option<Self> {
        match s {
            "no_answer" => Some(FailureReason::NoAnswer),
            "busy" => Some(FailureReason::Busy),
            "declined" => Some(FailureReason::Declined),
            "cancelled" => Some(FailureReason::Cancelled),
            "ring_timeout" => Some(FailureReason::RingTimeout),
            "agent_not_joined" => Some(FailureReason::AgentNotJoined),
            "provider_error" => Some(FailureReason::ProviderError),
            "expired" => Some(FailureReason::Expired),
            _ => None,
        }
    }
}

/// `call.end_reason` for an ended (answered) call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    AgentHangup,
    AgentDisconnected,
    RemoteHangup,
    MaxDuration,
    Reconciled,
}

impl EndReason {
    pub fn as_str(self) -> &'static str {
        match self {
            EndReason::AgentHangup => "agent_hangup",
            EndReason::AgentDisconnected => "agent_disconnected",
            EndReason::RemoteHangup => "remote_hangup",
            EndReason::MaxDuration => "max_duration",
            EndReason::Reconciled => "reconciled",
        }
    }

    pub fn decode(s: &str) -> Option<Self> {
        match s {
            "agent_hangup" => Some(EndReason::AgentHangup),
            "agent_disconnected" => Some(EndReason::AgentDisconnected),
            "remote_hangup" => Some(EndReason::RemoteHangup),
            "max_duration" => Some(EndReason::MaxDuration),
            "reconciled" => Some(EndReason::Reconciled),
            _ => None,
        }
    }
}
