//! Centrifugo realtime integration (docs/specs/SLICE_003.md §6, §9; D-023):
//! connection-token minting, the event envelope, and the best-effort
//! publisher. Centrifugo is delivery, not truth (D-011) — PostgreSQL
//! remains authoritative and a reconnecting client recovers by refetch.

pub mod events;
pub mod publisher;
pub mod token;

pub use events::{channel_for, PersonChange, Publication, RealtimeEvent};
pub use publisher::{CentrifugoTransport, PublishOutcome, Publisher};
