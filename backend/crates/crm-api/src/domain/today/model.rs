//! Today read-model types (docs/specs/SLICE_003.md §3, §4; D-010).

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::commands::ContactAttemptRef;
use crate::domain::person::model::PersonSummary;

/// The strict freshness window (§3): `latest_inquiry.received_at > now -
/// 24h`. Computed once, in SQL, and never re-evaluated by `rank()`.
pub const FRESH_INQUIRY_WINDOW_HOURS: i64 = 24;

/// The §3 reason codes, in the fixed order `rank()` always emits them:
/// `new_inquiry` (if fresh), `no_contact_attempt` (always), `repeat_inquiry`
/// (if the Person's total Inquiry count >= 2).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum TodayReason {
    NewInquiry {
        source: String,
        received_at: DateTime<Utc>,
    },
    NoContactAttempt {
        since: DateTime<Utc>,
    },
    RepeatInquiry {
        inquiry_count: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TodayPriority {
    High,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendedAction {
    Call,
    Email,
}

/// `latest_inquiry` on a `TodayItem` — exactly `{id, source, received_at}`
/// (docs/specs/SLICE_003.md §5).
#[derive(Debug, Clone, Serialize)]
pub struct InquiryRef {
    pub id: Uuid,
    pub source: String,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodayItem {
    pub person: PersonSummary,
    pub priority: TodayPriority,
    pub recommended_action: RecommendedAction,
    pub reasons: Vec<TodayReason>,
    pub waiting_since: DateTime<Utc>,
    pub latest_inquiry: InquiryRef,
    pub last_contact_attempt: Option<ContactAttemptRef>,
}

/// `GET /api/today`'s exact response shape (docs/specs/SLICE_003.md §5).
#[derive(Debug, Clone, Serialize)]
pub struct TodayList {
    pub generated_at: DateTime<Utc>,
    pub items: Vec<TodayItem>,
    pub truncated: bool,
}

/// One raw candidate row (docs/specs/SLICE_003.md §4): everything `rank()`
/// needs to compute reasons, priority, and recommended action, with
/// `fresh` computed once in SQL (§3) and never re-evaluated.
#[derive(Debug, Clone)]
pub struct TodayCandidate {
    pub person: PersonSummary,
    pub latest_inquiry: InquiryRef,
    pub last_contact_attempt: Option<ContactAttemptRef>,
    pub waiting_since: DateTime<Utc>,
    pub inquiry_count: i64,
    pub fresh: bool,
}
