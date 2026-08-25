//! Email lead intake (docs/plans/SLICE_007_LADDER.md). Rung 007a: the
//! Organization intake address value type only — nothing receives mail.
//! Rung 007b: Phase-A-only inbound email endpoint (receive module).

pub mod address;
pub mod receive;

pub use address::IntakeAddress;
pub use receive::{receive_inbound_email, InboundEmailOutcome, ReceiveInboundEmailError};
