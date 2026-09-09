use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::person::model::UserRef;
use crate::ids::{PersonId, TaskId};

use super::error::TaskError;

/// The closed `kind` enum (docs/specs/SLICE_016.md §1 rule 2, §2): drives
/// the recommended action on Today (016b) and is the FUB `type`
/// destination. `Default` is `FollowUp` (`due_at`-less requests still need
/// a value; the route's request struct applies this default via
/// `#[serde(default)]`). The variant names round-trip through
/// `serde(rename_all = "snake_case")` to the exact CHECK-matrix strings
/// (`FollowUp` -> `"follow_up"`), but decoding a *stored* row still goes
/// through [`TaskKind::from_db_str`] rather than `serde_json`, matching
/// every other closed-enum-in-a-TEXT-column precedent (`Role`,
/// `MembershipStatus`) — a `serde`-only round trip would silently accept
/// any string `serde` itself considers valid instead of failing closed on
/// a corrupt/unexpected stored value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Call,
    Email,
    Text,
    #[default]
    FollowUp,
    Other,
}

impl TaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskKind::Call => "call",
            TaskKind::Email => "email",
            TaskKind::Text => "text",
            TaskKind::FollowUp => "follow_up",
            TaskKind::Other => "other",
        }
    }

    /// `None` for an unrecognized stored value — a read path fails closed
    /// (`TaskError::Corrupt`), never guesses (the `Role::from_db_str`
    /// precedent).
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "call" => Some(TaskKind::Call),
            "email" => Some(TaskKind::Email),
            "text" => Some(TaskKind::Text),
            "follow_up" => Some(TaskKind::FollowUp),
            "other" => Some(TaskKind::Other),
            _ => None,
        }
    }
}

/// The exact `Task` response shape (docs/specs/SLICE_016.md §4):
/// `{"id","person_id","title","kind","due_at","assignee","created_by",
/// "completed_at","completed_by","created_at","updated_at","can_manage"}`.
/// `Debug` is a hand-written, redacting impl (never `#[derive(Debug)]`):
/// this struct carries a task title, and rule 7 (§1, §9) permits it at
/// exactly the response/receipt sites — a stray `?task`/`{:?}` in a log,
/// panic, or test-failure message must never print the title itself, only
/// its length.
#[derive(Clone, Serialize)]
pub struct Task {
    pub id: TaskId,
    pub person_id: PersonId,
    pub title: String,
    pub kind: TaskKind,
    pub due_at: Option<DateTime<Utc>>,
    pub assignee: Option<UserRef>,
    pub created_by: Option<UserRef>,
    pub completed_at: Option<DateTime<Utc>>,
    pub completed_by: Option<UserRef>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// The server's rule-1 verdict for the **viewer** at read time (admin,
    /// or the viewer is `assignee.id` or `created_by.id`); a display
    /// hint, re-decided under the row lock by every mutating command.
    pub can_manage: bool,
}

impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("id", &self.id)
            .field("person_id", &self.person_id)
            .field("title_chars", &self.title.chars().count())
            .field("kind", &self.kind)
            .field("due_at", &self.due_at)
            .field("assignee", &self.assignee)
            .field("created_by", &self.created_by)
            .field("completed_at", &self.completed_at)
            .field("completed_by", &self.completed_by)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("can_manage", &self.can_manage)
            .finish()
    }
}

/// Title validation (docs/specs/SLICE_016.md §1 rule 2, §3): a pure
/// function, unit-tested without a database. Trimmed (Rust's `trim()` is
/// Unicode-aware and strictly narrower than the §2 CHECK's ASCII-only
/// `btrim` — safe in the write direction, the `NoteBody` precedent),
/// 1–500 **code points** (`chars().count()`, matching the CHECK's
/// `char_length`), and rejected if **any** character is a control
/// character — unlike a note body, `\n` and `\t` are not exempted (a
/// task title is a single line; the CHECK's own
/// `position(E'\n' IN title) = 0` is implied by rejecting every control
/// character, not just newline).
pub struct TaskTitle;

impl TaskTitle {
    pub fn parse(raw: &str) -> Result<String, TaskError> {
        let trimmed = raw.trim();
        let count = trimmed.chars().count();
        if count == 0 || count > 500 {
            return Err(TaskError::MalformedRequest);
        }
        if trimmed.chars().any(|c| c.is_control()) {
            return Err(TaskError::MalformedRequest);
        }
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts_a_plain_title() {
        assert_eq!(
            TaskTitle::parse("  Call the client  ").unwrap(),
            "Call the client"
        );
    }

    #[test]
    fn empty_and_whitespace_only_are_rejected() {
        assert!(matches!(
            TaskTitle::parse(""),
            Err(TaskError::MalformedRequest)
        ));
        assert!(matches!(
            TaskTitle::parse("   \t  "),
            Err(TaskError::MalformedRequest)
        ));
    }

    #[test]
    fn exactly_500_chars_is_accepted_and_501_is_rejected() {
        let ok = "a".repeat(500);
        assert_eq!(TaskTitle::parse(&ok).unwrap().chars().count(), 500);

        let too_long = "a".repeat(501);
        assert!(matches!(
            TaskTitle::parse(&too_long),
            Err(TaskError::MalformedRequest)
        ));
    }

    #[test]
    fn exactly_500_four_byte_code_points_is_accepted() {
        // U+1F600 (grinning face) is a 4-byte UTF-8 / astral code point —
        // pins `chars().count()` against bytes or UTF-16 units.
        let ok = "\u{1F600}".repeat(500);
        assert_eq!(TaskTitle::parse(&ok).unwrap().chars().count(), 500);
    }

    #[test]
    fn newline_and_tab_are_rejected_unlike_a_note_body() {
        assert!(matches!(
            TaskTitle::parse("line one\nline two"),
            Err(TaskError::MalformedRequest)
        ));
        assert!(matches!(
            TaskTitle::parse("tabbed\ttitle"),
            Err(TaskError::MalformedRequest)
        ));
    }

    #[test]
    fn other_control_characters_are_rejected() {
        assert!(matches!(
            TaskTitle::parse("bad\u{0}null"),
            Err(TaskError::MalformedRequest)
        ));
        assert!(matches!(
            TaskTitle::parse("bad\u{1b}escape"),
            Err(TaskError::MalformedRequest)
        ));
        assert!(matches!(
            TaskTitle::parse("bad\rreturn"),
            Err(TaskError::MalformedRequest)
        ));
    }

    #[test]
    fn kind_as_str_and_from_db_str_round_trip() {
        for kind in [
            TaskKind::Call,
            TaskKind::Email,
            TaskKind::Text,
            TaskKind::FollowUp,
            TaskKind::Other,
        ] {
            assert_eq!(TaskKind::from_db_str(kind.as_str()), Some(kind));
        }
        assert_eq!(TaskKind::from_db_str("unknown_kind"), None);
    }

    #[test]
    fn kind_default_is_follow_up() {
        assert_eq!(TaskKind::default(), TaskKind::FollowUp);
    }
}
