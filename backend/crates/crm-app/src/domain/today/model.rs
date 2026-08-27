//! Today read-model types (docs/specs/SLICE_003.md §3, §4; D-010).

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::commands::ContactAttemptRef;
use crate::domain::person::model::PersonSummary;
use crate::ids::InquiryId;

/// The strict freshness window (§3): `latest_inquiry.received_at > now -
/// 24h`. Computed once, in SQL, and never re-evaluated by `rank()`.
pub const FRESH_INQUIRY_WINDOW_HOURS: i64 = 24;

/// The reason codes, in the fixed order `rank()` always emits them:
/// `new_inquiry` (if fresh), `no_contact_attempt` (when the Person
/// qualifies by Inquiry), `repeat_inquiry` (if the Person's total Inquiry
/// count >= 2), then `call_outcome_needed` (docs/specs/SLICE_006c.md §5a,
/// D-033: the viewer's most recent ended/failed call to this Person whose
/// effective attempt is still the automatic root).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    CallOutcomeNeeded {
        call_id: Uuid,
        ended_at: DateTime<Utc>,
    },
}

/// Tiers in list order: `high`, `normal`, then `low` (D-033's "outcome
/// needed" tier, always under every Inquiry-based item).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TodayPriority {
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendedAction {
    Call,
    Email,
    SetOutcome,
}

/// `latest_inquiry` on a `TodayItem` — exactly `{id, source, received_at}`
/// (docs/specs/SLICE_003.md §5).
#[derive(Debug, Clone, Serialize)]
pub struct InquiryRef {
    pub id: InquiryId,
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
    /// The Inquiry-based `waiting_since` when `by_inquiry`; otherwise the
    /// outcome-needed call's `ended_at` (docs/specs/SLICE_006c.md §5a).
    pub waiting_since: DateTime<Utc>,
    pub inquiry_count: i64,
    /// `false` whenever `by_inquiry` is `false` (computed in SQL).
    pub fresh: bool,
    /// Qualifies by the SLICE_003 §3 rule (assigned to the viewer with an
    /// unanswered Inquiry). When `false`, `outcome_needed` is `Some` and the
    /// item is `low`.
    pub by_inquiry: bool,
    /// The viewer's most recent ended/failed call to this Person that
    /// still has no chosen outcome (D-033), if any.
    pub outcome_needed: Option<OutcomeNeededCall>,
}

/// A call whose effective attempt is still the automatic root
/// (docs/specs/SLICE_006c.md §5a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutcomeNeededCall {
    pub call_id: Uuid,
    pub ended_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn priority_serialises_snake_case_and_low_sorts_last() {
        assert_eq!(serde_json::to_value(TodayPriority::High).unwrap(), "high");
        assert_eq!(
            serde_json::to_value(TodayPriority::Normal).unwrap(),
            "normal"
        );
        assert_eq!(serde_json::to_value(TodayPriority::Low).unwrap(), "low");
        assert!(TodayPriority::High < TodayPriority::Normal);
        assert!(TodayPriority::Normal < TodayPriority::Low);
    }

    #[test]
    fn recommended_action_serialises_set_outcome() {
        assert_eq!(
            serde_json::to_value(RecommendedAction::SetOutcome).unwrap(),
            "set_outcome"
        );
        assert_eq!(
            serde_json::to_value(RecommendedAction::Call).unwrap(),
            "call"
        );
    }

    #[test]
    fn call_outcome_needed_reason_is_exactly_code_call_id_ended_at() {
        let call_id = Uuid::new_v4();
        let ended_at = Utc.with_ymd_and_hms(2026, 8, 23, 14, 30, 0).unwrap();
        let value =
            serde_json::to_value(TodayReason::CallOutcomeNeeded { call_id, ended_at }).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "code": "call_outcome_needed",
                "call_id": call_id,
                "ended_at": "2026-08-23T14:30:00Z",
            })
        );
    }
}
