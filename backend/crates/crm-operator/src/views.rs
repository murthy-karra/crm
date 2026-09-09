//! Tool output view types (docs/specs/SLICE_005.md §3) — narrower than the
//! HTTP read models, and the one place outside-originated free text is
//! wrapped as [`UntrustedText`] before it can reach a prompt.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Maximum characters of any single untrusted value that reaches the
/// prompt (docs/specs/SLICE_005.md §14 item 4).
pub const UNTRUSTED_CLIP_CHARS: usize = 500;

/// The ordering rule reported by `explain_priority`, verbatim from
/// docs/specs/SLICE_003.md §3 / SLICE_005 §3, extended by SLICE_006c §5a
/// (D-033): the `low` "outcome needed" tier sorts under both Inquiry
/// tiers, by the call's `ended_at`.
pub const ORDERING_RULE: &str =
    "built_in_work_is_admitted_before_list_matches_at_the_200_item_cap; display_high_then_normal_then_list_then_low; list_matches_sort_by_last_contact_attempt_ascending_with_never_contacted_first_then_person_id; built_in_high_and_normal_sort_by_waiting_since_then_id; low_sorts_by_ended_at_then_id; overdue_task_raises_normal_to_high_after_fresh; task_only_items_follow_their_tier_by_due_at_then_id";

/// Zero-width and bidirectional formatting characters: invisible in a
/// rendered reply but able to reorder or hide text in a prompt.
pub fn is_invisible_format(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
    )
}

/// Free text that originated outside the application (inquiry messages,
/// Person names, contact values). Clipped to 500 chars with control
/// characters stripped at construction; serialized as
/// `{"untrusted_text": "..."}` so the system prompt can name the key and
/// the model is told to quote or summarize it, never obey it (§7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrustedText(String);

impl UntrustedText {
    pub fn new(raw: &str) -> Self {
        let mut out = String::with_capacity(raw.len().min(UNTRUSTED_CLIP_CHARS));
        let mut count = 0usize;
        for ch in raw.chars() {
            if count >= UNTRUSTED_CLIP_CHARS {
                break;
            }
            let ch = match ch {
                '\n' | '\r' | '\t' => ' ',
                c if c.is_control() || is_invisible_format(c) => continue,
                c => c,
            };
            out.push(ch);
            count += 1;
        }
        Self(out)
    }

    /// The clipped, stripped text — for the wire (`WirePersonCard`), where
    /// the wrapper is not needed (§5).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl Serialize for UntrustedText {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("UntrustedText", 1)?;
        s.serialize_field("untrusted_text", &self.0)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for UntrustedText {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wrapper {
            untrusted_text: String,
        }
        let w = Wrapper::deserialize(deserializer)?;
        Ok(UntrustedText::new(&w.untrusted_text))
    }
}

/// `start_call`'s outcome (docs/specs/SLICE_006b.md §3): a proposal the
/// user must confirm in the UI, a number-choice question, or "no phone".
/// Only `Proposed` writes a row; the model can never execute anything.
#[derive(Debug, Clone)]
pub enum StartCallProposalOutcome {
    Proposed(Box<ProposalView>),
    NeedsNumberChoice { phones: Vec<PhoneOption> },
    NoPhone,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProposalView {
    pub proposal_id: Uuid,
    pub person: PersonCard,
    pub phone: UntrustedText,
    pub contact_method_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhoneOption {
    pub contact_method_id: Uuid,
    // No label until one exists in the schema (docs/specs/SLICE_006b.md §3).
    pub value: UntrustedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonCard {
    pub id: Uuid,
    pub display_name: UntrustedText,
    pub stage_name: String,
    pub assigned_user_display_name: Option<String>,
    pub primary_email: Option<UntrustedText>,
    pub primary_phone: Option<UntrustedText>,
    pub inquiry_count: i64,
    pub last_inquiry_at: Option<DateTime<Utc>>,
}

/// `PersonCard` on the wire (docs/specs/SLICE_005.md §5): plain strings
/// where the prompt form has `UntrustedText`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePersonCard {
    pub id: Uuid,
    pub display_name: String,
    pub stage_name: String,
    pub assigned_user_display_name: Option<String>,
    pub primary_email: Option<String>,
    pub primary_phone: Option<String>,
    pub inquiry_count: i64,
    pub last_inquiry_at: Option<DateTime<Utc>>,
}

impl PersonCard {
    pub fn to_wire(&self) -> WirePersonCard {
        WirePersonCard {
            id: self.id,
            display_name: self.display_name.as_str().to_string(),
            stage_name: self.stage_name.clone(),
            assigned_user_display_name: self.assigned_user_display_name.clone(),
            primary_email: self.primary_email.as_ref().map(|t| t.as_str().to_string()),
            primary_phone: self.primary_phone.as_ref().map(|t| t.as_str().to_string()),
            inquiry_count: self.inquiry_count,
            last_inquiry_at: self.last_inquiry_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub matches: Vec<PersonCard>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactMethodView {
    pub kind: String,
    pub value: UntrustedText,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InquiryView {
    pub id: Uuid,
    /// Constrained to `[a-z0-9_]{1,64}` by intake parsing; stays bare.
    pub source: String,
    pub received_at: DateTime<Utc>,
    pub message: Option<UntrustedText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntryView {
    pub kind: String,
    pub occurred_at: DateTime<Utc>,
    pub actor_display_name: Option<String>,
    /// Rendered from reference-table values (stage names, member display
    /// names), not outside text — so not wrapped (§3).
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonDetail {
    pub person: PersonCard,
    pub contact_methods: Vec<ContactMethodView>,
    /// Latest 5.
    pub inquiries: Vec<InquiryView>,
    /// Latest 20, excluding `note` entries (Slice 015, docs/specs/
    /// SLICE_015.md §5): `notes` below already represents them, and the
    /// merged-sort truncation would otherwise let a burst of notes push
    /// stage, assignment, and call facts out of the model's view.
    pub history: Vec<HistoryEntryView>,
    pub on_your_today: bool,
    /// True only when the Person is in the bounded, returned Today queue.
    /// It does not make an uncapped membership claim.
    pub today_truncated: bool,
    pub sources: TodaySourcesView,
    /// Tag names are user-authored text (Slice 011e, docs/specs/
    /// SLICE_011e.md §5), so — like list names (011c) — they are never
    /// serialized as trusted strings in a model-facing tool result.
    pub tags: Vec<UntrustedText>,
    /// Latest 5 live notes, in creation order (Slice 015, docs/specs/
    /// SLICE_015.md §5, the `MAX_INQUIRIES` precedent). Note bodies are
    /// user-authored text about a client sent to the model provider —
    /// the same exposure class `inquiries[].message` already has
    /// (D-053 §4).
    pub notes: Vec<NoteView>,
    /// Open tasks, in `open_for_person` order (`due_at ASC NULLS LAST,
    /// created_at, id`), at most ten (Slice 016a, docs/specs/SLICE_016.md
    /// §7). Task titles are user-authored text about a client sent to the
    /// model provider — the same exposure class notes and inquiry
    /// messages already have (D-053 §4, D-054 §3). No `can_manage`: the
    /// Operator has no write tool for tasks in 016a (§7).
    pub tasks: Vec<TaskView>,
}

/// One note in the Operator's `PersonDetail.notes` (docs/specs/
/// SLICE_015.md §5): `author_display_name` is `None` for an imported note
/// whose FUB author matched no member (§1 rule 1); `body` is subject to
/// `UntrustedText`'s 500-character clip and whitespace flattening, the
/// `inquiries[].message` precedent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteView {
    pub author_display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub body: UntrustedText,
}

/// One open task in the Operator's `PersonDetail.tasks` (docs/specs/
/// SLICE_016.md §7): `title` is subject to `UntrustedText`'s 500-character
/// clip and whitespace flattening, the `inquiries[].message`/`NoteView`
/// precedent (a task title cannot exceed 500 characters at the source, so
/// the clip never actually engages — kept for the same defense-in-depth
/// reason every other outside-text view carries it). `assignee_display_name`
/// is `None` for an imported task whose FUB assignee matched no member
/// (§1 rule 1).
/// `Debug` is a hand-written, redacting impl (never `#[derive(Debug)]`,
/// the `crm_app::domain::task::Task` pattern): `PersonDetail` is itself
/// `Debug`-derived and `UntrustedText`'s own derived `Debug` prints its
/// raw inner text, so a stray `?person_detail`/`{:?}` in a log, panic, or
/// test-failure message must never be able to print a task title through
/// this type — only its length.
#[derive(Clone, Serialize, Deserialize)]
pub struct TaskView {
    pub title: UntrustedText,
    pub kind: String,
    pub due_at: Option<DateTime<Utc>>,
    pub assignee_display_name: Option<String>,
}

impl std::fmt::Debug for TaskView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskView")
            .field("title_chars", &self.title.as_str().chars().count())
            .field("kind", &self.kind)
            .field("due_at", &self.due_at)
            .field("assignee_display_name", &self.assignee_display_name)
            .finish()
    }
}

/// Source evaluation state shared by every Today-derived Operator output.
/// List names are user-authored, so they are never serialized as trusted
/// strings in model-facing tool results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodaySourcesView {
    pub status: String,
    pub issues: Vec<TodaySourceIssueView>,
    /// docs/specs/SLICE_011d.md §5, §6: additive. `feed_key` is a static
    /// token from the fixed vocabulary, never user-authored — unlike
    /// `TodaySourceIssueView::name`, it is a plain trusted `String`.
    pub system_feed_issues: Vec<SystemFeedIssueView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodaySourceIssueView {
    pub list_id: Uuid,
    pub name: UntrustedText,
    pub revision: i64,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemFeedIssueView {
    pub feed_key: String,
    pub error: String,
    pub fallback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayItemView {
    /// 1-based, equal to the index in `GET /api/today` plus one.
    pub position: usize,
    pub person: PersonCard,
    pub priority: String,
    pub recommended_action: String,
    /// docs/specs/SLICE_003.md §3 `TodayReason` objects (`code` plus the
    /// coded fields), each carrying an additional `explanation` line built
    /// from the coded payload only (docs/specs/SLICE_006c.md §5a).
    pub reasons: Vec<serde_json::Value>,
    pub waiting_since: Option<DateTime<Utc>>,
    pub last_contact_attempt: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayView {
    pub generated_at: DateTime<Utc>,
    pub total: usize,
    pub truncated: bool,
    pub sources: TodaySourcesView,
    pub items: Vec<TodayItemView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextWorkItem {
    pub item: Option<TodayItemView>,
    pub total: usize,
    pub truncated: bool,
    pub sources: TodaySourcesView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ahead {
    pub high: usize,
    pub normal: usize,
    /// List-only saved-list items ahead of this item (Slice 011c).
    pub list: usize,
    /// `low` "outcome needed" items ahead (SLICE_006c §5a, D-033; additive).
    pub low: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum NotOnTodayReason {
    NotInReturnedToday,
}

/// `explain_priority`'s result (docs/specs/SLICE_005.md §3). `person` is
/// carried on both variants so the loop can build the reference card §4
/// requires from this tool without a second call; the adapter has already
/// resolved it through the Organization scope (an invisible id is
/// `ToolError::NotFound`, never a variant here).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PriorityExplanation {
    OnToday {
        person: PersonCard,
        position: usize,
        total: usize,
        priority: String,
        reasons: Vec<serde_json::Value>,
        waiting_since: Option<DateTime<Utc>>,
        last_contact_attempt: Option<DateTime<Utc>>,
        recommended_action: String,
        ordering_rule: &'static str,
        ahead: Ahead,
        sources: TodaySourcesView,
    },
    NotOnToday {
        person: PersonCard,
        #[serde(flatten)]
        reason: NotOnTodayReason,
        truncated: bool,
        sources: TodaySourcesView,
    },
}

impl PriorityExplanation {
    pub fn person(&self) -> &PersonCard {
        match self {
            PriorityExplanation::OnToday { person, .. }
            | PriorityExplanation::NotOnToday { person, .. } => person,
        }
    }
}

/// A saved list's identity as returned to the model (docs/specs/
/// SLICE_013.md §2): `name` is user-authored (011c §6), so it is wrapped;
/// `scope` is a fixed `"personal"`/`"shared"` token, not user text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedListRef {
    pub list_id: Uuid,
    pub name: UntrustedText,
    pub scope: String,
}

/// `filter_people`'s and `run_saved_list`'s shared result view
/// (docs/specs/SLICE_013.md §2). `Matched` is a successful filter or list
/// evaluation; `NeedsClarification` is also a **successful** call (§1 rule
/// 2 — it resets the malformed-call counter, never a strike) reporting only
/// the vocabulary for the dimension(s) that failed; `ListInvalid` reports a
/// saved list whose stored definition cannot be evaluated (`error` is one
/// of `unsupported_filter`/`invalid_stage`/`invalid_assignee`/
/// `invalid_tag`, a fixed code, never echoed text).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FilterOutcome {
    Matched(FilterResult),
    NeedsClarification {
        unknown_stages: Vec<String>,
        unknown_tags: Vec<String>,
        unknown_assignees: Vec<String>,
        ambiguous_assignees: Vec<String>,
        available_stages: Vec<String>,
        available_tags: Vec<UntrustedText>,
        members: Vec<String>,
        candidate_lists: Vec<SavedListRef>,
    },
    ListInvalid {
        error: String,
    },
}

/// A successful `filter_people`/`run_saved_list` evaluation (docs/specs/
/// SLICE_013.md §2). `list` is `None` for `filter_people` and `Some` for
/// `run_saved_list`; `description` lines are `describe()` output, wrapped
/// because stage/tag/member names inside them are user-authored (§1 rule
/// 6); `count` is `min(matches, 500)` and `more_than_500` mirrors the
/// People page's truncation (§1 rule 4); `returned` is `matches.len()`
/// after the `limit` cut.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterResult {
    pub list: Option<SavedListRef>,
    pub description: Vec<UntrustedText>,
    pub count: usize,
    pub more_than_500: bool,
    pub returned: usize,
    pub matches: Vec<PersonCard>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_text_clips_to_500_chars() {
        let long = "é".repeat(700);
        let t = UntrustedText::new(&long);
        assert_eq!(t.as_str().chars().count(), 500);
    }

    #[test]
    fn untrusted_text_strips_control_chars_and_flattens_whitespace() {
        let t = UntrustedText::new("a\u{0}b\u{1b}[31m\nc\td\r");
        assert_eq!(t.as_str(), "ab[31m c d ");
    }

    #[test]
    fn untrusted_text_strips_bidi_and_zero_width() {
        let t = UntrustedText::new("a\u{202E}b\u{200B}c\u{FEFF}");
        assert_eq!(t.as_str(), "abc");
    }

    #[test]
    fn untrusted_text_serializes_under_named_key() {
        let v = serde_json::to_value(UntrustedText::new("hi")).unwrap();
        assert_eq!(v, serde_json::json!({"untrusted_text": "hi"}));
    }

    #[test]
    fn person_card_prompt_and_wire_serializations_differ_only_in_wrapping() {
        let card = PersonCard {
            id: Uuid::nil(),
            display_name: UntrustedText::new("Grace Hopper"),
            stage_name: "Lead".into(),
            assigned_user_display_name: Some("Alice".into()),
            primary_email: Some(UntrustedText::new("grace@example.com")),
            primary_phone: None,
            inquiry_count: 2,
            last_inquiry_at: None,
        };
        let prompt = serde_json::to_value(&card).unwrap();
        assert_eq!(
            prompt["display_name"],
            serde_json::json!({"untrusted_text": "Grace Hopper"})
        );
        assert_eq!(
            prompt["primary_email"],
            serde_json::json!({"untrusted_text": "grace@example.com"})
        );
        let wire = serde_json::to_value(card.to_wire()).unwrap();
        assert_eq!(wire["display_name"], serde_json::json!("Grace Hopper"));
        assert_eq!(
            wire["primary_email"],
            serde_json::json!("grace@example.com")
        );
        assert_eq!(wire["primary_phone"], serde_json::Value::Null);
        assert_eq!(wire["stage_name"], serde_json::json!("Lead"));
    }

    #[test]
    fn not_on_today_reason_flattens_into_explanation() {
        let card = PersonCard {
            id: Uuid::nil(),
            display_name: UntrustedText::new("G"),
            stage_name: "Lead".into(),
            assigned_user_display_name: None,
            primary_email: None,
            primary_phone: None,
            inquiry_count: 0,
            last_inquiry_at: None,
        };
        let v = serde_json::to_value(PriorityExplanation::NotOnToday {
            person: card,
            reason: NotOnTodayReason::NotInReturnedToday,
            truncated: true,
            sources: TodaySourcesView {
                status: "complete".to_string(),
                issues: vec![],
                system_feed_issues: vec![],
            },
        })
        .unwrap();
        assert_eq!(v["status"], "not_on_today");
        assert_eq!(v["reason"], "not_in_returned_today");
    }

    #[test]
    fn filter_outcome_variants_tag_by_status_and_wrap_untrusted_text() {
        let matched = serde_json::to_value(FilterOutcome::Matched(FilterResult {
            list: Some(SavedListRef {
                list_id: Uuid::nil(),
                name: UntrustedText::new("Stale Zillow"),
                scope: "personal".to_string(),
            }),
            description: vec![UntrustedText::new("Stage is Lead")],
            count: 3,
            more_than_500: false,
            returned: 3,
            matches: vec![],
        }))
        .unwrap();
        assert_eq!(matched["status"], "matched");
        assert_eq!(
            matched["list"]["name"],
            serde_json::json!({"untrusted_text": "Stale Zillow"})
        );
        assert_eq!(
            matched["description"][0],
            serde_json::json!({"untrusted_text": "Stage is Lead"})
        );

        let clarification = serde_json::to_value(FilterOutcome::NeedsClarification {
            unknown_stages: vec!["Bogus".to_string()],
            unknown_tags: vec![],
            unknown_assignees: vec![],
            ambiguous_assignees: vec![],
            available_stages: vec!["Lead".to_string()],
            available_tags: vec![],
            members: vec![],
            candidate_lists: vec![],
        })
        .unwrap();
        assert_eq!(clarification["status"], "needs_clarification");
        assert_eq!(clarification["unknown_stages"][0], "Bogus");

        let invalid = serde_json::to_value(FilterOutcome::ListInvalid {
            error: "invalid_tag".to_string(),
        })
        .unwrap();
        assert_eq!(invalid["status"], "list_invalid");
        assert_eq!(invalid["error"], "invalid_tag");
    }
}
