//! The call state machine (docs/specs/SLICE_006.md §2), pure: `apply`
//! maps a locked status and a signal to a `Transition`; `settle` does the
//! writing. The D-031 attempt mapping and the SIP-failure → signal
//! mapping are pure functions here too, so every state × signal cell is
//! unit-testable without a database.

use crate::domain::commands::ContactOutcome;
use crate::domain::telephony::{CallStatus, EndReason, FailureReason};
use crate::telephony::SipFailure;

/// The four SIP failures the dial task maps to a `failed{…}` reason with
/// a `no_answer` attempt (docs/specs/SLICE_006.md §2). Anything else is a
/// `ProviderError` signal (no attempt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialFailure {
    Busy,
    Declined,
    NoAnswer,
    RingTimeout,
}

impl DialFailure {
    pub fn reason(self) -> FailureReason {
        match self {
            DialFailure::Busy => FailureReason::Busy,
            DialFailure::Declined => FailureReason::Declined,
            DialFailure::NoAnswer => FailureReason::NoAnswer,
            DialFailure::RingTimeout => FailureReason::RingTimeout,
        }
    }
}

/// Every signal a call can receive, from any source (command, dial task,
/// webhook, sweep). Source annotations per the §2 table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// Dial task, after the presence check.
    Dialing,
    /// Dial task: the callee answered; `call_ref` = provider leg id.
    Answered { call_ref: Option<String> },
    /// Dial task: SIP busy / declined / no answer / ring timeout.
    DialFailed(DialFailure),
    /// `hangup` command before `answered` (mic denied, user cancelled).
    Cancelled,
    /// `hangup` command: `answered → ended{agent_hangup}`;
    /// `placing|ringing → failed{cancelled}`.
    AgentHangup,
    /// Webhook: `participant_left` for `agent:*`.
    AgentLeft,
    /// Dial task: the agent never joined within the join timeout.
    AgentNotJoined,
    /// Dial task / `start_call`: the provider failed.
    ProviderError,
    /// Webhook: `participant_left` for `sip:*`, or the dial task's
    /// post-answer presence re-check.
    RemoteLeft,
    /// Webhook: `room_finished`.
    RoomFinished,
    /// Provider reported the max-duration cut distinguishably.
    MaxDuration,
    /// Sweep: `placing|ringing` past their horizon.
    Expired,
    /// Sweep: `answered` past `max_call + 60 s`.
    Reconciled,
}

impl Signal {
    /// §3 mapping of a SIP failure: 486 → busy, 603 → declined, 480/408 →
    /// no answer, ring timeout → ring timeout, other → provider error.
    pub fn from_sip_failure(failure: SipFailure) -> Self {
        match failure {
            SipFailure::Busy => Signal::DialFailed(DialFailure::Busy),
            SipFailure::Declined => Signal::DialFailed(DialFailure::Declined),
            SipFailure::NoAnswer => Signal::DialFailed(DialFailure::NoAnswer),
            SipFailure::RingTimeout => Signal::DialFailed(DialFailure::RingTimeout),
            SipFailure::Other(_) => Signal::ProviderError,
        }
    }

    /// Stable tag for span fields.
    pub fn kind(&self) -> &'static str {
        match self {
            Signal::Dialing => "dialing",
            Signal::Answered { .. } => "answered",
            Signal::DialFailed(DialFailure::Busy) => "dial_failed_busy",
            Signal::DialFailed(DialFailure::Declined) => "dial_failed_declined",
            Signal::DialFailed(DialFailure::NoAnswer) => "dial_failed_no_answer",
            Signal::DialFailed(DialFailure::RingTimeout) => "dial_failed_ring_timeout",
            Signal::Cancelled => "cancelled",
            Signal::AgentHangup => "agent_hangup",
            Signal::AgentLeft => "agent_left",
            Signal::AgentNotJoined => "agent_not_joined",
            Signal::ProviderError => "provider_error",
            Signal::RemoteLeft => "remote_left",
            Signal::RoomFinished => "room_finished",
            Signal::MaxDuration => "max_duration",
            Signal::Expired => "expired",
            Signal::Reconciled => "reconciled",
        }
    }
}

/// What `apply` decides. `attempt` on the failed/answered variants is the
/// D-031 contact attempt `settle` writes in the same transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// Idempotent absorb: nothing is written, nothing is published.
    NoOp,
    /// `placing → ringing`.
    Ringing,
    /// `ringing → answered`; attempt `reached`.
    Answered { call_ref: Option<String> },
    /// `→ failed{reason}`; `attempt` is `Some(no_answer)` only when ringing
    /// had started and the reason is one of the D-031 set.
    Failed {
        reason: FailureReason,
        attempt: Option<ContactOutcome>,
    },
    /// `answered → ended{reason}`; never an attempt (it was written at
    /// answer time).
    Ended { reason: EndReason },
}

impl Transition {
    pub fn is_noop(&self) -> bool {
        matches!(self, Transition::NoOp)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Transition::Failed { .. } | Transition::Ended { .. })
    }

    /// The D-031 attempt this transition writes, if any.
    pub fn attempt(&self) -> Option<ContactOutcome> {
        match self {
            Transition::Answered { .. } => Some(ContactOutcome::Reached),
            Transition::Failed { attempt, .. } => *attempt,
            Transition::NoOp | Transition::Ringing | Transition::Ended { .. } => None,
        }
    }

    pub fn status(&self) -> Option<CallStatus> {
        match self {
            Transition::NoOp => None,
            Transition::Ringing => Some(CallStatus::Ringing),
            Transition::Answered { .. } => Some(CallStatus::Answered),
            Transition::Failed { .. } => Some(CallStatus::Failed),
            Transition::Ended { .. } => Some(CallStatus::Ended),
        }
    }
}

/// Whether `ringing → failed{reason}` carries a `no_answer` attempt
/// (docs/specs/SLICE_006.md §2 D-031 table).
fn ringing_failure_attempt(reason: FailureReason) -> Option<ContactOutcome> {
    match reason {
        FailureReason::NoAnswer
        | FailureReason::Busy
        | FailureReason::Declined
        | FailureReason::RingTimeout
        | FailureReason::Cancelled => Some(ContactOutcome::NoAnswer),
        FailureReason::AgentNotJoined | FailureReason::ProviderError | FailureReason::Expired => {
            None
        }
    }
}

/// The §2 table, exhaustively. Anything the table does not name is a
/// `NoOp` (a signal for a state the dial task owns, or a terminal call).
pub fn apply(status: CallStatus, signal: &Signal) -> Transition {
    use CallStatus::*;
    match (status, signal) {
        // --- placing ----------------------------------------------------
        (Placing, Signal::Dialing) => Transition::Ringing,
        (Placing, Signal::Cancelled | Signal::AgentLeft | Signal::AgentHangup) => {
            Transition::Failed {
                reason: FailureReason::Cancelled,
                attempt: None,
            }
        }
        (Placing, Signal::AgentNotJoined) => Transition::Failed {
            reason: FailureReason::AgentNotJoined,
            attempt: None,
        },
        (Placing, Signal::ProviderError) => Transition::Failed {
            reason: FailureReason::ProviderError,
            attempt: None,
        },
        (Placing, Signal::Expired) => Transition::Failed {
            reason: FailureReason::Expired,
            attempt: None,
        },
        (
            Placing,
            Signal::Answered { .. }
            | Signal::DialFailed(_)
            | Signal::RemoteLeft
            | Signal::RoomFinished
            | Signal::MaxDuration
            | Signal::Reconciled,
        ) => Transition::NoOp,

        // --- ringing ----------------------------------------------------
        (Ringing, Signal::Answered { call_ref }) => Transition::Answered {
            call_ref: call_ref.clone(),
        },
        (Ringing, Signal::DialFailed(failure)) => {
            let reason = failure.reason();
            Transition::Failed {
                reason,
                attempt: ringing_failure_attempt(reason),
            }
        }
        (Ringing, Signal::Cancelled | Signal::AgentLeft | Signal::AgentHangup) => {
            Transition::Failed {
                reason: FailureReason::Cancelled,
                attempt: ringing_failure_attempt(FailureReason::Cancelled),
            }
        }
        (Ringing, Signal::ProviderError) => Transition::Failed {
            reason: FailureReason::ProviderError,
            attempt: None,
        },
        (Ringing, Signal::Expired) => Transition::Failed {
            reason: FailureReason::Expired,
            attempt: None,
        },
        (
            Ringing,
            Signal::Dialing
            | Signal::AgentNotJoined
            | Signal::RemoteLeft
            | Signal::RoomFinished
            | Signal::MaxDuration
            | Signal::Reconciled,
        ) => Transition::NoOp,

        // --- answered ---------------------------------------------------
        (Answered, Signal::AgentHangup) => Transition::Ended {
            reason: EndReason::AgentHangup,
        },
        (Answered, Signal::AgentLeft) => Transition::Ended {
            reason: EndReason::AgentDisconnected,
        },
        (Answered, Signal::RemoteLeft | Signal::RoomFinished) => Transition::Ended {
            reason: EndReason::RemoteHangup,
        },
        (Answered, Signal::MaxDuration) => Transition::Ended {
            reason: EndReason::MaxDuration,
        },
        (Answered, Signal::Reconciled) => Transition::Ended {
            reason: EndReason::Reconciled,
        },
        (
            Answered,
            Signal::Dialing
            | Signal::Answered { .. }
            | Signal::DialFailed(_)
            | Signal::Cancelled
            | Signal::AgentNotJoined
            | Signal::ProviderError
            | Signal::Expired,
        ) => Transition::NoOp,

        // --- terminal ---------------------------------------------------
        (Ended | Failed, _) => Transition::NoOp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_signals() -> Vec<Signal> {
        vec![
            Signal::Dialing,
            Signal::Answered {
                call_ref: Some("ref".into()),
            },
            Signal::DialFailed(DialFailure::Busy),
            Signal::DialFailed(DialFailure::Declined),
            Signal::DialFailed(DialFailure::NoAnswer),
            Signal::DialFailed(DialFailure::RingTimeout),
            Signal::Cancelled,
            Signal::AgentHangup,
            Signal::AgentLeft,
            Signal::AgentNotJoined,
            Signal::ProviderError,
            Signal::RemoteLeft,
            Signal::RoomFinished,
            Signal::MaxDuration,
            Signal::Expired,
            Signal::Reconciled,
        ]
    }

    fn failed(reason: FailureReason, attempt: Option<ContactOutcome>) -> Transition {
        Transition::Failed { reason, attempt }
    }

    #[test]
    fn placing_row_of_the_table() {
        assert_eq!(
            apply(CallStatus::Placing, &Signal::Dialing),
            Transition::Ringing
        );
        for s in [Signal::Cancelled, Signal::AgentLeft, Signal::AgentHangup] {
            assert_eq!(
                apply(CallStatus::Placing, &s),
                failed(FailureReason::Cancelled, None),
                "{s:?}"
            );
        }
        assert_eq!(
            apply(CallStatus::Placing, &Signal::AgentNotJoined),
            failed(FailureReason::AgentNotJoined, None)
        );
        assert_eq!(
            apply(CallStatus::Placing, &Signal::ProviderError),
            failed(FailureReason::ProviderError, None)
        );
        assert_eq!(
            apply(CallStatus::Placing, &Signal::Expired),
            failed(FailureReason::Expired, None)
        );
        for s in [
            Signal::RemoteLeft,
            Signal::RoomFinished,
            Signal::Answered { call_ref: None },
            Signal::DialFailed(DialFailure::Busy),
            Signal::MaxDuration,
            Signal::Reconciled,
        ] {
            assert_eq!(apply(CallStatus::Placing, &s), Transition::NoOp, "{s:?}");
        }
    }

    #[test]
    fn ringing_row_of_the_table() {
        assert_eq!(
            apply(
                CallStatus::Ringing,
                &Signal::Answered {
                    call_ref: Some("SCL_1".into())
                }
            ),
            Transition::Answered {
                call_ref: Some("SCL_1".into())
            }
        );
        for (f, reason) in [
            (DialFailure::Busy, FailureReason::Busy),
            (DialFailure::Declined, FailureReason::Declined),
            (DialFailure::NoAnswer, FailureReason::NoAnswer),
            (DialFailure::RingTimeout, FailureReason::RingTimeout),
        ] {
            assert_eq!(
                apply(CallStatus::Ringing, &Signal::DialFailed(f)),
                failed(reason, Some(ContactOutcome::NoAnswer))
            );
        }
        for s in [Signal::Cancelled, Signal::AgentLeft, Signal::AgentHangup] {
            assert_eq!(
                apply(CallStatus::Ringing, &s),
                failed(FailureReason::Cancelled, Some(ContactOutcome::NoAnswer)),
                "{s:?}"
            );
        }
        assert_eq!(
            apply(CallStatus::Ringing, &Signal::ProviderError),
            failed(FailureReason::ProviderError, None)
        );
        assert_eq!(
            apply(CallStatus::Ringing, &Signal::Expired),
            failed(FailureReason::Expired, None)
        );
        for s in [
            Signal::RemoteLeft,
            Signal::RoomFinished,
            Signal::Dialing,
            Signal::AgentNotJoined,
            Signal::MaxDuration,
            Signal::Reconciled,
        ] {
            assert_eq!(apply(CallStatus::Ringing, &s), Transition::NoOp, "{s:?}");
        }
    }

    #[test]
    fn answered_row_of_the_table() {
        let ended = |reason| Transition::Ended { reason };
        assert_eq!(
            apply(CallStatus::Answered, &Signal::AgentHangup),
            ended(EndReason::AgentHangup)
        );
        assert_eq!(
            apply(CallStatus::Answered, &Signal::AgentLeft),
            ended(EndReason::AgentDisconnected)
        );
        assert_eq!(
            apply(CallStatus::Answered, &Signal::RemoteLeft),
            ended(EndReason::RemoteHangup)
        );
        assert_eq!(
            apply(CallStatus::Answered, &Signal::RoomFinished),
            ended(EndReason::RemoteHangup)
        );
        assert_eq!(
            apply(CallStatus::Answered, &Signal::MaxDuration),
            ended(EndReason::MaxDuration)
        );
        assert_eq!(
            apply(CallStatus::Answered, &Signal::Reconciled),
            ended(EndReason::Reconciled)
        );
        for s in [
            Signal::Dialing,
            Signal::Answered { call_ref: None },
            Signal::DialFailed(DialFailure::NoAnswer),
            Signal::Cancelled,
            Signal::AgentNotJoined,
            Signal::ProviderError,
            Signal::Expired,
        ] {
            assert_eq!(apply(CallStatus::Answered, &s), Transition::NoOp, "{s:?}");
        }
    }

    #[test]
    fn terminal_states_absorb_every_signal() {
        for status in [CallStatus::Ended, CallStatus::Failed] {
            for s in all_signals() {
                assert_eq!(apply(status, &s), Transition::NoOp, "{status:?} {s:?}");
            }
        }
    }

    #[test]
    fn every_state_times_signal_is_covered_and_consistent() {
        // Exhaustiveness is enforced by the compiler; this pins the
        // invariants: an attempt only on answered or a ringing failure;
        // `Ended` only from `answered`; nothing leaves a terminal state.
        for status in [
            CallStatus::Placing,
            CallStatus::Ringing,
            CallStatus::Answered,
            CallStatus::Ended,
            CallStatus::Failed,
        ] {
            for s in all_signals() {
                let t = apply(status, &s);
                if status.is_terminal() {
                    assert!(t.is_noop());
                }
                if matches!(t, Transition::Ended { .. }) {
                    assert_eq!(status, CallStatus::Answered);
                }
                if t.attempt() == Some(ContactOutcome::Reached) {
                    assert!(matches!(t, Transition::Answered { .. }));
                }
                if t.attempt() == Some(ContactOutcome::NoAnswer) {
                    assert_eq!(status, CallStatus::Ringing);
                    assert!(matches!(t, Transition::Failed { .. }));
                }
                if let Some(next) = t.status() {
                    assert_ne!(next, status);
                }
            }
        }
    }

    #[test]
    fn d031_attempt_mapping() {
        assert_eq!(
            ringing_failure_attempt(FailureReason::NoAnswer),
            Some(ContactOutcome::NoAnswer)
        );
        assert_eq!(
            ringing_failure_attempt(FailureReason::Busy),
            Some(ContactOutcome::NoAnswer)
        );
        assert_eq!(
            ringing_failure_attempt(FailureReason::Declined),
            Some(ContactOutcome::NoAnswer)
        );
        assert_eq!(
            ringing_failure_attempt(FailureReason::RingTimeout),
            Some(ContactOutcome::NoAnswer)
        );
        assert_eq!(
            ringing_failure_attempt(FailureReason::Cancelled),
            Some(ContactOutcome::NoAnswer)
        );
        assert_eq!(ringing_failure_attempt(FailureReason::AgentNotJoined), None);
        assert_eq!(ringing_failure_attempt(FailureReason::ProviderError), None);
        assert_eq!(ringing_failure_attempt(FailureReason::Expired), None);
        assert_eq!(
            Transition::Answered { call_ref: None }.attempt(),
            Some(ContactOutcome::Reached)
        );
        assert_eq!(
            Transition::Ended {
                reason: EndReason::RemoteHangup
            }
            .attempt(),
            None
        );
    }

    #[test]
    fn sip_failure_to_signal_mapping() {
        assert_eq!(
            Signal::from_sip_failure(SipFailure::Busy),
            Signal::DialFailed(DialFailure::Busy)
        );
        assert_eq!(
            Signal::from_sip_failure(SipFailure::Declined),
            Signal::DialFailed(DialFailure::Declined)
        );
        assert_eq!(
            Signal::from_sip_failure(SipFailure::NoAnswer),
            Signal::DialFailed(DialFailure::NoAnswer)
        );
        assert_eq!(
            Signal::from_sip_failure(SipFailure::RingTimeout),
            Signal::DialFailed(DialFailure::RingTimeout)
        );
        assert_eq!(
            Signal::from_sip_failure(SipFailure::Other(503)),
            Signal::ProviderError
        );
        assert_eq!(
            Signal::from_sip_failure(SipFailure::from_sip_status(486)),
            Signal::DialFailed(DialFailure::Busy)
        );
    }
}
