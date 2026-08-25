//! The AI Operator (docs/specs/SLICE_005.md §3, §4; D-028, D-029).
//!
//! This crate is the whole Operator: the trusted [`OperatorContext`], the
//! [`ToolBackend`] trait that is the *only* data surface the model can
//! reach, the provider-neutral [`InferenceProvider`], the bounded tool loop
//! in [`OperatorService`], the view types the tools return, and the system
//! prompt. It depends on neither `sqlx` nor `axum` nor `crm-api` (D-028
//! §5): `crm-api` implements `ToolBackend` and calls `run_turn`.
//!
//! Nothing here logs or stores message text, reply text, tool arguments,
//! or tool results (D-029).

pub mod backend;
pub mod context;
pub mod provider;
pub mod providers;
pub mod service;
pub mod tools;
pub mod views;

pub use backend::{ToolBackend, ToolError, ToolResult};
pub use context::OperatorContext;
pub use provider::{
    ChatMessage, ChatRequest, ChatResponse, InferenceProvider, ProviderError, ResponseFormat,
    ToolCall, ToolChoice, ToolDefinition, Usage,
};
pub use providers::groq::{GroqApiKey, GroqConfig, GroqProvider, DEFAULT_CONNECT_TIMEOUT};
#[cfg(any(test, feature = "test-support"))]
pub use providers::scripted::{ScriptedProvider, ScriptedStep};
pub use service::{
    HistoryMessage, HistoryRole, Limits, OperatorService, References, ScreenContext, ScreenRoute,
    ToolCallOutcome, ToolCallRecord, TurnInput, TurnOutcome, TurnOutput,
};
pub use tools::tool_definitions;
pub use views::{
    Ahead, ContactMethodView, HistoryEntryView, InquiryView, NextWorkItem, NotOnTodayReason,
    PersonCard, PersonDetail, PhoneOption, PriorityExplanation, ProposalView, SearchResult,
    StartCallProposalOutcome, TodayItemView, TodayView, UntrustedText, WirePersonCard,
    ORDERING_RULE,
};

/// The system prompt (docs/specs/SLICE_005.md §3). Not a contract; the
/// five rules it encodes are tested through the loop, not by string match.
pub const SYSTEM_PROMPT: &str = include_str!("../prompts/system.md");
