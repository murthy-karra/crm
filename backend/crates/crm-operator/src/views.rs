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
    "high_before_normal_before_low, then waiting_since ascending (ended_at for low), then id";

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
    /// Latest 20.
    pub history: Vec<HistoryEntryView>,
    pub on_your_today: bool,
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
    pub waiting_since: DateTime<Utc>,
    pub last_contact_attempt: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayView {
    pub generated_at: DateTime<Utc>,
    pub total: usize,
    pub truncated: bool,
    pub items: Vec<TodayItemView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextWorkItem {
    pub item: Option<TodayItemView>,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ahead {
    pub high: usize,
    pub normal: usize,
    /// `low` "outcome needed" items ahead (SLICE_006c §5a, D-033; additive).
    pub low: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum NotOnTodayReason {
    NotAssignedToYou {
        assigned_user_display_name: Option<String>,
    },
    AlreadyContacted,
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
        waiting_since: DateTime<Utc>,
        recommended_action: String,
        ordering_rule: &'static str,
        ahead: Ahead,
    },
    NotOnToday {
        person: PersonCard,
        #[serde(flatten)]
        reason: NotOnTodayReason,
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
            reason: NotOnTodayReason::AlreadyContacted,
        })
        .unwrap();
        assert_eq!(v["status"], "not_on_today");
        assert_eq!(v["reason"], "already_contacted");
    }
}
