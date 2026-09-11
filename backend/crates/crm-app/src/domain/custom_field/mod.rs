//! Typed custom fields on People (Slice 019a): relational CRUD, not
//! history (AGENTS.md §4.6) — definitions and options are archived, never
//! deleted (D-058 §2). Layout mirrors `domain::tag` — commands live here
//! so Web, native clients, the public API, and the Operator share one
//! authorization and validation path (D-008).

mod commands;
mod error;
mod model;
mod queries;

pub use commands::{
    add_custom_field_option, clear_person_custom_field_value, create_custom_field,
    normalize_and_validate_label, reorder_custom_fields, set_person_custom_field_value,
    update_custom_field, update_custom_field_option, AddCustomFieldOption,
    AddCustomFieldOptionOutcome, ClearPersonCustomFieldValue, CreateCustomField,
    CreateCustomFieldOutcome, PersonCustomFieldOutcome, ReorderCustomFields,
    ReorderCustomFieldsOutcome, SetPersonCustomFieldValue, UpdateCustomField,
    UpdateCustomFieldOption, UpdateCustomFieldOptionOutcome, UpdateCustomFieldOutcome,
};
pub use error::CustomFieldError;
pub use model::{CustomField, CustomFieldOption, CustomFieldValue, FieldType, Value};
pub use queries::{list_definitions, load_custom_field, values_for_person};
