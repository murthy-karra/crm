//! Saved People lists (Slice 011b): relational CRUD around the versioned
//! `FilterDefinition` vocabulary. Commands live here rather than in the HTTP
//! layer so Web and future clients share authorization, optimistic revision,
//! retry, and reference-validation rules.

mod commands;
mod error;
mod queries;

pub(crate) use commands::{acquire_saved_lists_lock, lock_current_membership};
pub use commands::{
    create_saved_list, delete_saved_list, normalize_and_validate_input, update_saved_list,
    validate_expected_revision, CreateSavedList, CreateSavedListOutcome, DeleteSavedList,
    DeleteSavedListOutcome, UpdateSavedList, UpdateSavedListOutcome, MAX_WIRE_REVISION,
};
pub use error::SavedListError;
pub use queries::{
    count_saved_list_matches, list_saved_lists, saved_list_detail, SavedListCount, SavedListDetail,
    SavedListFilterError, SavedListMetadata, SavedListScope,
};
pub(crate) use queries::{decode_structural_filter, visible_live_row_for_update};
