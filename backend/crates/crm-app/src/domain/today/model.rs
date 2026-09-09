//! Today read-model types (docs/specs/SLICE_003.md §3, §4; D-010).

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::domain::commands::ContactAttemptRef;
use crate::domain::person::model::PersonSummary;
use crate::domain::task::TaskKind;
use crate::ids::{InquiryId, SavedListId, TaskId};

/// The strict freshness window (§3): `latest_inquiry.received_at > now -
/// 24h`. Computed once, in SQL, and never re-evaluated by `rank()`.
pub const FRESH_INQUIRY_WINDOW_HOURS: i64 = 24;

/// The reason codes, in the fixed order `rank()` always emits them:
/// `new_inquiry` (if fresh), `no_contact_attempt` (when the Person
/// qualifies by Inquiry), `repeat_inquiry` (if the Person's total Inquiry
/// count >= 2), then `call_outcome_needed` (docs/specs/SLICE_006c.md §5a,
/// D-033: the viewer's most recent ended/failed call to this Person whose
/// effective attempt is still the automatic root). `client_replied`
/// (Slice 009, docs/specs/SLICE_009.md §6; declared additive SLICE_003 §5
/// change) WINS the reason slot in place of the Inquiry-based trio above
/// when the Person also qualifies for it — see `rank::rank_one` — but
/// never replaces `call_outcome_needed`, which is always appended last
/// regardless of which reason(s) precede it.
#[derive(Clone, PartialEq, Eq, Serialize)]
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
    ClientReplied {
        occurred_at: DateTime<Utc>,
    },
    ListMember {
        list_id: SavedListId,
        name: String,
    },
    /// The built-in task axis (Slice 016b, docs/specs/SLICE_016.md §5,
    /// D-054 §1): `due_at < now` at evaluation time. Appended after any
    /// list reasons and before `call_outcome_needed`, in `today::mod` —
    /// never emitted by `rank()`.
    TaskOverdue {
        task_id: TaskId,
        title: String,
        kind: TaskKind,
        due_at: DateTime<Utc>,
    },
    /// The built-in task axis: `due_at >= now`, within the 24h window
    /// (docs/specs/SLICE_016.md §5).
    TaskDue {
        task_id: TaskId,
        title: String,
        kind: TaskKind,
        due_at: DateTime<Utc>,
    },
}

/// A hand-written, redacting `Debug` impl (never `#[derive(Debug)]`, the
/// `crm_app::domain::task::Task` pattern): `TaskOverdue`/`TaskDue` carry a
/// task title (docs/specs/SLICE_016.md §1 rule 7, §9), and `TodayList` is
/// `Debug`-printed in tests and error paths, so a stray `?list`/`{:?}`
/// must never print the title itself, only its length.
impl std::fmt::Debug for TodayReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodayReason::NewInquiry {
                source,
                received_at,
            } => f
                .debug_struct("NewInquiry")
                .field("source", source)
                .field("received_at", received_at)
                .finish(),
            TodayReason::NoContactAttempt { since } => f
                .debug_struct("NoContactAttempt")
                .field("since", since)
                .finish(),
            TodayReason::RepeatInquiry { inquiry_count } => f
                .debug_struct("RepeatInquiry")
                .field("inquiry_count", inquiry_count)
                .finish(),
            TodayReason::CallOutcomeNeeded { call_id, ended_at } => f
                .debug_struct("CallOutcomeNeeded")
                .field("call_id", call_id)
                .field("ended_at", ended_at)
                .finish(),
            TodayReason::ClientReplied { occurred_at } => f
                .debug_struct("ClientReplied")
                .field("occurred_at", occurred_at)
                .finish(),
            TodayReason::ListMember { list_id, name } => f
                .debug_struct("ListMember")
                .field("list_id", list_id)
                .field("name_chars", &name.chars().count())
                .finish(),
            TodayReason::TaskOverdue {
                task_id,
                title,
                kind,
                due_at,
            } => f
                .debug_struct("TaskOverdue")
                .field("task_id", task_id)
                .field("title_chars", &title.chars().count())
                .field("kind", kind)
                .field("due_at", due_at)
                .finish(),
            TodayReason::TaskDue {
                task_id,
                title,
                kind,
                due_at,
            } => f
                .debug_struct("TaskDue")
                .field("task_id", task_id)
                .field("title_chars", &title.chars().count())
                .field("kind", kind)
                .field("due_at", due_at)
                .finish(),
        }
    }
}

/// Tiers in list order: `high`, `normal`, then `low` (D-033's "outcome
/// needed" tier, always under every Inquiry-based item).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TodayPriority {
    High,
    Normal,
    List,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendedAction {
    Call,
    Email,
    ReviewPerson,
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
    pub waiting_since: Option<DateTime<Utc>>,
    pub latest_inquiry: Option<InquiryRef>,
    pub last_contact_attempt: Option<ContactAttemptRef>,
}

/// `GET /api/today`'s exact response shape (docs/specs/SLICE_003.md §5).
#[derive(Debug, Clone, Serialize)]
pub struct TodayList {
    pub generated_at: DateTime<Utc>,
    pub items: Vec<TodayItem>,
    pub truncated: bool,
    pub sources: TodaySources,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodaySources {
    pub status: TodaySourcesStatus,
    pub issues: Vec<TodaySourceIssue>,
    /// docs/specs/SLICE_011d.md §5: additive. One entry per system feed
    /// whose stored definition fell back to canonical
    /// (`error: invalid_definition, fallback: true`) or whose evaluation
    /// failed (`error: unavailable, fallback: false` — only the call feed
    /// can fail this way; a person-state failure is a 503, never reported
    /// here). A disabled feed contributes no entry.
    pub system_feed_issues: Vec<SystemFeedIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemFeedIssue {
    pub feed_key: &'static str,
    pub error: SystemFeedIssueError,
    pub fallback: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemFeedIssueError {
    Unavailable,
    InvalidDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TodaySourcesStatus {
    Complete,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodaySourceIssue {
    pub list_id: SavedListId,
    pub name: String,
    pub revision: i64,
    pub error: TodaySourceIssueError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TodaySourceIssueError {
    UnsupportedFilter,
    InvalidStage,
    InvalidAssignee,
    /// docs/specs/SLICE_011e.md §4b: a `tags`/`not_tags` value vanished
    /// during Today's per-source evaluation.
    InvalidTag,
    Unavailable,
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
    /// Slice 009 (docs/specs/SLICE_009.md §6): the qualifying inbound
    /// correspondence's `occurred_at` when the Person is assigned to the
    /// viewer AND that inbound is later than every effective
    /// contact_attempted and every outbound correspondence for them.
    /// `Some` here WINS the reason slot over `by_inquiry`'s trio (`rank`
    /// reads this before falling back to Inquiry-based reasons) — the
    /// reply is the newer, more actionable signal. `fresh`/`waiting_since`
    /// are already precedence-resolved in SQL (reply > inquiry > call) by
    /// the time they reach here, exactly like `by_inquiry`'s existing
    /// fields — `rank` never recomputes them.
    pub client_replied: Option<DateTime<Utc>>,
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
        assert_eq!(serde_json::to_value(TodayPriority::List).unwrap(), "list");
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
        assert_eq!(
            serde_json::to_value(RecommendedAction::ReviewPerson).unwrap(),
            "review_person"
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

    #[test]
    fn client_replied_reason_is_exactly_code_occurred_at() {
        let occurred_at = Utc.with_ymd_and_hms(2026, 8, 27, 9, 0, 0).unwrap();
        let value = serde_json::to_value(TodayReason::ClientReplied { occurred_at }).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "code": "client_replied",
                "occurred_at": "2026-08-27T09:00:00Z",
            })
        );
    }
}
