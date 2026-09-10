//! `ToolBackend`: the complete data surface reachable from the tool loop
//! (docs/specs/SLICE_005.md §3; D-028 §5). `crm-api` implements it over
//! the existing `domain::` queries; tests implement it with fakes.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::context::OperatorContext;
use crate::views::{
    CompleteTaskOutcome, CreateTaskProposalOutcome, FilterOutcome, NextWorkItem, PersonDetail,
    PriorityExplanation, SearchResult, StartCallProposalOutcome, TodayView,
};

// --- `filter_people` / `run_saved_list` input types (docs/specs/
// SLICE_013.md §2; D-034) ---------------------------------------------------
//
// Both types are crm-operator-owned: no crm-app type (`FilterDefinition`,
// `Clause`, `StageId`, ...) crosses this seam. `crm-api`'s adapter resolves
// the names these carry against the caller's own Organization and builds a
// `FilterDefinition` from the public clause structs on its own side of the
// fence.

/// One age-axis condition (§2's `created`/`last_inquiry`/`last_contact`/
/// `last_inbound` fields), mirroring `crm-app`'s `AgeSpec` shape without
/// depending on it. `days` is already range-checked (1..=3650) by the
/// parser that builds this value; `Never` is rejected by that same parser
/// for `created` (§2: "`never` excluded"), so a `PeopleFilterSpec.created`
/// never holds `AgeCondition::Never` — enforced by construction, not by a
/// separate type, matching `crm-app::domain::person::filter::AgeSpec`'s own
/// single-type-plus-`allow_never`-flag precedent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeCondition {
    WithinDays(u32),
    NotWithinDays(u32),
    Never,
}

/// The `filter_people` tool's parsed, bounded arguments (docs/specs/
/// SLICE_013.md §2): a one-to-one mirror of the fifteen `Clause` kinds with
/// ids replaced by names. Every `Vec` is non-empty when the model supplied
/// the property at all (the parser rejects an empty array), so an empty
/// `Vec` here always means "the model omitted this property" — never "the
/// model asked for nothing on this axis". At least one field is populated;
/// the all-empty/all-`None` case is `invalid_arguments` before this type is
/// ever constructed (§1 rule 5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PeopleFilterSpec {
    pub stage_names: Vec<String>,
    pub assignees: Vec<String>,
    pub sources: Vec<String>,
    pub tag_names_any: Vec<String>,
    pub tag_names_none: Vec<String>,
    pub created: Option<AgeCondition>,
    pub last_inquiry: Option<AgeCondition>,
    pub last_contact: Option<AgeCondition>,
    pub last_inbound: Option<AgeCondition>,
    pub has_replied: Option<bool>,
    pub has_phone: Option<bool>,
    pub has_email: Option<bool>,
    pub awaiting_response: Option<bool>,
    pub client_replied_unanswered: Option<bool>,
    pub awaiting_call_outcome: Option<bool>,
    /// 1..=25, default 10 (§1 rule 4).
    pub limit: usize,
}

/// The `run_saved_list` tool's parsed arguments (docs/specs/SLICE_013.md
/// §2): exactly one of `name`/`list_id` is populated — enforced by the
/// parser, not by this type's shape (an `enum` would also work, but a
/// struct mirrors `PeopleFilterSpec`'s style and keeps both fields
/// individually named for the adapter). `list_id` is the one schema
/// property in either tool that is an id rather than a name (the
/// duplicate-name clarification's `candidate_lists`, the `start_call`
/// `contact_method_id` precedent); the adapter re-validates it through the
/// visibility predicate exactly like any other saved-list lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedListSelector {
    pub name: Option<String>,
    pub list_id: Option<Uuid>,
    /// 1..=25, default 10 (§1 rule 4).
    pub limit: usize,
}

/// `create_task`'s parsed, bounded arguments (docs/specs/SLICE_018.md §2,
/// §3): the seam type — crm-operator-owned, no crm-app type crosses it
/// (D-034). `due_at` is already a composed UTC instant by the time this is
/// built (the parser's job, from `due_date`/`due_time` and the turn's
/// `utc_offset_minutes`, pure — never a database); the model never
/// supplies an instant directly. `title`/`kind` are already structurally
/// validated (the mirrored `TaskTitle`/`TaskKind` checks); the adapter
/// re-runs the real validators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateTaskSpec {
    pub person_id: Uuid,
    pub title: String,
    pub kind: String,
    pub due_at: Option<DateTime<Utc>>,
    /// `None` means the default ("me"); `Some(name)` is `"me"` or a
    /// member's display name, cleaned and clipped by the parser.
    pub assignee: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Nonexistent *or* not visible under the caller's scope — byte-identical
    /// by design (docs/specs/SLICE_005.md §7).
    #[error("not found")]
    NotFound,
    /// Returned to the model as a structured error; the turn continues.
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    /// A backend (database) failure: aborts the turn with
    /// `TurnOutcome::ToolError`. The string is an operator-facing reason,
    /// never data.
    #[error("backend error: {0}")]
    Backend(String),
}

pub type ToolResult<T> = Result<T, ToolError>;

/// Every method takes the server-built context and returns a view scoped by
/// it. No method accepts an Organization or user id (§7).
#[async_trait]
pub trait ToolBackend: Send + Sync {
    async fn search_people(
        &self,
        ctx: &OperatorContext,
        query: &str,
        limit: usize,
    ) -> ToolResult<SearchResult>;

    async fn get_person(&self, ctx: &OperatorContext, person_id: Uuid) -> ToolResult<PersonDetail>;

    async fn get_today(&self, ctx: &OperatorContext, limit: usize) -> ToolResult<TodayView>;

    async fn get_next_work_item(&self, ctx: &OperatorContext) -> ToolResult<NextWorkItem>;

    async fn explain_priority(
        &self,
        ctx: &OperatorContext,
        person_id: Uuid,
    ) -> ToolResult<PriorityExplanation>;

    /// The one mutation-adjacent tool (docs/specs/SLICE_006b.md §3;
    /// D-009, D-034): validates and **proposes** a call — never places
    /// one. Execution happens on the model-free confirm endpoint after a
    /// human click. Ids only; the model can never supply a phone number.
    async fn propose_start_call(
        &self,
        ctx: &OperatorContext,
        person_id: Uuid,
        contact_method_id: Option<Uuid>,
    ) -> ToolResult<StartCallProposalOutcome>;

    /// `complete_task` (docs/specs/SLICE_018.md §2, D-057 §1): the first
    /// "low-risk and reversible" action — executes at once, as the
    /// signed-in member, exactly where the Task panel's own Complete
    /// button would succeed (D-053 rule 1). `person_id` and `task_id`
    /// bind together (the "reached through the Person path" invariant);
    /// a mismatch, foreign, or nonexistent pair is `ToolError::NotFound`,
    /// byte-identical.
    async fn complete_task(
        &self,
        ctx: &OperatorContext,
        person_id: Uuid,
        task_id: Uuid,
    ) -> ToolResult<CompleteTaskOutcome>;

    /// `create_task` (docs/specs/SLICE_018.md §2, D-057 §2): only
    /// *proposes* — never creates. Execution happens on the model-free
    /// confirm endpoint after a human click, the `propose_start_call`
    /// shape.
    async fn propose_create_task(
        &self,
        ctx: &OperatorContext,
        spec: &CreateTaskSpec,
    ) -> ToolResult<CreateTaskProposalOutcome>;

    /// Resolve `spec`'s names against the caller's Organization, build and
    /// validate a `FilterDefinition`, and run `filtered_summaries`
    /// (docs/specs/SLICE_013.md §2). Read-only, executes immediately (§1
    /// rule 7).
    async fn filter_people(
        &self,
        ctx: &OperatorContext,
        spec: &PeopleFilterSpec,
    ) -> ToolResult<FilterOutcome>;

    /// Resolve `selector` (by name or, for the duplicate-name case, by
    /// `list_id`) through the same visibility predicate the Lists index
    /// uses, honour its stored sort, and run the matching statement
    /// (docs/specs/SLICE_013.md §2, D-046, D-048). Read-only, executes
    /// immediately (§1 rule 7).
    async fn run_saved_list(
        &self,
        ctx: &OperatorContext,
        selector: &SavedListSelector,
    ) -> ToolResult<FilterOutcome>;
}
