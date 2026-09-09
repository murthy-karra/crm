//! Free-text notes on People (Slice 015): relational CRUD, cascaded with
//! the Person, never a history fact (AGENTS.md §4.6, D-053). Layout
//! mirrors `domain::tag` — commands live here so Web, native clients, the
//! public API, and the Operator share one authorization and validation
//! path (D-008).

mod commands;
mod error;
mod model;
mod queries;

pub use commands::{
    add_note, delete_note, edit_note, AddNote, DeleteNote, DeleteNoteOutcome, EditNote,
    EditNoteOutcome,
};
pub use error::NoteError;
pub use model::{Note, NoteBody};
pub use queries::{latest_for_person, NoteSummary};

/// Internal wiring surface for `domain::person::queries::history_for_person`
/// (not part of this module's cross-crate public API — `pub(crate)`).
pub(crate) use queries::note_history;
