//! Correspondence capture v1 (docs/specs/SLICE_009.md; D-042; O-014 #2):
//! per-agent CC/BCC capture addresses, metadata-only timeline entries,
//! auto-contact-attempt on outbound, the `client_replied` Today arm
//! (`domain/today/`), and the per-agent unmatched held queue.
//!
//! Layout: `token` (the `CaptureToken`/parse/render/mint grammar —
//! mirrors `domain/intake/address.rs`), `address` (persistence: mint-if-
//! absent, the receive-path lookup, self-service rotation — mirrors
//! `domain/intake/rotate.rs`), `queries` (direction-ladder gathering
//! reads), `ladder` (the pure direction/attribution decision, spec §5),
//! `store` (`correspondence_raw`/`capture_message` persistence), `pipeline`
//! (metadata derivation + fact insertion, shared by the live path and the
//! link command), `receive` (the Phase A/B entry point, wired into
//! `domain/intake/receive.rs`'s dispatch), `commands` (the unmatched
//! link/dismiss mutations).

pub mod address;
pub mod commands;
pub mod ladder;
pub mod pipeline;
pub mod queries;
pub mod receive;
pub mod store;
pub mod token;

pub use ladder::Direction;
pub use receive::{receive_captured_email, CaptureEmailOutcome};
pub use token::CaptureToken;
