//! `ToolBackend`: the complete data surface reachable from the tool loop
//! (docs/specs/SLICE_005.md §3; D-028 §5). `crm-api` implements it over
//! the existing `domain::` queries; tests implement it with fakes.

use async_trait::async_trait;
use uuid::Uuid;

use crate::context::OperatorContext;
use crate::views::{
    NextWorkItem, PersonDetail, PriorityExplanation, SearchResult, StartCallProposalOutcome,
    TodayView,
};

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
}
