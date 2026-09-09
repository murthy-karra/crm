//! Typed tasks on a Person (Slice 016a): relational CRUD (AGENTS.md §4.6,
//! D-053/D-054) — erasable, cascaded with the Person — except its one
//! derived history projection, `task_completed` (rank 8, only while a
//! task stays completed and live). Layout mirrors `domain::note` —
//! commands live here so Web, native clients, the public API, and the
//! Operator share one authorization and validation path (D-008). Tasks
//! deliberately require an **active** assignee (docs/specs/SLICE_016.md
//! §3's stated divergence from `AssignPerson`, which accepts inactive
//! members) — do not "harmonise" the two.

mod commands;
mod error;
mod model;
mod queries;

pub use commands::{
    complete_task, create_task, delete_task, reopen_task, snooze_task, update_task, CompleteTask,
    CompleteTaskOutcome, CreateTask, DeleteTask, DeleteTaskOutcome, ReopenTask, ReopenTaskOutcome,
    SnoozeTask, SnoozeTaskOutcome, UpdateTask, UpdateTaskOutcome,
};
pub use error::TaskError;
pub use model::{Task, TaskKind, TaskTitle};
pub use queries::open_for_person;

/// Internal wiring surface for `domain::person::queries::history_for_person`
/// (not part of this module's cross-crate public API — `pub(crate)`).
pub(crate) use queries::task_completed_history;
