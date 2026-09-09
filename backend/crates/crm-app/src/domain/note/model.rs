use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::domain::person::model::UserRef;
use crate::ids::{NoteId, PersonId};

use super::error::NoteError;

/// The exact `Note` response shape (docs/specs/SLICE_015.md §5):
/// `{"id","person_id","body","author": {"id","display_name"} | null,
/// "created_at","updated_at","edited","can_manage"}`. `Debug` is a
/// hand-written, redacting impl (never `#[derive(Debug)]`): this struct
/// carries a note body, and `AddNote`'s mutation receipt is one of the
/// exactly-three sites rule 7 permits — a stray `?note`/`{:?}` in a log,
/// panic, or test failure message must never print the body itself, only
/// its length.
#[derive(Clone, Serialize)]
pub struct Note {
    pub id: NoteId,
    pub person_id: PersonId,
    pub body: String,
    pub author: Option<UserRef>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// `updated_at > created_at` (docs/specs/SLICE_015.md §1 rule 4).
    pub edited: bool,
    /// The server's rule-1 verdict for the actor issuing this command
    /// (always `true` here — an `AddNote`/`EditNote` caller who reached
    /// this point already passed the permission check under the row
    /// lock); a display hint on the wire, never itself enforcing anything.
    pub can_manage: bool,
}

impl std::fmt::Debug for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Note")
            .field("id", &self.id)
            .field("person_id", &self.person_id)
            .field("body_chars", &self.body.chars().count())
            .field("author", &self.author)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("edited", &self.edited)
            .field("can_manage", &self.can_manage)
            .finish()
    }
}

/// Body validation (docs/specs/SLICE_015.md §1 rule 2, §3): a pure
/// function, unit-tested without a database. `\r\n` -> `\n`, trimmed
/// (Rust's `trim()` is Unicode-aware and strictly narrower than the §2
/// CHECK's ASCII-only `btrim` — safe in the write direction), 1–10,000
/// **code points** (`chars().count()`, matching the CHECK's
/// `char_length`, not bytes or UTF-16 units), and rejected if any
/// character other than `\n`/`\t` is a control character.
pub struct NoteBody;

impl NoteBody {
    pub fn parse(raw: &str) -> Result<String, NoteError> {
        let normalized = raw.replace("\r\n", "\n");
        let trimmed = normalized.trim();
        let count = trimmed.chars().count();
        if count == 0 || count > 10_000 {
            return Err(NoteError::MalformedRequest);
        }
        if trimmed
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(NoteError::MalformedRequest);
        }
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_crlf_and_trims() {
        assert_eq!(
            NoteBody::parse("  Hello\r\nWorld  ").unwrap(),
            "Hello\nWorld"
        );
    }

    #[test]
    fn empty_and_whitespace_only_are_rejected() {
        assert!(matches!(
            NoteBody::parse(""),
            Err(NoteError::MalformedRequest)
        ));
        assert!(matches!(
            NoteBody::parse("   \t  "),
            Err(NoteError::MalformedRequest)
        ));
    }

    #[test]
    fn exactly_10_000_chars_is_accepted_and_10_001_is_rejected() {
        let ok = "a".repeat(10_000);
        assert_eq!(NoteBody::parse(&ok).unwrap().chars().count(), 10_000);

        let too_long = "a".repeat(10_001);
        assert!(matches!(
            NoteBody::parse(&too_long),
            Err(NoteError::MalformedRequest)
        ));
    }

    #[test]
    fn exactly_10_000_four_byte_code_points_is_accepted() {
        // U+1F600 (grinning face) is a 4-byte UTF-8 / astral code point —
        // pins `chars().count()` against bytes or UTF-16 units.
        let ok = "\u{1F600}".repeat(10_000);
        assert_eq!(NoteBody::parse(&ok).unwrap().chars().count(), 10_000);
    }

    #[test]
    fn newline_and_tab_are_kept_other_control_characters_are_rejected() {
        assert_eq!(
            NoteBody::parse("line one\nline two\tend").unwrap(),
            "line one\nline two\tend"
        );
        assert!(matches!(
            NoteBody::parse("bad\u{0}null"),
            Err(NoteError::MalformedRequest)
        ));
        assert!(matches!(
            NoteBody::parse("bad\u{1b}escape"),
            Err(NoteError::MalformedRequest)
        ));
    }

    #[test]
    fn lone_carriage_return_not_part_of_crlf_is_rejected_as_a_control_character() {
        assert!(matches!(
            NoteBody::parse("bad\rreturn"),
            Err(NoteError::MalformedRequest)
        ));
    }
}
