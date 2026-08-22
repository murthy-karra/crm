//! `OperatorContext` (docs/specs/SLICE_005.md §2, §7): built by the HTTP
//! handler from `AuthContext` and nothing else — never from the request
//! body, the history, the model, or the screen context. Every tool call
//! receives it by reference; the model cannot influence it.

use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct OperatorContext {
    pub actor_user_id: Uuid,
    pub organization_id: Uuid,
    /// Member-entered, ≤ 120 chars; enters the prompt as trusted text
    /// (docs/specs/SLICE_005.md §14 item 13).
    pub actor_display_name: String,
    /// Also the `correlation_id` on spans and ledger rows.
    pub turn_id: Uuid,
    pub now: DateTime<Utc>,
}
