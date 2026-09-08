//! Free-form tags on People (Slice 011e, e1): relational CRUD, not history
//! (AGENTS.md §4.6). Layout mirrors `domain::saved_list` — commands live
//! here so Web, native clients, the public API, and the Operator share one
//! authorization and validation path (D-008).

mod commands;
mod error;
mod model;
mod queries;

pub use commands::{
    add_person_tag, create_tag, delete_tag, normalize_and_validate_name, remove_person_tag,
    rename_tag, AddPersonTag, CreateTag, CreateTagOutcome, DeleteTag, DeleteTagOutcome,
    PersonTagOutcome, RemovePersonTag, RenameTag, RenameTagOutcome,
};
pub use error::TagError;
pub use model::{can_manage, Tag, TagRef};
pub use queries::{exists, list_for_organization, list_for_person, names_for, TagRow};
