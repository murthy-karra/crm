//! The bounded tool loop (docs/specs/SLICE_005.md §3, §4, §9).
//!
//! `OperatorService::run_turn` is infallible: every path — timeouts,
//! provider failures, backend failures — returns a `TurnOutput` so the
//! caller can always write the ledger row from the same value.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::Instrument;
use uuid::Uuid;

use crate::backend::{CreateTaskSpec, ToolBackend, ToolError};
use crate::context::OperatorContext;
use crate::provider::{
    ChatMessage, ChatRequest, ChatResponse, InferenceProvider, ProviderError, ToolCall, ToolChoice,
    Usage,
};
use crate::tools::{self, parse_invocation, tool_definitions, ArgumentError, ToolInvocation};
use crate::views::{
    CompleteTaskOutcome, CreateTaskProposalOutcome, FilterOutcome, PersonCard,
    StartCallProposalOutcome, TaskReceiptView, TurnProposal,
};
use crate::SYSTEM_PROMPT;

/// Total characters of replayed history (§4).
pub const MAX_HISTORY_CHARS: usize = 6000;
/// Reference cards returned per turn (§4, §14 item 4). 10 -> 25 (docs/specs/
/// SLICE_013.md §1 rule 4: the drawer shows every card the model was given
/// for `filter_people`/`run_saved_list`'s 25-card `limit`); side effect,
/// accepted: `get_today` can now place up to 20 cards in the drawer instead
/// of 10.
pub const MAX_REFERENCES: usize = 25;
/// `Unavailable` is retried once only if more than this remains (§4).
pub const RETRY_MIN_REMAINING: Duration = Duration::from_secs(5);

pub const CANNED_BUDGET_EXHAUSTED: &str = "I couldn't finish that — try asking more specifically.";
pub const CANNED_MALFORMED: &str = "I had trouble looking that up — try asking more specifically.";

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_rounds: u8,
    pub max_calls_per_round: u8,
    pub turn_timeout: Duration,
    pub max_history: usize,
    pub max_reply_chars: usize,
}

impl Default for Limits {
    /// docs/specs/SLICE_005.md §14 item 4.
    fn default() -> Self {
        Self {
            max_rounds: 4,
            max_calls_per_round: 3,
            turn_timeout: Duration::from_secs(20),
            max_history: 6,
            max_reply_chars: 1500,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryMessage {
    pub role: HistoryRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenRoute {
    Today,
    Person,
    People,
    Other,
}

/// An untrusted hint from the web (§2). `person_id` is re-validated
/// through the scope on every use, like any model-supplied id.
#[derive(Debug, Clone, Copy)]
pub struct ScreenContext {
    pub route: ScreenRoute,
    pub person_id: Option<Uuid>,
}

impl ScreenContext {
    pub fn other() -> Self {
        Self {
            route: ScreenRoute::Other,
            person_id: None,
        }
    }
}

/// `Debug` is redacted: `message` and `history` are user text (D-029).
#[derive(Clone)]
pub struct TurnInput {
    pub message: String,
    pub history: Vec<HistoryMessage>,
    pub screen: ScreenContext,
    /// docs/specs/SLICE_018.md §3: client-supplied, harmless data (the
    /// browser's `-new Date().getTimezoneOffset()`), already range-checked
    /// (−840..=840) by the HTTP layer before this is built; `None` when the
    /// client did not supply one. Threaded to `build_messages`'s local-time
    /// line and to the `create_task` due-instant composition — never part
    /// of `OperatorContext` (which stays "from `AuthContext` and nothing
    /// else", §7).
    pub utc_offset_minutes: Option<i32>,
}

impl std::fmt::Debug for TurnInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnInput")
            .field(
                "message",
                &format_args!("[{} chars]", self.message.chars().count()),
            )
            .field(
                "history",
                &format_args!("[{} messages]", self.history.len()),
            )
            .field("screen", &self.screen)
            .field("utc_offset_minutes", &self.utc_offset_minutes)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    ToolBudgetExhausted,
    MalformedToolCall,
    ModelTimeout,
    TurnTimeout,
    ProviderError,
    ToolError,
}

impl TurnOutcome {
    /// The ledger's `outcome` column value.
    pub fn as_str(self) -> &'static str {
        match self {
            TurnOutcome::Completed => "completed",
            TurnOutcome::ToolBudgetExhausted => "tool_budget_exhausted",
            TurnOutcome::MalformedToolCall => "malformed_tool_call",
            TurnOutcome::ModelTimeout => "model_timeout",
            TurnOutcome::TurnTimeout => "turn_timeout",
            TurnOutcome::ProviderError => "provider_error",
            TurnOutcome::ToolError => "tool_error",
        }
    }

    /// §5: the three 200 outcomes; the rest are 503s.
    pub fn is_reply(self) -> bool {
        matches!(
            self,
            TurnOutcome::Completed
                | TurnOutcome::ToolBudgetExhausted
                | TurnOutcome::MalformedToolCall
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallOutcome {
    Ok,
    NotFound,
    InvalidArguments,
    Error,
}

impl ToolCallOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolCallOutcome::Ok => "ok",
            ToolCallOutcome::NotFound => "not_found",
            ToolCallOutcome::InvalidArguments => "invalid_arguments",
            ToolCallOutcome::Error => "error",
        }
    }
}

/// One executed (or rejected) tool call — the ledger's `operator_tool_call`
/// row and the wire's `tool_calls[]` entry. `person_ids` is ledger-only.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallRecord {
    /// One of the five tool names, or `"unknown"` for a name the model
    /// invented (model text never reaches the ledger, D-029).
    pub name: &'static str,
    pub outcome: ToolCallOutcome,
    pub duration_ms: u32,
    #[serde(skip)]
    pub person_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct References {
    pub people: Vec<PersonCard>,
}

/// `Debug` is redacted: `reply` is model text and the cards carry names
/// (D-029).
#[derive(Clone)]
pub struct TurnOutput {
    /// `None` for the 503 outcomes.
    pub reply: Option<String>,
    pub references: References,
    pub tool_calls: Vec<ToolCallRecord>,
    /// The turn's single inserted proposal, if any — `start_call` or
    /// `create_task`, at most one per turn either way (docs/specs/
    /// SLICE_006b.md §3; docs/specs/SLICE_018.md §2). The wire renders the
    /// card from this object only, never from model prose.
    pub proposal: Option<TurnProposal>,
    /// The turn's `complete_task` receipt, if any — at most one per turn
    /// (docs/specs/SLICE_018.md §2). The wire renders the card from this
    /// object only, never from model prose.
    pub receipt: Option<TaskReceiptView>,
    pub outcome: TurnOutcome,
    pub usage: Usage,
    pub model_call_count: u32,
}

impl std::fmt::Debug for TurnOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let proposal_id = self.proposal.as_ref().map(|p| match p {
            TurnProposal::StartCall(view) => view.proposal_id,
            TurnProposal::CreateTask(view) => view.proposal_id,
        });
        f.debug_struct("TurnOutput")
            .field("reply", &self.reply.as_ref().map(|r| r.chars().count()))
            .field("references", &self.references.people.len())
            .field("tool_calls", &self.tool_calls)
            .field("proposal", &proposal_id)
            .field("receipt", &self.receipt.as_ref().map(|r| r.task_id))
            .field("outcome", &self.outcome)
            .field("usage", &self.usage)
            .field("model_call_count", &self.model_call_count)
            .finish()
    }
}

pub struct OperatorService {
    provider: Arc<dyn InferenceProvider>,
    limits: Limits,
}

// --- Internals --------------------------------------------------------

/// Reference precedence (§4): cards from `get_person`, `explain_priority`,
/// `get_next_work_item` first, then `search_people`, then `get_today`.
#[derive(Debug, Clone, Copy)]
enum RefBucket {
    Primary,
    Search,
    Today,
}

#[derive(Default)]
struct RefAccumulator {
    primary: Vec<PersonCard>,
    search: Vec<PersonCard>,
    today: Vec<PersonCard>,
}

impl RefAccumulator {
    fn add(&mut self, bucket: RefBucket, cards: impl IntoIterator<Item = PersonCard>) {
        match bucket {
            RefBucket::Primary => self.primary.extend(cards),
            RefBucket::Search => self.search.extend(cards),
            RefBucket::Today => self.today.extend(cards),
        }
    }

    fn finish(self) -> References {
        let mut seen = std::collections::HashSet::new();
        let people = self
            .primary
            .into_iter()
            .chain(self.search)
            .chain(self.today)
            .filter(|card| seen.insert(card.id))
            .take(MAX_REFERENCES)
            .collect();
        References { people }
    }
}

struct TurnState {
    messages: Vec<ChatMessage>,
    tool_calls: Vec<ToolCallRecord>,
    usage: Usage,
    model_call_count: u32,
    refs: RefAccumulator,
    consecutive_malformed: u8,
    consecutive_over_cap: u8,
    started: Instant,
    /// At most one inserted proposal per turn, either kind (docs/specs/
    /// SLICE_006b.md §3; docs/specs/SLICE_018.md §2); `NeedsNumberChoice`/
    /// `NoPhone`/`NeedsClarification` do not set this.
    proposal: Option<TurnProposal>,
    /// At most one `complete_task` receipt per turn (docs/specs/
    /// SLICE_018.md §2); `AlreadyCompleted`/`Forbidden` do not set this.
    receipt: Option<TaskReceiptView>,
    /// Copied from `TurnInput` at construction (docs/specs/SLICE_018.md
    /// §3): the `create_task` due-instant composition's only source of the
    /// client's time zone.
    utc_offset_minutes: Option<i32>,
}

enum LoopEnd {
    Reply(String, TurnOutcome),
    Abort(TurnOutcome),
}

enum ExecError {
    /// A structured `invalid_arguments` went back to the model; the caller
    /// decides whether the strike count ends the turn.
    Malformed,
    Abort(TurnOutcome),
}

/// What a successful `dispatch()` call does to the turn's single proposal/
/// receipt slots (docs/specs/SLICE_018.md §2). Kept out of `views.rs`: this
/// is a loop-internal concept, not a seam type.
enum ToolEffect {
    None,
    Proposal(TurnProposal),
    Receipt(Box<TaskReceiptView>),
}

fn tool_error_json(code: &str, detail: &str) -> String {
    json!({ "ok": false, "error": code, "detail": detail }).to_string()
}

fn tool_ok_json(result: Value) -> String {
    json!({ "ok": true, "result": result }).to_string()
}

fn ledger_name(model_supplied: &str) -> &'static str {
    match model_supplied {
        tools::SEARCH_PEOPLE => tools::SEARCH_PEOPLE,
        tools::GET_PERSON => tools::GET_PERSON,
        tools::GET_TODAY => tools::GET_TODAY,
        tools::GET_NEXT_WORK_ITEM => tools::GET_NEXT_WORK_ITEM,
        tools::EXPLAIN_PRIORITY => tools::EXPLAIN_PRIORITY,
        tools::START_CALL => tools::START_CALL,
        tools::FILTER_PEOPLE => tools::FILTER_PEOPLE,
        tools::RUN_SAVED_LIST => tools::RUN_SAVED_LIST,
        tools::COMPLETE_TASK => tools::COMPLETE_TASK,
        tools::CREATE_TASK => tools::CREATE_TASK,
        _ => "unknown",
    }
}

fn clip_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// The "current time" prompt line (docs/specs/SLICE_018.md §2): rendered
/// in the client's local offset instead of `Z` when one was supplied, so
/// the model can resolve "Friday"; `Z` (the pre-018 shape) when it was not.
fn local_time_line(now: chrono::DateTime<chrono::Utc>, utc_offset_minutes: Option<i32>) -> String {
    match utc_offset_minutes {
        Some(minutes) => match FixedOffset::east_opt(minutes * 60) {
            Some(offset) => now
                .with_timezone(&offset)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
            // Out-of-range values never reach here (the HTTP layer rejects
            // them, ±840 max), but fail to the pre-018 shape rather than
            // panic if that invariant is ever broken.
            None => now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        },
        None => now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    }
}

fn screen_line(screen: &ScreenContext) -> Option<String> {
    match (screen.route, screen.person_id) {
        (ScreenRoute::Today, _) => Some("(The user is viewing their Today list.)".to_string()),
        (ScreenRoute::Person, Some(id)) => Some(format!("(The user is viewing Person {id}.)")),
        (ScreenRoute::Person, None) => Some("(The user is viewing a Person page.)".to_string()),
        (ScreenRoute::People, _) => Some("(The user is viewing the People list.)".to_string()),
        (ScreenRoute::Other, _) => None,
    }
}

/// History (§4): at most `max_history` messages and `MAX_HISTORY_CHARS`
/// characters in total, oldest dropped first.
fn truncate_history(history: Vec<HistoryMessage>, max_history: usize) -> Vec<HistoryMessage> {
    let mut kept: Vec<HistoryMessage> = history;
    if kept.len() > max_history {
        kept.drain(0..kept.len() - max_history);
    }
    let mut total: usize = kept.iter().map(|m| m.content.chars().count()).sum();
    while total > MAX_HISTORY_CHARS && !kept.is_empty() {
        let dropped = kept.remove(0);
        total -= dropped.content.chars().count();
    }
    kept
}

impl OperatorService {
    pub fn new(provider: Arc<dyn InferenceProvider>, limits: Limits) -> Self {
        Self { provider, limits }
    }

    pub fn provider(&self) -> &dyn InferenceProvider {
        self.provider.as_ref()
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    fn build_messages(&self, ctx: &OperatorContext, input: TurnInput) -> Vec<ChatMessage> {
        let mut messages = Vec::with_capacity(input.history.len() + 2);
        // Member-entered (§14 item 13), but control characters are still
        // stripped so a display name cannot break the prompt's structure.
        let actor_name: String = ctx
            .actor_display_name
            .chars()
            .filter(|c| !c.is_control() && !crate::views::is_invisible_format(*c))
            .collect();
        messages.push(ChatMessage::System {
            content: format!(
                "{}\n\nThe member you are assisting is {}. The current time is {}.",
                SYSTEM_PROMPT.trim_end(),
                actor_name,
                local_time_line(ctx.now, input.utc_offset_minutes)
            ),
        });
        for message in truncate_history(input.history, self.limits.max_history) {
            messages.push(match message.role {
                HistoryRole::User => ChatMessage::User {
                    content: message.content,
                },
                HistoryRole::Assistant => ChatMessage::Assistant {
                    content: Some(message.content),
                    tool_calls: Vec::new(),
                },
            });
        }
        let content = match screen_line(&input.screen) {
            Some(line) => format!("{line}\n\n{}", input.message),
            None => input.message,
        };
        messages.push(ChatMessage::User { content });
        messages
    }

    /// Infallible: every path yields a `TurnOutput`. The whole loop runs
    /// under `turn_timeout`; on expiry the partial records gathered so far
    /// (tool calls, usage, call count) are still reported.
    pub async fn run_turn(
        &self,
        ctx: &OperatorContext,
        backend: &dyn ToolBackend,
        input: TurnInput,
    ) -> TurnOutput {
        let utc_offset_minutes = input.utc_offset_minutes;
        let mut state = TurnState {
            messages: self.build_messages(ctx, input),
            tool_calls: Vec::new(),
            usage: Usage::default(),
            model_call_count: 0,
            refs: RefAccumulator::default(),
            consecutive_malformed: 0,
            consecutive_over_cap: 0,
            started: Instant::now(),
            proposal: None,
            receipt: None,
            utc_offset_minutes,
        };

        let result = tokio::time::timeout(
            self.limits.turn_timeout,
            self.drive(ctx, backend, &mut state),
        )
        .await;

        let (reply, outcome) = match result {
            Ok(LoopEnd::Reply(text, outcome)) => (
                Some(clip_chars(&text, self.limits.max_reply_chars)),
                outcome,
            ),
            Ok(LoopEnd::Abort(outcome)) => (None, outcome),
            Err(_elapsed) => (None, TurnOutcome::TurnTimeout),
        };

        // A 503 outcome never surfaces its proposal or receipt (docs/specs/
        // SLICE_006b.md §10: "never shown; expires inert"; docs/specs/
        // SLICE_018.md §2, same rule extended to the receipt).
        let (proposal, receipt) = if outcome.is_reply() {
            (state.proposal, state.receipt)
        } else {
            (None, None)
        };

        TurnOutput {
            reply,
            references: state.refs.finish(),
            tool_calls: state.tool_calls,
            proposal,
            receipt,
            outcome,
            usage: state.usage,
            model_call_count: state.model_call_count,
        }
    }

    async fn drive(
        &self,
        ctx: &OperatorContext,
        backend: &dyn ToolBackend,
        state: &mut TurnState,
    ) -> LoopEnd {
        for _round in 0..self.limits.max_rounds {
            let response = match self.call_provider(state, ToolChoice::Auto).await {
                Ok(response) => response,
                Err(outcome) => return LoopEnd::Abort(outcome),
            };

            if response.tool_calls.is_empty() {
                return match non_empty(response.content) {
                    Some(text) => LoopEnd::Reply(text, TurnOutcome::Completed),
                    None => LoopEnd::Abort(TurnOutcome::ProviderError),
                };
            }

            state.messages.push(ChatMessage::Assistant {
                content: response.content.clone(),
                tool_calls: response.tool_calls.clone(),
            });

            let cap = usize::from(self.limits.max_calls_per_round);
            for (index, call) in response.tool_calls.iter().enumerate() {
                if index >= cap {
                    // Every tool_call id must be answered for the next
                    // request to be well-formed; extras are refused, not
                    // executed, and not recorded (nothing ran).
                    state.messages.push(ChatMessage::Tool {
                        tool_call_id: call.id.clone(),
                        content: tool_error_json(
                            "invalid_arguments",
                            &format!(
                                "too many tool calls in one round; at most {cap} are executed"
                            ),
                        ),
                    });
                    continue;
                }
                match self.execute(ctx, backend, state, call).await {
                    Ok(()) => {}
                    Err(ExecError::Abort(outcome)) => return LoopEnd::Abort(outcome),
                    Err(ExecError::Malformed) => {
                        if state.consecutive_malformed >= 2 {
                            return LoopEnd::Reply(
                                CANNED_MALFORMED.to_string(),
                                TurnOutcome::MalformedToolCall,
                            );
                        }
                    }
                }
            }
            if response.tool_calls.len() > cap {
                // Over-asking is a malformed round (§4: at most
                // `max_calls_per_round` are executed); two in a row end the
                // turn, counted separately from per-call strikes so the
                // round's executed calls cannot mask it.
                state.consecutive_over_cap += 1;
                if state.consecutive_over_cap >= 2 {
                    return LoopEnd::Reply(
                        CANNED_MALFORMED.to_string(),
                        TurnOutcome::MalformedToolCall,
                    );
                }
            } else {
                state.consecutive_over_cap = 0;
            }
        }

        // Rounds exhausted: one final call without tools (§4).
        match self.call_provider(state, ToolChoice::None).await {
            Ok(response) if response.tool_calls.is_empty() => match non_empty(response.content) {
                Some(text) => LoopEnd::Reply(text, TurnOutcome::Completed),
                None => LoopEnd::Reply(
                    CANNED_BUDGET_EXHAUSTED.to_string(),
                    TurnOutcome::ToolBudgetExhausted,
                ),
            },
            _ => LoopEnd::Reply(
                CANNED_BUDGET_EXHAUSTED.to_string(),
                TurnOutcome::ToolBudgetExhausted,
            ),
        }
    }

    /// One provider call with the §4 retry rules: `Unavailable` is retried
    /// once only if more than 5 s of budget remain; `RateLimited` and
    /// `Timeout` are never retried.
    async fn call_provider(
        &self,
        state: &mut TurnState,
        tool_choice: ToolChoice,
    ) -> Result<ChatResponse, TurnOutcome> {
        let request = ChatRequest {
            messages: state.messages.clone(),
            tools: tool_definitions(),
            tool_choice,
            response_format: None,
        };

        let first = self.one_call(state, request.clone(), 1).await;
        let result = match first {
            Err(ProviderError::Unavailable(_)) if self.remaining(state) > RETRY_MIN_REMAINING => {
                self.one_call(state, request, 2).await
            }
            other => other,
        };

        match result {
            Ok(response) => {
                state.usage.add(response.usage);
                Ok(response)
            }
            Err(ProviderError::Timeout) => Err(TurnOutcome::ModelTimeout),
            Err(ProviderError::RateLimited)
            | Err(ProviderError::Unavailable(_))
            | Err(ProviderError::Malformed(_)) => Err(TurnOutcome::ProviderError),
        }
    }

    async fn one_call(
        &self,
        state: &mut TurnState,
        request: ChatRequest,
        attempt: u8,
    ) -> Result<ChatResponse, ProviderError> {
        state.model_call_count += 1;
        let span = tracing::info_span!(
            "operator.provider_call",
            attempt,
            status = tracing::field::Empty
        );
        let result = self
            .provider
            .complete(request)
            .instrument(span.clone())
            .await;
        span.record(
            "status",
            match &result {
                Ok(_) => "ok",
                Err(ProviderError::Timeout) => "timeout",
                Err(ProviderError::RateLimited) => "rate_limited",
                Err(ProviderError::Unavailable(_)) => "unavailable",
                Err(ProviderError::Malformed(_)) => "malformed",
            },
        );
        result
    }

    fn remaining(&self, state: &TurnState) -> Duration {
        self.limits
            .turn_timeout
            .saturating_sub(state.started.elapsed())
    }

    async fn execute(
        &self,
        ctx: &OperatorContext,
        backend: &dyn ToolBackend,
        state: &mut TurnState,
        call: &ToolCall,
    ) -> Result<(), ExecError> {
        let name = ledger_name(&call.name);
        let started = Instant::now();
        // docs/specs/SLICE_013.md §6: `filter_kinds`/`filter_clause_count`/
        // `resolution`/`match_count`/`more_than_500`/`returned`/
        // `saved_list_scope` are declared here (an undeclared field is
        // silently dropped by `Span::record`, per `tracing`) but recorded
        // by the adapter (`crm-api`'s `SqlxToolBackend::filter_people`/
        // `run_saved_list`) while it runs inside this span — the same
        // ambient-span pattern `saved_list::queries::
        // count_saved_list_matches` already uses. crm-operator itself
        // never sees a name, id, or day count to record here even if it
        // wanted to (D-034 fence; D-029).
        // docs/specs/SLICE_018.md §11: `task_id`/`action_outcome`/
        // `title_chars` are recorded in `dispatch()` below for
        // `complete_task`/`create_task` — crm-operator's own data (the
        // model-supplied `task_id`, the backend's outcome tag, the title's
        // length), never the title itself.
        let span = tracing::info_span!(
            "operator.tool_call",
            tool = name,
            outcome = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
            filter_kinds = tracing::field::Empty,
            filter_clause_count = tracing::field::Empty,
            resolution = tracing::field::Empty,
            match_count = tracing::field::Empty,
            more_than_500 = tracing::field::Empty,
            returned = tracing::field::Empty,
            saved_list_scope = tracing::field::Empty,
            task_id = tracing::field::Empty,
            action_outcome = tracing::field::Empty,
            title_chars = tracing::field::Empty
        );

        let invocation = match parse_invocation(&call.name, &call.arguments) {
            Ok(invocation) => invocation,
            Err(err) => {
                let duration_ms = elapsed_ms(started);
                span.record("outcome", "invalid_arguments");
                span.record("duration_ms", duration_ms);
                state.tool_calls.push(ToolCallRecord {
                    name,
                    outcome: ToolCallOutcome::InvalidArguments,
                    duration_ms,
                    person_ids: Vec::new(),
                });
                state.messages.push(ChatMessage::Tool {
                    tool_call_id: call.id.clone(),
                    content: tool_error_json("invalid_arguments", &argument_message(&err)),
                });
                state.consecutive_malformed += 1;
                return Err(ExecError::Malformed);
            }
        };

        // One inserted proposal per turn, either kind (docs/specs/
        // SLICE_006b.md §3; docs/specs/SLICE_018.md §2): a second
        // `start_call`/`create_task` after a `Proposed` outcome is a
        // structured rejection with no backend hit.
        if matches!(
            invocation,
            ToolInvocation::StartCall { .. } | ToolInvocation::CreateTask { .. }
        ) && state.proposal.is_some()
        {
            let duration_ms = elapsed_ms(started);
            span.record("outcome", "invalid_arguments");
            span.record("duration_ms", duration_ms);
            state.tool_calls.push(ToolCallRecord {
                name,
                outcome: ToolCallOutcome::InvalidArguments,
                duration_ms,
                person_ids: Vec::new(),
            });
            state.messages.push(ChatMessage::Tool {
                tool_call_id: call.id.clone(),
                content: tool_error_json(
                    "invalid_arguments",
                    "a call or task is already proposed in this turn; the user must confirm or dismiss it first",
                ),
            });
            state.consecutive_malformed += 1;
            return Err(ExecError::Malformed);
        }

        // At most one executed `complete_task` per turn (docs/specs/
        // SLICE_018.md §2): the same guard shape as the proposal slot
        // above, keyed on whether a receipt already stands.
        if matches!(invocation, ToolInvocation::CompleteTask { .. }) && state.receipt.is_some() {
            let duration_ms = elapsed_ms(started);
            span.record("outcome", "invalid_arguments");
            span.record("duration_ms", duration_ms);
            state.tool_calls.push(ToolCallRecord {
                name,
                outcome: ToolCallOutcome::InvalidArguments,
                duration_ms,
                person_ids: Vec::new(),
            });
            state.messages.push(ChatMessage::Tool {
                tool_call_id: call.id.clone(),
                content: tool_error_json(
                    "invalid_arguments",
                    "a task was already completed in this turn",
                ),
            });
            state.consecutive_malformed += 1;
            return Err(ExecError::Malformed);
        }

        // Recorded as `error` *before* it runs: if the turn deadline fires
        // inside this tool (e.g. inside its DB query) the future is dropped
        // here, and the ledger still shows that the tool started (D-029's
        // audit value). Overwritten with the real outcome below.
        let slot = state.tool_calls.len();
        state.tool_calls.push(ToolCallRecord {
            name,
            outcome: ToolCallOutcome::Error,
            duration_ms: 0,
            person_ids: Vec::new(),
        });

        let result = dispatch(ctx, backend, &invocation, state.utc_offset_minutes)
            .instrument(span.clone())
            .await;
        let duration_ms = elapsed_ms(started);
        span.record("duration_ms", duration_ms);

        match result {
            Ok((value, bucket, cards, effect)) => {
                span.record("outcome", "ok");
                state.tool_calls[slot] = ToolCallRecord {
                    name,
                    outcome: ToolCallOutcome::Ok,
                    duration_ms,
                    person_ids: cards.iter().map(|c| c.id).collect(),
                };
                match effect {
                    ToolEffect::None => {}
                    ToolEffect::Proposal(p) => state.proposal = Some(p),
                    ToolEffect::Receipt(r) => state.receipt = Some(*r),
                }
                state.refs.add(bucket, cards);
                state.messages.push(ChatMessage::Tool {
                    tool_call_id: call.id.clone(),
                    content: tool_ok_json(value),
                });
                state.consecutive_malformed = 0;
                Ok(())
            }
            Err(ToolError::NotFound) => {
                span.record("outcome", "not_found");
                state.tool_calls[slot] = ToolCallRecord {
                    name,
                    outcome: ToolCallOutcome::NotFound,
                    duration_ms,
                    person_ids: Vec::new(),
                };
                state.messages.push(ChatMessage::Tool {
                    tool_call_id: call.id.clone(),
                    content: tool_error_json("not_found", not_found_detail(&invocation)),
                });
                state.consecutive_malformed = 0;
                Ok(())
            }
            Err(ToolError::InvalidArguments(detail)) => {
                span.record("outcome", "invalid_arguments");
                state.tool_calls[slot] = ToolCallRecord {
                    name,
                    outcome: ToolCallOutcome::InvalidArguments,
                    duration_ms,
                    person_ids: Vec::new(),
                };
                state.messages.push(ChatMessage::Tool {
                    tool_call_id: call.id.clone(),
                    content: tool_error_json("invalid_arguments", &detail),
                });
                state.consecutive_malformed += 1;
                Err(ExecError::Malformed)
            }
            Err(ToolError::Backend(reason)) => {
                span.record("outcome", "error");
                tracing::error!(tool = name, reason = %reason, "operator tool backend error");
                state.tool_calls[slot] = ToolCallRecord {
                    name,
                    outcome: ToolCallOutcome::Error,
                    duration_ms,
                    person_ids: Vec::new(),
                };
                Err(ExecError::Abort(TurnOutcome::ToolError))
            }
        }
    }
}

fn argument_message(err: &ArgumentError) -> String {
    err.message()
}

/// `ToolError::NotFound`'s fixed detail string, tool-aware (docs/specs/
/// SLICE_013.md §3): `run_saved_list` names a list, not a Person, so its
/// message says so; every other tool keeps the original Person wording.
/// Never echoes the model's argument text (D-029).
fn not_found_detail(invocation: &ToolInvocation) -> &'static str {
    match invocation {
        ToolInvocation::RunSavedList { .. } => "you cannot see a list by that name",
        // docs/specs/SLICE_018.md §3: "no such task on that Person" —
        // distinct from every other tool's Person-not-found wording, since
        // `person_id` and `task_id` both name resources here.
        ToolInvocation::CompleteTask { .. } => "no such task on that Person",
        ToolInvocation::SearchPeople { .. }
        | ToolInvocation::GetPerson { .. }
        | ToolInvocation::GetToday { .. }
        | ToolInvocation::GetNextWorkItem
        | ToolInvocation::ExplainPriority { .. }
        | ToolInvocation::StartCall { .. }
        | ToolInvocation::CreateTask { .. }
        | ToolInvocation::FilterPeople { .. } => "no such person in your Organization",
    }
}

fn non_empty(content: Option<String>) -> Option<String> {
    content.filter(|c| !c.trim().is_empty())
}

fn elapsed_ms(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

/// Runs one validated invocation and returns the JSON the model sees, the
/// reference bucket, the cards for `references` / `person_ids`, and the
/// effect (if any) on the turn's single proposal/receipt slots.
async fn dispatch(
    ctx: &OperatorContext,
    backend: &dyn ToolBackend,
    invocation: &ToolInvocation,
    utc_offset_minutes: Option<i32>,
) -> Result<(Value, RefBucket, Vec<PersonCard>, ToolEffect), ToolError> {
    fn to_value<T: Serialize>(v: &T) -> Result<Value, ToolError> {
        serde_json::to_value(v).map_err(|e| ToolError::Backend(format!("serialize: {e}")))
    }

    match invocation {
        ToolInvocation::SearchPeople { query, limit } => {
            let result = backend.search_people(ctx, query, *limit).await?;
            let value = to_value(&result)?;
            Ok((value, RefBucket::Search, result.matches, ToolEffect::None))
        }
        ToolInvocation::GetPerson { person_id } => {
            let detail = backend.get_person(ctx, *person_id).await?;
            let value = to_value(&detail)?;
            Ok((
                value,
                RefBucket::Primary,
                vec![detail.person],
                ToolEffect::None,
            ))
        }
        ToolInvocation::GetToday { limit } => {
            let view = backend.get_today(ctx, *limit).await?;
            let value = to_value(&view)?;
            let cards = view.items.into_iter().map(|i| i.person).collect();
            Ok((value, RefBucket::Today, cards, ToolEffect::None))
        }
        ToolInvocation::GetNextWorkItem => {
            let next = backend.get_next_work_item(ctx).await?;
            let value = to_value(&next)?;
            let cards = next.item.map(|i| i.person).into_iter().collect();
            Ok((value, RefBucket::Primary, cards, ToolEffect::None))
        }
        ToolInvocation::ExplainPriority { person_id } => {
            let explanation = backend.explain_priority(ctx, *person_id).await?;
            let value = to_value(&explanation)?;
            let card = explanation.person().clone();
            Ok((value, RefBucket::Primary, vec![card], ToolEffect::None))
        }
        ToolInvocation::StartCall {
            person_id,
            contact_method_id,
        } => {
            let outcome = backend
                .propose_start_call(ctx, *person_id, *contact_method_id)
                .await?;
            match outcome {
                StartCallProposalOutcome::Proposed(boxed) => {
                    let view = *boxed;
                    // The model sees a confirmation-pending summary; the
                    // wire card renders from `TurnOutput::proposal`, and
                    // the model is told the user must confirm in the UI.
                    let value = json!({
                        "status": "awaiting_user_confirmation",
                        "person": to_value(&view.person)?,
                        "phone": to_value(&view.phone)?,
                        "expires_at": view.expires_at,
                    });
                    let card = view.person.clone();
                    Ok((
                        value,
                        RefBucket::Primary,
                        vec![card],
                        ToolEffect::Proposal(TurnProposal::StartCall(Box::new(view))),
                    ))
                }
                StartCallProposalOutcome::NeedsNumberChoice { phones } => {
                    let value = json!({
                        "status": "choice_required",
                        "phones": to_value(&phones)?,
                    });
                    Ok((value, RefBucket::Primary, Vec::new(), ToolEffect::None))
                }
                StartCallProposalOutcome::NoPhone => {
                    let value = json!({
                        "status": "no_phone",
                    });
                    Ok((value, RefBucket::Primary, Vec::new(), ToolEffect::None))
                }
            }
        }
        // docs/specs/SLICE_013.md §4 precedence: `filter_people`/
        // `run_saved_list` cards share `RefBucket::Search`'s precedence
        // (equal standing with `search_people`'s matches, behind an
        // explicitly asked-about Person). `NeedsClarification`/
        // `ListInvalid` are still `Ok(...)` results here, not
        // `ToolError`s, so `execute()` treats either as a successful call
        // (outcome "ok") and resets `consecutive_malformed` — never a
        // strike (§1 rule 2).
        ToolInvocation::FilterPeople { spec } => {
            let outcome = backend.filter_people(ctx, spec).await?;
            let value = to_value(&outcome)?;
            let cards = match &outcome {
                FilterOutcome::Matched(result) => result.matches.clone(),
                FilterOutcome::NeedsClarification { .. } | FilterOutcome::ListInvalid { .. } => {
                    Vec::new()
                }
            };
            Ok((value, RefBucket::Search, cards, ToolEffect::None))
        }
        ToolInvocation::RunSavedList { selector } => {
            let outcome = backend.run_saved_list(ctx, selector).await?;
            let value = to_value(&outcome)?;
            let cards = match &outcome {
                FilterOutcome::Matched(result) => result.matches.clone(),
                FilterOutcome::NeedsClarification { .. } | FilterOutcome::ListInvalid { .. } => {
                    Vec::new()
                }
            };
            Ok((value, RefBucket::Search, cards, ToolEffect::None))
        }
        // docs/specs/SLICE_018.md §2, §3, §11.
        ToolInvocation::CompleteTask { person_id, task_id } => {
            let outcome = backend.complete_task(ctx, *person_id, *task_id).await?;
            let span = tracing::Span::current();
            match outcome {
                CompleteTaskOutcome::Completed(boxed) => {
                    let view = *boxed;
                    span.record("task_id", tracing::field::display(view.task_id));
                    span.record("action_outcome", "completed");
                    span.record("title_chars", view.title.as_str().chars().count());
                    let value = json!({
                        "status": "completed",
                        "task": {
                            "task_id": view.task_id,
                            "title": to_value(&view.title)?,
                            "kind": view.kind,
                            "due_at": view.due_at,
                            "completed_at": view.completed_at,
                            "completed_by_display_name": view.completed_by_display_name,
                        },
                    });
                    let card = view.person.clone();
                    Ok((
                        value,
                        RefBucket::Primary,
                        vec![card],
                        ToolEffect::Receipt(Box::new(view)),
                    ))
                }
                CompleteTaskOutcome::AlreadyCompleted(boxed) => {
                    let view = *boxed;
                    span.record("task_id", tracing::field::display(view.task_id));
                    span.record("action_outcome", "already_completed");
                    span.record("title_chars", view.title.as_str().chars().count());
                    let value = json!({
                        "status": "already_completed",
                        "task": {
                            "task_id": view.task_id,
                            "title": to_value(&view.title)?,
                            "kind": view.kind,
                            "due_at": view.due_at,
                            "completed_at": view.completed_at,
                            "completed_by_display_name": view.completed_by_display_name,
                        },
                    });
                    let card = view.person.clone();
                    Ok((value, RefBucket::Primary, vec![card], ToolEffect::None))
                }
                CompleteTaskOutcome::Forbidden => {
                    span.record("task_id", tracing::field::display(*task_id));
                    span.record("action_outcome", "forbidden");
                    let value = json!({ "status": "forbidden" });
                    Ok((value, RefBucket::Primary, Vec::new(), ToolEffect::None))
                }
            }
        }
        ToolInvocation::CreateTask {
            person_id,
            title,
            kind,
            due_date,
            due_time,
            assignee,
        } => {
            let span = tracing::Span::current();
            span.record("title_chars", title.chars().count());
            let due_at = compose_due_at(*due_date, *due_time, utc_offset_minutes)?;
            let spec = CreateTaskSpec {
                person_id: *person_id,
                title: title.clone(),
                kind: kind.clone(),
                due_at,
                assignee: assignee.clone(),
            };
            let outcome = backend.propose_create_task(ctx, &spec).await?;
            match outcome {
                CreateTaskProposalOutcome::Proposed(boxed) => {
                    let view = *boxed;
                    span.record("action_outcome", "proposed");
                    // The model sees a confirmation-pending summary; the
                    // wire card renders from `TurnOutput::proposal`, and
                    // the model is told the user must confirm in the UI.
                    let value = json!({
                        "status": "proposed",
                        "proposal": {
                            "person": to_value(&view.person)?,
                            "title": to_value(&view.title)?,
                            "kind": view.kind,
                            "due_at": view.due_at,
                            "assignee": to_value(&view.assignee)?,
                            "expires_at": view.expires_at,
                        },
                    });
                    let card = view.person.clone();
                    Ok((
                        value,
                        RefBucket::Primary,
                        vec![card],
                        ToolEffect::Proposal(TurnProposal::CreateTask(Box::new(view))),
                    ))
                }
                CreateTaskProposalOutcome::NeedsClarification {
                    unknown_assignees,
                    ambiguous_assignees,
                    members,
                } => {
                    span.record("action_outcome", "needs_clarification");
                    let value = json!({
                        "status": "needs_clarification",
                        "unknown_assignees": unknown_assignees,
                        "ambiguous_assignees": ambiguous_assignees,
                        "members": members,
                    });
                    Ok((value, RefBucket::Primary, Vec::new(), ToolEffect::None))
                }
            }
        }
    }
}

/// Composes `create_task`'s due instant from the parser's already-validated
/// `due_date`/`due_time` and the turn's `utc_offset_minutes` (docs/specs/
/// SLICE_018.md §3): pure `NaiveDate` + `NaiveTime` + `FixedOffset` ->
/// `DateTime<Utc>`, no database. A missing `due_date` composes to `None`
/// (no due time). A `due_date` with no offset fails closed —
/// `invalid_arguments`, never a UTC guess. Absent `due_time` defaults to
/// end of day, local (23:59:59).
fn compose_due_at(
    due_date: Option<chrono::NaiveDate>,
    due_time: Option<chrono::NaiveTime>,
    utc_offset_minutes: Option<i32>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, ToolError> {
    let Some(date) = due_date else {
        return Ok(None);
    };
    let Some(offset_minutes) = utc_offset_minutes else {
        return Err(ToolError::InvalidArguments(
            "a due date needs the client's local time zone, which was not supplied".to_string(),
        ));
    };
    let time = due_time.unwrap_or_else(|| {
        chrono::NaiveTime::from_hms_opt(23, 59, 59).expect("23:59:59 is always valid")
    });
    let offset = FixedOffset::east_opt(offset_minutes * 60)
        .ok_or_else(|| ToolError::InvalidArguments("invalid time zone offset".to_string()))?;
    let naive = NaiveDateTime::new(date, time);
    let local = offset
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| ToolError::InvalidArguments("ambiguous local time".to_string()))?;
    Ok(Some(local.with_timezone(&chrono::Utc)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{CreateTaskSpec, PeopleFilterSpec, SavedListSelector};
    use crate::providers::scripted::{ScriptedProvider, ScriptedStep};
    use crate::views::*;
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use std::sync::Mutex;

    fn ctx() -> OperatorContext {
        OperatorContext {
            actor_user_id: Uuid::new_v4(),
            organization_id: Uuid::new_v4(),
            actor_display_name: "Alice".to_string(),
            turn_id: Uuid::new_v4(),
            now: Utc.with_ymd_and_hms(2026, 8, 22, 12, 0, 0).unwrap(),
        }
    }

    fn card(id: Uuid, name: &str) -> PersonCard {
        PersonCard {
            id,
            display_name: UntrustedText::new(name),
            stage_name: "Lead".to_string(),
            assigned_user_display_name: Some("Alice".to_string()),
            primary_email: None,
            primary_phone: Some(UntrustedText::new("+15555550100")),
            inquiry_count: 1,
            last_inquiry_at: None,
        }
    }

    fn item(position: usize, id: Uuid, name: &str) -> TodayItemView {
        TodayItemView {
            position,
            person: card(id, name),
            priority: "high".to_string(),
            recommended_action: "call".to_string(),
            reasons: vec![json!({"code": "no_contact_attempt"})],
            waiting_since: Some(Utc.with_ymd_and_hms(2026, 8, 22, 11, 0, 0).unwrap()),
            last_contact_attempt: None,
        }
    }

    fn sources() -> TodaySourcesView {
        TodaySourcesView {
            status: "complete".to_string(),
            issues: vec![],
            system_feed_issues: vec![],
        }
    }

    /// Records the `OperatorContext` it receives on every call and serves
    /// fixed data; `backend_error` makes every call a `ToolError::Backend`.
    #[derive(Default)]
    struct FakeBackend {
        seen: Mutex<Vec<OperatorContext>>,
        backend_error: bool,
        not_found: bool,
        today_ids: Vec<Uuid>,
        search_ids: Vec<Uuid>,
        /// `propose_start_call` fixture: the Person's phone options.
        /// 0 => NoPhone; 1 => Proposed; >1 => NeedsNumberChoice unless a
        /// contact_method_id picks one.
        phones: Vec<Uuid>,
        /// `filter_people`'s `seen` fixture (docs/specs/SLICE_013.md §2):
        /// the ids `Matched` returns, in order.
        filter_people_ids: Vec<Uuid>,
        /// `run_saved_list`'s `seen` fixture, same shape.
        run_saved_list_ids: Vec<Uuid>,
        /// When set, `filter_people` returns `NeedsClarification` instead
        /// of `Matched` — still `Ok(...)`, so the loop treats it as a
        /// successful call (docs/specs/SLICE_013.md §1 rule 2).
        filter_people_needs_clarification: bool,
        /// `complete_task` fixture (docs/specs/SLICE_018.md §2): `Some`
        /// selects the returned outcome; `None` (the default) is
        /// `ToolError::NotFound`, the every-other-unset-fixture default.
        complete_task_outcome: Option<FakeCompleteOutcome>,
        /// `propose_create_task` fixture, same shape.
        create_task_outcome: Option<FakeCreateOutcome>,
    }

    #[derive(Clone, Copy)]
    enum FakeCompleteOutcome {
        Completed,
        AlreadyCompleted,
        Forbidden,
    }

    #[derive(Clone, Copy)]
    enum FakeCreateOutcome {
        Proposed,
        NeedsClarification,
    }

    impl FakeBackend {
        fn note(&self, ctx: &OperatorContext) -> Result<(), ToolError> {
            self.seen.lock().unwrap().push(ctx.clone());
            if self.backend_error {
                return Err(ToolError::Backend("db down".to_string()));
            }
            if self.not_found {
                return Err(ToolError::NotFound);
            }
            Ok(())
        }
    }

    #[async_trait]
    impl ToolBackend for FakeBackend {
        async fn search_people(
            &self,
            ctx: &OperatorContext,
            _query: &str,
            limit: usize,
        ) -> Result<SearchResult, ToolError> {
            self.note(ctx)?;
            let matches = self
                .search_ids
                .iter()
                .take(limit)
                .map(|id| card(*id, "S"))
                .collect();
            Ok(SearchResult {
                matches,
                truncated: false,
            })
        }
        async fn get_person(
            &self,
            ctx: &OperatorContext,
            person_id: Uuid,
        ) -> Result<PersonDetail, ToolError> {
            self.note(ctx)?;
            Ok(PersonDetail {
                person: card(person_id, "P"),
                contact_methods: vec![],
                inquiries: vec![],
                history: vec![],
                on_your_today: true,
                today_truncated: false,
                sources: sources(),
                tags: vec![],
                notes: vec![],
                tasks: vec![],
            })
        }
        async fn get_today(
            &self,
            ctx: &OperatorContext,
            limit: usize,
        ) -> Result<TodayView, ToolError> {
            self.note(ctx)?;
            let items: Vec<_> = self
                .today_ids
                .iter()
                .enumerate()
                .take(limit)
                .map(|(i, id)| item(i + 1, *id, "T"))
                .collect();
            Ok(TodayView {
                generated_at: ctx.now,
                total: self.today_ids.len(),
                truncated: false,
                sources: sources(),
                items,
            })
        }
        async fn get_next_work_item(
            &self,
            ctx: &OperatorContext,
        ) -> Result<NextWorkItem, ToolError> {
            self.note(ctx)?;
            Ok(NextWorkItem {
                item: self.today_ids.first().map(|id| item(1, *id, "N")),
                total: self.today_ids.len(),
                truncated: false,
                sources: sources(),
            })
        }
        async fn explain_priority(
            &self,
            ctx: &OperatorContext,
            person_id: Uuid,
        ) -> Result<PriorityExplanation, ToolError> {
            self.note(ctx)?;
            Ok(PriorityExplanation::OnToday {
                person: card(person_id, "E"),
                position: 1,
                total: 1,
                priority: "high".to_string(),
                reasons: vec![],
                waiting_since: Some(ctx.now),
                last_contact_attempt: None,
                recommended_action: "call".to_string(),
                ordering_rule: ORDERING_RULE,
                ahead: Ahead {
                    high: 0,
                    normal: 0,
                    list: 0,
                    low: 0,
                },
                sources: sources(),
            })
        }
        async fn propose_start_call(
            &self,
            ctx: &OperatorContext,
            person_id: Uuid,
            contact_method_id: Option<Uuid>,
        ) -> Result<StartCallProposalOutcome, ToolError> {
            self.note(ctx)?;
            let chosen = match contact_method_id {
                Some(id) => {
                    if !self.phones.contains(&id) {
                        return Err(ToolError::NotFound);
                    }
                    Some(id)
                }
                None if self.phones.len() == 1 => Some(self.phones[0]),
                None => None,
            };
            match chosen {
                Some(id) => Ok(StartCallProposalOutcome::Proposed(Box::new(ProposalView {
                    proposal_id: Uuid::new_v4(),
                    person: card(person_id, "P"),
                    phone: UntrustedText::new("(555) 015-0100"),
                    contact_method_id: id,
                    expires_at: ctx.now + chrono::Duration::seconds(120),
                }))),
                None if self.phones.is_empty() => Ok(StartCallProposalOutcome::NoPhone),
                None => Ok(StartCallProposalOutcome::NeedsNumberChoice {
                    phones: self
                        .phones
                        .iter()
                        .map(|id| PhoneOption {
                            contact_method_id: *id,
                            value: UntrustedText::new("(555) 015-0100"),
                        })
                        .collect(),
                }),
            }
        }

        async fn filter_people(
            &self,
            ctx: &OperatorContext,
            _spec: &PeopleFilterSpec,
        ) -> Result<FilterOutcome, ToolError> {
            self.note(ctx)?;
            if self.filter_people_needs_clarification {
                return Ok(FilterOutcome::NeedsClarification {
                    unknown_stages: vec!["Bogus".to_string()],
                    unknown_tags: vec![],
                    unknown_assignees: vec![],
                    ambiguous_assignees: vec![],
                    available_stages: vec!["Lead".to_string()],
                    available_tags: vec![],
                    members: vec![],
                    candidate_lists: vec![],
                });
            }
            let matches: Vec<PersonCard> = self
                .filter_people_ids
                .iter()
                .map(|id| card(*id, "F"))
                .collect();
            Ok(FilterOutcome::Matched(FilterResult {
                list: None,
                description: vec![UntrustedText::new("Stage is Lead")],
                count: matches.len(),
                more_than_500: false,
                returned: matches.len(),
                matches,
            }))
        }

        async fn run_saved_list(
            &self,
            ctx: &OperatorContext,
            _selector: &SavedListSelector,
        ) -> Result<FilterOutcome, ToolError> {
            self.note(ctx)?;
            let matches: Vec<PersonCard> = self
                .run_saved_list_ids
                .iter()
                .map(|id| card(*id, "L"))
                .collect();
            Ok(FilterOutcome::Matched(FilterResult {
                list: Some(SavedListRef {
                    list_id: Uuid::new_v4(),
                    name: UntrustedText::new("My List"),
                    scope: "personal".to_string(),
                }),
                description: vec![],
                count: matches.len(),
                more_than_500: false,
                returned: matches.len(),
                matches,
            }))
        }

        async fn complete_task(
            &self,
            ctx: &OperatorContext,
            person_id: Uuid,
            task_id: Uuid,
        ) -> Result<CompleteTaskOutcome, ToolError> {
            self.note(ctx)?;
            match self.complete_task_outcome {
                Some(FakeCompleteOutcome::Completed) => {
                    Ok(CompleteTaskOutcome::Completed(Box::new(TaskReceiptView {
                        task_id,
                        person: card(person_id, "P"),
                        title: UntrustedText::new("Call about the listing"),
                        kind: "call".to_string(),
                        due_at: Some(ctx.now),
                        completed_at: Some(ctx.now),
                        completed_by_display_name: Some("Alice".to_string()),
                    })))
                }
                Some(FakeCompleteOutcome::AlreadyCompleted) => Ok(
                    CompleteTaskOutcome::AlreadyCompleted(Box::new(TaskReceiptView {
                        task_id,
                        person: card(person_id, "P"),
                        title: UntrustedText::new("Call about the listing"),
                        kind: "call".to_string(),
                        due_at: Some(ctx.now),
                        completed_at: Some(ctx.now),
                        completed_by_display_name: Some("Bob".to_string()),
                    })),
                ),
                Some(FakeCompleteOutcome::Forbidden) => Ok(CompleteTaskOutcome::Forbidden),
                None => Err(ToolError::NotFound),
            }
        }

        async fn propose_create_task(
            &self,
            ctx: &OperatorContext,
            spec: &CreateTaskSpec,
        ) -> Result<CreateTaskProposalOutcome, ToolError> {
            self.note(ctx)?;
            match self.create_task_outcome {
                Some(FakeCreateOutcome::Proposed) => Ok(CreateTaskProposalOutcome::Proposed(
                    Box::new(TaskProposalView {
                        proposal_id: Uuid::new_v4(),
                        person: card(spec.person_id, "P"),
                        title: UntrustedText::new(&spec.title),
                        kind: spec.kind.clone(),
                        due_at: spec.due_at,
                        assignee: MemberRef {
                            id: ctx.actor_user_id,
                            display_name: "Alice".to_string(),
                        },
                        expires_at: ctx.now + chrono::Duration::seconds(120),
                    }),
                )),
                Some(FakeCreateOutcome::NeedsClarification) => {
                    Ok(CreateTaskProposalOutcome::NeedsClarification {
                        unknown_assignees: vec!["Bob".to_string()],
                        ambiguous_assignees: vec![],
                        members: vec!["Alice".to_string()],
                    })
                }
                None => Err(ToolError::NotFound),
            }
        }
    }

    fn call(id: &str, name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: args.to_string(),
        }
    }

    fn service(steps: Vec<ScriptedStep>, limits: Limits) -> (OperatorService, ScriptedProvider) {
        let provider = ScriptedProvider::new(steps);
        (
            OperatorService::new(Arc::new(provider.clone()), limits),
            provider,
        )
    }

    fn input(message: &str) -> TurnInput {
        TurnInput {
            message: message.to_string(),
            history: vec![],
            screen: ScreenContext::other(),
            utc_offset_minutes: None,
        }
    }

    /// docs/specs/SLICE_018.md §3: the due-instant composition's only
    /// source of the client's time zone.
    fn input_with_offset(message: &str, utc_offset_minutes: i32) -> TurnInput {
        TurnInput {
            utc_offset_minutes: Some(utc_offset_minutes),
            ..input(message)
        }
    }

    #[tokio::test]
    async fn happy_path_one_tool_call_then_answer() {
        let (svc, provider) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "get_next_work_item",
                    json!({}),
                )])),
                ScriptedStep::Respond(ChatResponse {
                    content: Some("Call Grace first.".into()),
                    tool_calls: vec![],
                    usage: Usage {
                        prompt_tokens: Some(100),
                        completion_tokens: Some(10),
                    },
                }),
            ],
            Limits::default(),
        );
        let grace = Uuid::new_v4();
        let backend = FakeBackend {
            today_ids: vec![grace],
            ..Default::default()
        };
        let out = svc.run_turn(&ctx(), &backend, input("Who next?")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert_eq!(out.reply.as_deref(), Some("Call Grace first."));
        assert_eq!(out.model_call_count, 2);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "get_next_work_item");
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::Ok);
        assert_eq!(out.tool_calls[0].person_ids, vec![grace]);
        assert_eq!(out.references.people.len(), 1);
        assert_eq!(out.references.people[0].id, grace);
        assert_eq!(out.usage.prompt_tokens, Some(100));

        // The second request carried the assistant tool call and its result.
        let reqs = provider.requests();
        assert_eq!(reqs.len(), 2);
        let last = &reqs[1].messages;
        assert!(
            matches!(&last[last.len() - 2], ChatMessage::Assistant { tool_calls, .. } if tool_calls.len() == 1)
        );
        assert!(
            matches!(&last[last.len() - 1], ChatMessage::Tool { tool_call_id, content } if tool_call_id == "c1" && content.contains("\"ok\":true"))
        );
        assert_eq!(reqs[0].tool_choice, ToolChoice::Auto);
    }

    #[tokio::test]
    async fn multi_round_accumulates_records_in_order() {
        let a = Uuid::new_v4();
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "search_people",
                    json!({"query": "gr"}),
                )])),
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c2",
                    "get_person",
                    json!({"person_id": a}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("Done.")),
            ],
            Limits::default(),
        );
        let backend = FakeBackend {
            search_ids: vec![a],
            ..Default::default()
        };
        let out = svc.run_turn(&ctx(), &backend, input("x")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert_eq!(out.model_call_count, 3);
        let names: Vec<_> = out.tool_calls.iter().map(|t| t.name).collect();
        assert_eq!(names, vec!["search_people", "get_person"]);
        assert_eq!(out.references.people.len(), 1);
    }

    #[tokio::test]
    async fn unknown_tool_yields_invalid_arguments_then_recovers() {
        let (svc, provider) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "delete_everything",
                    json!({}),
                )])),
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c2",
                    "get_next_work_item",
                    json!({}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("ok")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert_eq!(out.tool_calls[0].name, "unknown");
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::InvalidArguments);
        assert_eq!(out.tool_calls[1].outcome, ToolCallOutcome::Ok);
        let second = &provider.requests()[1].messages;
        assert!(
            matches!(second.last(), Some(ChatMessage::Tool { content, .. }) if content.contains("invalid_arguments"))
        );
    }

    #[tokio::test]
    async fn two_consecutive_malformed_calls_end_the_turn() {
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "get_person",
                    json!({"person_id": "nope"}),
                )])),
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c2",
                    "get_today",
                    json!({"organization_id": Uuid::new_v4()}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("never reached")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::MalformedToolCall);
        assert_eq!(out.reply.as_deref(), Some(CANNED_MALFORMED));
        assert_eq!(out.model_call_count, 2);
        assert_eq!(out.tool_calls.len(), 2);
    }

    #[tokio::test]
    async fn malformed_strike_count_resets_after_a_valid_call() {
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![
                    call("c1", "nope", json!({})),
                    call("c2", "get_next_work_item", json!({})),
                    call("c3", "nope", json!({})),
                ])),
                ScriptedStep::Respond(ChatResponse::text("fine")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
    }

    /// docs/specs/SLICE_013.md §1 rule 2: `needs_clarification` is an `Ok`
    /// outcome that resets `consecutive_malformed`, exactly like any other
    /// successful call — strike, clarification, strike (three separate
    /// rounds, so the malformed counter would trip at 2-in-a-row if the
    /// clarification in between did not reset it) still completes.
    #[tokio::test]
    async fn a_needs_clarification_result_resets_the_malformed_strike_count() {
        let (svc, _) = service(
            vec![
                // Strike 1: filter_people's own empty-spec check.
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "filter_people",
                    json!({}),
                )])),
                // A valid call the fake backend turns into
                // NeedsClarification — Ok, resets the counter.
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c2",
                    "filter_people",
                    json!({"stage_names": ["Bogus"]}),
                )])),
                // Strike 1 again — would be strike 2 (ending the turn) if
                // the clarification above had not reset the counter.
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c3",
                    "filter_people",
                    json!({}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("ok")),
            ],
            Limits::default(),
        );
        let backend = FakeBackend {
            filter_people_needs_clarification: true,
            ..Default::default()
        };
        let out = svc.run_turn(&ctx(), &backend, input("x")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert_eq!(out.tool_calls.len(), 3);
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::InvalidArguments);
        assert_eq!(out.tool_calls[1].outcome, ToolCallOutcome::Ok);
        assert_eq!(out.tool_calls[2].outcome, ToolCallOutcome::InvalidArguments);
    }

    #[tokio::test]
    async fn round_cap_makes_a_final_no_tools_call_then_canned_reply() {
        let tool_round = || {
            ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                "c",
                "get_next_work_item",
                json!({}),
            )]))
        };
        let (svc, provider) = service(
            vec![
                tool_round(),
                tool_round(),
                tool_round(),
                tool_round(),
                // The final call still tries to call a tool: canned reply.
                tool_round(),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::ToolBudgetExhausted);
        assert_eq!(out.reply.as_deref(), Some(CANNED_BUDGET_EXHAUSTED));
        assert_eq!(out.model_call_count, 5);
        assert_eq!(out.tool_calls.len(), 4);
        let reqs = provider.requests();
        assert_eq!(reqs[4].tool_choice, ToolChoice::None);
        assert!(reqs[..4].iter().all(|r| r.tool_choice == ToolChoice::Auto));
    }

    #[tokio::test]
    async fn round_cap_final_call_with_content_completes() {
        let tool_round = || {
            ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                "c",
                "get_next_work_item",
                json!({}),
            )]))
        };
        let (svc, _) = service(
            vec![
                tool_round(),
                tool_round(),
                tool_round(),
                tool_round(),
                ScriptedStep::Respond(ChatResponse::text("Here is what I found.")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert_eq!(out.reply.as_deref(), Some("Here is what I found."));
    }

    #[tokio::test]
    async fn extra_tool_calls_beyond_per_round_cap_are_refused_not_executed() {
        let calls: Vec<_> = (0..5)
            .map(|i| call(&format!("c{i}"), "get_next_work_item", json!({})))
            .collect();
        let (svc, provider) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(calls)),
                ScriptedStep::Respond(ChatResponse::text("ok")),
            ],
            Limits::default(),
        );
        let backend = FakeBackend::default();
        let out = svc.run_turn(&ctx(), &backend, input("x")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert_eq!(out.tool_calls.len(), 3);
        assert_eq!(backend.seen.lock().unwrap().len(), 3);
        let msgs = &provider.requests()[1].messages;
        let tool_msgs = msgs
            .iter()
            .filter(|m| matches!(m, ChatMessage::Tool { .. }))
            .count();
        assert_eq!(tool_msgs, 5, "every tool_call id is answered");
    }

    #[tokio::test]
    async fn provider_timeout_is_model_timeout_without_retry() {
        let (svc, provider) = service(
            vec![
                ScriptedStep::Fail(ProviderError::Timeout),
                ScriptedStep::Respond(ChatResponse::text("never")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::ModelTimeout);
        assert_eq!(out.reply, None);
        assert_eq!(out.model_call_count, 1);
        assert_eq!(provider.requests().len(), 1);
    }

    #[tokio::test]
    async fn rate_limited_is_provider_error_without_retry() {
        let (svc, provider) = service(
            vec![
                ScriptedStep::Fail(ProviderError::RateLimited),
                ScriptedStep::Respond(ChatResponse::text("never")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::ProviderError);
        assert_eq!(provider.requests().len(), 1);
    }

    #[tokio::test]
    async fn unavailable_is_retried_once_when_budget_remains() {
        let (svc, provider) = service(
            vec![
                ScriptedStep::Fail(ProviderError::Unavailable("503".into())),
                ScriptedStep::Respond(ChatResponse::text("recovered")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert_eq!(out.reply.as_deref(), Some("recovered"));
        assert_eq!(out.model_call_count, 2);
        assert_eq!(provider.requests().len(), 2);
    }

    #[tokio::test]
    async fn unavailable_twice_is_provider_error() {
        let (svc, _) = service(
            vec![
                ScriptedStep::Fail(ProviderError::Unavailable("503".into())),
                ScriptedStep::Fail(ProviderError::Unavailable("503".into())),
                ScriptedStep::Respond(ChatResponse::text("never")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::ProviderError);
        assert_eq!(out.model_call_count, 2);
    }

    #[tokio::test]
    async fn unavailable_is_not_retried_when_under_five_seconds_remain() {
        let (svc, provider) = service(
            vec![
                ScriptedStep::Fail(ProviderError::Unavailable("503".into())),
                ScriptedStep::Respond(ChatResponse::text("never")),
            ],
            Limits {
                turn_timeout: Duration::from_secs(4),
                ..Limits::default()
            },
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::ProviderError);
        assert_eq!(provider.requests().len(), 1);
    }

    #[tokio::test]
    async fn malformed_provider_body_is_provider_error() {
        let (svc, _) = service(
            vec![ScriptedStep::Fail(ProviderError::Malformed("bad".into()))],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::ProviderError);
    }

    #[tokio::test]
    async fn empty_content_without_tool_calls_is_provider_error() {
        let (svc, _) = service(
            vec![ScriptedStep::Respond(ChatResponse::text("   "))],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::ProviderError);
    }

    #[tokio::test]
    async fn turn_timeout_fires_with_a_sleeping_provider_and_keeps_partial_records() {
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "get_next_work_item",
                    json!({}),
                )])),
                ScriptedStep::SleepThenRespond(
                    Duration::from_secs(30),
                    ChatResponse::text("too late"),
                ),
            ],
            Limits {
                turn_timeout: Duration::from_millis(300),
                ..Limits::default()
            },
        );
        let started = Instant::now();
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(out.outcome, TurnOutcome::TurnTimeout);
        assert_eq!(out.reply, None);
        assert_eq!(out.model_call_count, 2);
        assert_eq!(
            out.tool_calls.len(),
            1,
            "the executed tool call survives the deadline"
        );
    }

    #[tokio::test]
    async fn turn_timeout_inside_a_tool_records_the_started_tool_as_error() {
        struct SleepyBackend;
        #[async_trait]
        impl ToolBackend for SleepyBackend {
            async fn search_people(
                &self,
                _: &OperatorContext,
                _: &str,
                _: usize,
            ) -> Result<SearchResult, ToolError> {
                unreachable!()
            }
            async fn get_person(
                &self,
                _: &OperatorContext,
                _: Uuid,
            ) -> Result<PersonDetail, ToolError> {
                unreachable!()
            }
            async fn get_today(
                &self,
                _: &OperatorContext,
                _: usize,
            ) -> Result<TodayView, ToolError> {
                unreachable!()
            }
            async fn get_next_work_item(
                &self,
                _: &OperatorContext,
            ) -> Result<NextWorkItem, ToolError> {
                tokio::time::sleep(Duration::from_secs(30)).await;
                unreachable!()
            }
            async fn explain_priority(
                &self,
                _: &OperatorContext,
                _: Uuid,
            ) -> Result<PriorityExplanation, ToolError> {
                unreachable!()
            }
            async fn propose_start_call(
                &self,
                _: &OperatorContext,
                _: Uuid,
                _: Option<Uuid>,
            ) -> Result<StartCallProposalOutcome, ToolError> {
                unreachable!()
            }
            async fn filter_people(
                &self,
                _: &OperatorContext,
                _: &PeopleFilterSpec,
            ) -> Result<FilterOutcome, ToolError> {
                unreachable!()
            }
            async fn run_saved_list(
                &self,
                _: &OperatorContext,
                _: &SavedListSelector,
            ) -> Result<FilterOutcome, ToolError> {
                unreachable!()
            }
            async fn complete_task(
                &self,
                _: &OperatorContext,
                _: Uuid,
                _: Uuid,
            ) -> Result<CompleteTaskOutcome, ToolError> {
                unreachable!()
            }
            async fn propose_create_task(
                &self,
                _: &OperatorContext,
                _: &CreateTaskSpec,
            ) -> Result<CreateTaskProposalOutcome, ToolError> {
                unreachable!()
            }
        }
        let (svc, _) = service(
            vec![ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                "c1",
                "get_next_work_item",
                json!({}),
            )]))],
            Limits {
                turn_timeout: Duration::from_millis(300),
                ..Limits::default()
            },
        );
        let started = Instant::now();
        let out = svc.run_turn(&ctx(), &SleepyBackend, input("x")).await;
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(out.outcome, TurnOutcome::TurnTimeout);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "get_next_work_item");
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::Error);
    }

    #[tokio::test]
    async fn final_no_tools_call_failure_is_budget_exhausted_not_provider_error() {
        let tool_round = || {
            ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                "c",
                "get_next_work_item",
                json!({}),
            )]))
        };
        let (svc, _) = service(
            vec![
                tool_round(),
                tool_round(),
                tool_round(),
                tool_round(),
                ScriptedStep::Fail(ProviderError::Timeout),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::ToolBudgetExhausted);
        assert_eq!(out.reply.as_deref(), Some(CANNED_BUDGET_EXHAUSTED));
    }

    #[tokio::test]
    async fn two_over_cap_rounds_are_malformed() {
        let big = || {
            ScriptedStep::Respond(ChatResponse::tool_calls(
                (0..5)
                    .map(|i| call(&format!("c{i}"), "get_next_work_item", json!({})))
                    .collect(),
            ))
        };
        let (svc, _) = service(vec![big(), big(), text_never()], Limits::default());
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::MalformedToolCall);
        assert_eq!(out.model_call_count, 2);
    }

    fn text_never() -> ScriptedStep {
        ScriptedStep::Respond(ChatResponse::text("never"))
    }

    #[tokio::test]
    async fn debug_output_never_carries_message_or_reply_text() {
        let input = TurnInput {
            message: "SECRET-MESSAGE".into(),
            history: vec![HistoryMessage {
                role: HistoryRole::User,
                content: "SECRET-HISTORY".into(),
            }],
            screen: ScreenContext::other(),
            utc_offset_minutes: None,
        };
        let debug = format!("{input:?}");
        assert!(!debug.contains("SECRET"));
        let (svc, provider) = service(
            vec![ScriptedStep::Respond(ChatResponse::text("SECRET-REPLY"))],
            Limits::default(),
        );
        let out = svc.run_turn(&ctx(), &FakeBackend::default(), input).await;
        assert!(!format!("{out:?}").contains("SECRET"));
        assert!(!format!("{:?}", provider.requests()).contains("SECRET"));
    }

    #[tokio::test]
    async fn backend_error_aborts_with_tool_error() {
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "get_today",
                    json!({}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("never")),
            ],
            Limits::default(),
        );
        let backend = FakeBackend {
            backend_error: true,
            ..Default::default()
        };
        let out = svc.run_turn(&ctx(), &backend, input("x")).await;
        assert_eq!(out.outcome, TurnOutcome::ToolError);
        assert_eq!(out.reply, None);
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::Error);
        assert_eq!(out.model_call_count, 1);
    }

    #[tokio::test]
    async fn not_found_is_returned_to_the_model_and_the_turn_continues() {
        let (svc, provider) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "get_person",
                    json!({"person_id": Uuid::new_v4()}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("I couldn't find that person.")),
            ],
            Limits::default(),
        );
        let backend = FakeBackend {
            not_found: true,
            ..Default::default()
        };
        let out = svc.run_turn(&ctx(), &backend, input("x")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::NotFound);
        assert!(out.tool_calls[0].person_ids.is_empty());
        assert!(out.references.people.is_empty());
        let msgs = &provider.requests()[1].messages;
        assert!(
            matches!(msgs.last(), Some(ChatMessage::Tool { content, .. }) if content.contains("not_found"))
        );
    }

    /// docs/specs/SLICE_013.md §3: `run_saved_list`'s `not_found` detail is
    /// tool-aware — a list, not a Person — and, like every `not_found`, is
    /// a successful call: the turn continues and the strike counter
    /// resets.
    #[tokio::test]
    async fn run_saved_list_not_found_uses_a_list_aware_detail_and_the_turn_continues() {
        let (svc, provider) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "run_saved_list",
                    json!({"name": "Stale Zillow"}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("I can't see a list by that name.")),
            ],
            Limits::default(),
        );
        let backend = FakeBackend {
            not_found: true,
            ..Default::default()
        };
        let out = svc.run_turn(&ctx(), &backend, input("x")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert_eq!(out.tool_calls[0].name, "run_saved_list");
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::NotFound);
        let msgs = &provider.requests()[1].messages;
        let content = match msgs.last() {
            Some(ChatMessage::Tool { content, .. }) => content,
            other => panic!("{other:?}"),
        };
        assert!(content.contains("not_found"));
        assert!(content.contains("you cannot see a list by that name"));
        assert!(!content.contains("no such person"));
    }

    #[tokio::test]
    async fn history_is_truncated_by_count_and_chars_oldest_first() {
        let (svc, provider) = service(
            vec![ScriptedStep::Respond(ChatResponse::text("ok"))],
            Limits::default(),
        );
        let history: Vec<_> = (0..8)
            .map(|i| HistoryMessage {
                role: if i % 2 == 0 {
                    HistoryRole::User
                } else {
                    HistoryRole::Assistant
                },
                content: format!("m{i}"),
            })
            .collect();
        svc.run_turn(
            &ctx(),
            &FakeBackend::default(),
            TurnInput {
                message: "now".into(),
                history,
                screen: ScreenContext::other(),
                utc_offset_minutes: None,
            },
        )
        .await;
        let msgs = &provider.requests()[0].messages;
        // system + 6 history + user
        assert_eq!(msgs.len(), 8);
        assert!(matches!(&msgs[1], ChatMessage::User { content } if content == "m2"));
        assert!(matches!(&msgs[6], ChatMessage::Assistant { content: Some(c), .. } if c == "m7"));
        assert!(matches!(&msgs[7], ChatMessage::User { content } if content == "now"));

        // Char budget: three 2500-char messages exceed 6000; the oldest goes.
        let (svc, provider) = service(
            vec![ScriptedStep::Respond(ChatResponse::text("ok"))],
            Limits::default(),
        );
        let history = vec![
            HistoryMessage {
                role: HistoryRole::User,
                content: "a".repeat(2500),
            },
            HistoryMessage {
                role: HistoryRole::Assistant,
                content: "b".repeat(2500),
            },
            HistoryMessage {
                role: HistoryRole::User,
                content: "c".repeat(2500),
            },
        ];
        svc.run_turn(
            &ctx(),
            &FakeBackend::default(),
            TurnInput {
                message: "now".into(),
                history,
                screen: ScreenContext::other(),
                utc_offset_minutes: None,
            },
        )
        .await;
        let msgs = &provider.requests()[0].messages;
        assert_eq!(msgs.len(), 4);
        assert!(
            matches!(&msgs[1], ChatMessage::Assistant { content: Some(c), .. } if c.starts_with('b'))
        );
    }

    #[tokio::test]
    async fn system_prompt_names_the_actor_and_screen_context_is_one_trusted_line() {
        let (svc, provider) = service(
            vec![ScriptedStep::Respond(ChatResponse::text("ok"))],
            Limits::default(),
        );
        let pid = Uuid::new_v4();
        svc.run_turn(
            &ctx(),
            &FakeBackend::default(),
            TurnInput {
                message: "why is she first?".into(),
                history: vec![],
                screen: ScreenContext {
                    route: ScreenRoute::Person,
                    person_id: Some(pid),
                },
                utc_offset_minutes: None,
            },
        )
        .await;
        let msgs = &provider.requests()[0].messages;
        assert!(
            matches!(&msgs[0], ChatMessage::System { content } if content.contains("Alice") && content.contains("untrusted_text"))
        );
        assert!(
            matches!(&msgs[1], ChatMessage::User { content } if content == &format!("(The user is viewing Person {pid}.)\n\nwhy is she first?"))
        );
    }

    #[tokio::test]
    async fn reply_is_clipped_to_max_reply_chars() {
        let (svc, _) = service(
            vec![ScriptedStep::Respond(ChatResponse::text("x".repeat(3000)))],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &FakeBackend::default(), input("x"))
            .await;
        assert_eq!(out.reply.unwrap().len(), 1500);
    }

    #[tokio::test]
    async fn references_follow_precedence_dedup_and_cap() {
        // docs/specs/SLICE_013.md §1 rule 4: MAX_REFERENCES rose 10 -> 25,
        // so the fixture supply must exceed 25 unique ids for the cap to
        // still bind (search's own limit is 10, get_today's is 20; the
        // fixture sizes below are the minimum that push total supply past
        // 25 with both buckets near their own per-call caps).
        let asked = Uuid::new_v4();
        let today_ids: Vec<Uuid> = (0..15).map(|_| Uuid::new_v4()).collect();
        let mut search_ids: Vec<Uuid> = (0..9).map(|_| Uuid::new_v4()).collect();
        search_ids.push(today_ids[3]);
        assert_eq!(search_ids.len(), 10, "at search_people's own limit");
        let (svc, _) = service(
            vec![
                // get_today first in time, but must not crowd out the
                // Person the user asked about.
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![
                    call("c1", "get_today", json!({"limit": 20})),
                    call("c2", "search_people", json!({"query": "x"})),
                    call("c3", "get_person", json!({"person_id": asked})),
                ])),
                ScriptedStep::Respond(ChatResponse::text("ok")),
            ],
            Limits::default(),
        );
        let backend = FakeBackend {
            today_ids: today_ids.clone(),
            search_ids: search_ids.clone(),
            ..Default::default()
        };
        let out = svc.run_turn(&ctx(), &backend, input("x")).await;
        let ids: Vec<Uuid> = out.references.people.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), MAX_REFERENCES);
        assert_eq!(ids[0], asked);
        assert_eq!(&ids[1..11], &search_ids[..]);
        // today_ids[3] already appeared via search_people, so it is
        // deduplicated out of the Today tail; the remaining 14 fill the
        // cap exactly (1 asked + 10 search + 14 today == 25).
        let expected_tail: Vec<Uuid> = today_ids
            .iter()
            .copied()
            .filter(|id| *id != today_ids[3])
            .take(14)
            .collect();
        assert_eq!(expected_tail.len(), 14);
        assert_eq!(&ids[11..], &expected_tail[..]);
        assert_eq!(ids.iter().filter(|id| **id == today_ids[3]).count(), 1);
        // The tool record for get_today carries every id it returned.
        assert_eq!(out.tool_calls[0].person_ids.len(), 15);
    }

    #[tokio::test]
    async fn context_reaches_the_backend_unchanged_regardless_of_model_arguments() {
        let foreign_org = Uuid::new_v4();
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![
                    // An invented trusted id is a schema violation.
                    call(
                        "c1",
                        "get_today",
                        json!({"organization_id": foreign_org, "limit": 5}),
                    ),
                    call("c2", "get_today", json!({"limit": 5})),
                ])),
                ScriptedStep::Respond(ChatResponse::text("ok")),
            ],
            Limits::default(),
        );
        let backend = FakeBackend::default();
        let ctx = ctx();
        let out = svc.run_turn(&ctx, &backend, input("x")).await;
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::InvalidArguments);
        assert_eq!(out.tool_calls[1].outcome, ToolCallOutcome::Ok);
        let seen = backend.seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "the rejected call never reached the backend");
        assert_eq!(seen[0].organization_id, ctx.organization_id);
        assert_eq!(seen[0].actor_user_id, ctx.actor_user_id);
        assert_eq!(seen[0].turn_id, ctx.turn_id);
        assert_ne!(seen[0].organization_id, foreign_org);
    }

    #[tokio::test]
    async fn tool_call_record_serializes_without_person_ids() {
        let record = ToolCallRecord {
            name: "get_today",
            outcome: ToolCallOutcome::Ok,
            duration_ms: 12,
            person_ids: vec![Uuid::new_v4()],
        };
        let v = serde_json::to_value(&record).unwrap();
        assert_eq!(
            v,
            json!({"name": "get_today", "outcome": "ok", "duration_ms": 12})
        );
    }

    // --- Slice 006b: start_call proposal flow (docs/specs/SLICE_006b.md §3) ---

    #[tokio::test]
    async fn start_call_proposal_lands_in_turn_output_and_ledger() {
        let person = Uuid::new_v4();
        let phone_method = Uuid::new_v4();
        let backend = FakeBackend {
            phones: vec![phone_method],
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "start_call",
                    json!({"person_id": person.to_string()}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("Ready — confirm the call to place it.")),
            ],
            Limits::default(),
        );
        let out = svc.run_turn(&ctx(), &backend, input("call P")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        let proposal = match out.proposal.expect("proposal on the output") {
            TurnProposal::StartCall(view) => view,
            other => panic!("expected a start_call proposal: {other:?}"),
        };
        assert_eq!(proposal.contact_method_id, phone_method);
        assert_eq!(proposal.person.id, person);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "start_call");
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::Ok);
        assert_eq!(out.tool_calls[0].person_ids, vec![person]);
    }

    #[tokio::test]
    async fn second_start_call_after_a_proposal_is_rejected_without_backend_hit() {
        let person = Uuid::new_v4();
        let backend = FakeBackend {
            phones: vec![Uuid::new_v4()],
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![
                    call("c1", "start_call", json!({"person_id": person.to_string()})),
                    call("c2", "start_call", json!({"person_id": person.to_string()})),
                ])),
                ScriptedStep::Respond(ChatResponse::text("done")),
            ],
            Limits::default(),
        );
        let out = svc.run_turn(&ctx(), &backend, input("call P twice")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert!(out.proposal.is_some(), "the first proposal stands");
        assert_eq!(out.tool_calls.len(), 2);
        assert_eq!(out.tool_calls[1].outcome, ToolCallOutcome::InvalidArguments);
        // Exactly one backend call reached propose_start_call: the fake
        // records every noted ctx; search/etc not called here.
        assert_eq!(backend.seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn needs_number_choice_and_no_phone_set_no_proposal() {
        for phones in [Vec::new(), vec![Uuid::new_v4(), Uuid::new_v4()]] {
            let person = Uuid::new_v4();
            let backend = FakeBackend {
                phones,
                ..Default::default()
            };
            let (svc, _) = service(
                vec![
                    ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                        "c1",
                        "start_call",
                        json!({"person_id": person.to_string()}),
                    )])),
                    ScriptedStep::Respond(ChatResponse::text("which number?")),
                ],
                Limits::default(),
            );
            let out = svc.run_turn(&ctx(), &backend, input("call P")).await;
            assert_eq!(out.outcome, TurnOutcome::Completed);
            assert!(out.proposal.is_none());
            assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::Ok);
        }
    }

    #[tokio::test]
    async fn a_503_outcome_never_surfaces_its_proposal() {
        // The tool runs (row inserted server-side), then the provider
        // dies: the wire must not show the proposal (§10: never shown;
        // expires inert).
        let person = Uuid::new_v4();
        let backend = FakeBackend {
            phones: vec![Uuid::new_v4()],
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "start_call",
                    json!({"person_id": person.to_string()}),
                )])),
                ScriptedStep::Fail(ProviderError::Timeout),
            ],
            Limits::default(),
        );
        let out = svc.run_turn(&ctx(), &backend, input("call P")).await;
        assert!(!out.outcome.is_reply());
        assert!(out.proposal.is_none());
    }

    #[tokio::test]
    async fn start_call_with_invented_contact_method_is_not_found() {
        let person = Uuid::new_v4();
        let backend = FakeBackend {
            phones: vec![Uuid::new_v4()],
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "start_call",
                    json!({
                        "person_id": person.to_string(),
                        "contact_method_id": Uuid::new_v4().to_string(),
                    }),
                )])),
                ScriptedStep::Respond(ChatResponse::text("could not find it")),
            ],
            Limits::default(),
        );
        let out = svc.run_turn(&ctx(), &backend, input("call P")).await;
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::NotFound);
        assert!(out.proposal.is_none());
    }

    // --- filter_people / run_saved_list wiring (docs/specs/SLICE_013.md
    // §8.3): the fake backend's `seen` assertions cover both new
    // `ToolBackend` methods, called directly. Full scripted-provider turn
    // loop coverage (dispatch, ledger, clarification-as-non-strike) is
    // below, mirroring `not_found_is_returned_to_the_model_and_the_turn_
    // continues`'s style.

    #[tokio::test]
    async fn fake_backend_filter_people_records_context_and_returns_matches() {
        let id = Uuid::new_v4();
        let backend = FakeBackend {
            filter_people_ids: vec![id],
            ..Default::default()
        };
        let spec = PeopleFilterSpec {
            has_phone: Some(true),
            limit: 10,
            ..Default::default()
        };
        let outcome = backend.filter_people(&ctx(), &spec).await.unwrap();
        assert_eq!(backend.seen.lock().unwrap().len(), 1);
        match outcome {
            FilterOutcome::Matched(result) => {
                assert_eq!(result.matches.len(), 1);
                assert_eq!(result.matches[0].id, id);
                assert_eq!(result.count, 1);
                assert_eq!(result.returned, 1);
                assert!(!result.more_than_500);
                assert!(result.list.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn fake_backend_run_saved_list_records_context_and_returns_matches() {
        let id = Uuid::new_v4();
        let backend = FakeBackend {
            run_saved_list_ids: vec![id],
            ..Default::default()
        };
        let selector = SavedListSelector {
            name: Some("Stale Zillow".to_string()),
            list_id: None,
            limit: 10,
        };
        let outcome = backend.run_saved_list(&ctx(), &selector).await.unwrap();
        assert_eq!(backend.seen.lock().unwrap().len(), 1);
        match outcome {
            FilterOutcome::Matched(result) => {
                assert_eq!(result.matches.len(), 1);
                assert_eq!(result.matches[0].id, id);
                assert!(result.list.is_some());
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn fake_backend_filter_and_saved_list_share_the_backend_error_and_not_found_fixtures() {
        let backend = FakeBackend {
            backend_error: true,
            ..Default::default()
        };
        let spec = PeopleFilterSpec {
            has_phone: Some(true),
            limit: 10,
            ..Default::default()
        };
        assert!(matches!(
            backend.filter_people(&ctx(), &spec).await,
            Err(ToolError::Backend(_))
        ));
        let selector = SavedListSelector {
            name: Some("x".to_string()),
            list_id: None,
            limit: 10,
        };
        assert!(matches!(
            backend.run_saved_list(&ctx(), &selector).await,
            Err(ToolError::Backend(_))
        ));
        assert_eq!(backend.seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn the_prompt_carries_the_start_call_rules() {
        // SLICE_006b §5: prepare-not-place. String-pinned like the actor
        // line — a reworded prompt that drops a rule must fail a test.
        let prompt = include_str!("../prompts/system.md");
        for rule in [
            "start_call",
            "never places a call",
            "Never claim a call was placed",
            "Never propose a call the user did not ask for",
            "you can never dial a number from the conversation",
            "ten tools",
        ] {
            assert!(prompt.contains(rule), "prompt lost the rule: {rule}");
        }
        assert!(
            !prompt.contains("read-only assistant"),
            "the read-only framing is gone (006b)"
        );
        assert!(!prompt.contains("six tools"), "the tool count is stale");
        assert!(!prompt.contains("eight tools"), "the tool count is stale");
    }

    /// docs/specs/SLICE_018.md §7: string-pinned like
    /// `the_prompt_carries_the_start_call_rules` above.
    #[test]
    fn the_prompt_carries_the_task_rules() {
        let prompt = include_str!("../prompts/system.md");
        for rule in [
            "complete_task",
            "create_task",
            "may complete a task",
            "complete_task executes at once",
            "may propose a task",
            "creates nothing until the member confirms",
            "never claim a task exists before",
            "needs_clarification",
            "who the task is assigned to",
            "who completed it and when",
            "never guessing a time zone",
        ] {
            assert!(prompt.contains(rule), "prompt lost the rule: {rule}");
        }
        assert!(
            !prompt.contains("create tasks"),
            "the \"cannot create tasks\" sentence must be gone (SLICE_018 §7)"
        );
    }

    /// docs/specs/SLICE_018.md §2, §3: the local-time line renders in the
    /// client's offset when one is supplied, and stays `Z` when it is not.
    #[test]
    fn local_time_line_renders_the_offset_or_falls_back_to_z() {
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 3, 30, 0).unwrap();
        // -240 minutes = UTC-4 (America/New_York in September).
        let with_offset = local_time_line(now, Some(-240));
        assert!(
            with_offset.starts_with("2026-09-10T23:30:00-04:00"),
            "{with_offset}"
        );
        assert!(!with_offset.ends_with('Z'));

        let without_offset = local_time_line(now, None);
        assert!(without_offset.ends_with('Z'), "{without_offset}");
    }

    /// docs/specs/SLICE_013.md §3: string-pinned like
    /// `the_prompt_carries_the_start_call_rules` above — a reworded prompt
    /// that drops one of these rules must fail a test.
    #[test]
    fn the_prompt_carries_the_filter_people_and_run_saved_list_rules() {
        let prompt = include_str!("../prompts/system.md");
        for rule in [
            "filter_people",
            "run_saved_list",
            "assignees: [\"me\"]",
            "People never contacted at all",
            "more than 500 matched",
            "description lines as the reason",
            "needs_clarification",
            "never retry the same call with a guessed name",
            "you cannot see a list by that name",
        ] {
            assert!(prompt.contains(rule), "prompt lost the rule: {rule}");
        }
    }

    // --- Slice 018: complete_task / create_task (docs/specs/SLICE_018.md
    // §2, §12) --------------------------------------------------------

    #[tokio::test]
    async fn complete_task_sets_the_turn_receipt() {
        let person = Uuid::new_v4();
        let task = Uuid::new_v4();
        let backend = FakeBackend {
            complete_task_outcome: Some(FakeCompleteOutcome::Completed),
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "complete_task",
                    json!({"person_id": person.to_string(), "task_id": task.to_string()}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("Done — marked it complete.")),
            ],
            Limits::default(),
        );
        let out = svc.run_turn(&ctx(), &backend, input("mark it done")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        let receipt = out.receipt.expect("receipt on the output");
        assert_eq!(receipt.task_id, task);
        assert_eq!(receipt.person.id, person);
        assert!(out.proposal.is_none());
    }

    /// `already_completed`/`forbidden` never set a receipt (docs/specs/
    /// SLICE_018.md §2).
    #[tokio::test]
    async fn already_completed_and_forbidden_set_no_receipt() {
        for outcome in [
            FakeCompleteOutcome::AlreadyCompleted,
            FakeCompleteOutcome::Forbidden,
        ] {
            let person = Uuid::new_v4();
            let task = Uuid::new_v4();
            let backend = FakeBackend {
                complete_task_outcome: Some(outcome),
                ..Default::default()
            };
            let (svc, _) = service(
                vec![
                    ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                        "c1",
                        "complete_task",
                        json!({"person_id": person.to_string(), "task_id": task.to_string()}),
                    )])),
                    ScriptedStep::Respond(ChatResponse::text("noted")),
                ],
                Limits::default(),
            );
            let out = svc.run_turn(&ctx(), &backend, input("mark it done")).await;
            assert_eq!(out.outcome, TurnOutcome::Completed);
            assert!(out.receipt.is_none());
        }
    }

    #[tokio::test]
    async fn second_complete_task_after_a_receipt_is_rejected_without_backend_hit() {
        let person = Uuid::new_v4();
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let backend = FakeBackend {
            complete_task_outcome: Some(FakeCompleteOutcome::Completed),
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![
                    call(
                        "c1",
                        "complete_task",
                        json!({"person_id": person.to_string(), "task_id": task_a.to_string()}),
                    ),
                    call(
                        "c2",
                        "complete_task",
                        json!({"person_id": person.to_string(), "task_id": task_b.to_string()}),
                    ),
                ])),
                ScriptedStep::Respond(ChatResponse::text("done")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &backend, input("mark both done"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert!(out.receipt.is_some(), "the first receipt stands");
        assert_eq!(out.tool_calls.len(), 2);
        assert_eq!(out.tool_calls[1].outcome, ToolCallOutcome::InvalidArguments);
        assert_eq!(backend.seen.lock().unwrap().len(), 1);
    }

    /// Review round 1: the receipt guard keys on a *standing receipt*
    /// (`state.receipt.is_some()`), not on "a complete_task already ran
    /// this turn" — `Forbidden` never sets a receipt (docs/specs/
    /// SLICE_018.md §2), so a second `complete_task` after a `Forbidden`
    /// outcome is NOT blocked and reaches the backend again.
    #[tokio::test]
    async fn a_second_complete_task_after_a_forbidden_outcome_still_reaches_the_backend() {
        let person = Uuid::new_v4();
        let task_a = Uuid::new_v4();
        let task_b = Uuid::new_v4();
        let backend = FakeBackend {
            complete_task_outcome: Some(FakeCompleteOutcome::Forbidden),
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![
                    call(
                        "c1",
                        "complete_task",
                        json!({"person_id": person.to_string(), "task_id": task_a.to_string()}),
                    ),
                    call(
                        "c2",
                        "complete_task",
                        json!({"person_id": person.to_string(), "task_id": task_b.to_string()}),
                    ),
                ])),
                ScriptedStep::Respond(ChatResponse::text("done")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &backend, input("try to complete both"))
            .await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert!(out.receipt.is_none(), "forbidden never sets a receipt");
        assert_eq!(out.tool_calls.len(), 2);
        assert_eq!(
            out.tool_calls[1].outcome,
            ToolCallOutcome::Ok,
            "the second call was not blocked by the guard"
        );
        assert_eq!(
            backend.seen.lock().unwrap().len(),
            2,
            "both calls reached the backend"
        );
    }

    /// A 503 outcome never surfaces its receipt (docs/specs/SLICE_018.md
    /// §2, the SLICE_006b §10 proposal rule extended).
    #[tokio::test]
    async fn a_503_outcome_never_surfaces_its_receipt() {
        let person = Uuid::new_v4();
        let task = Uuid::new_v4();
        let backend = FakeBackend {
            complete_task_outcome: Some(FakeCompleteOutcome::Completed),
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "complete_task",
                    json!({"person_id": person.to_string(), "task_id": task.to_string()}),
                )])),
                ScriptedStep::Fail(ProviderError::Timeout),
            ],
            Limits::default(),
        );
        let out = svc.run_turn(&ctx(), &backend, input("mark it done")).await;
        assert!(!out.outcome.is_reply());
        assert!(out.receipt.is_none());
    }

    /// The proposal slot is shared across `start_call` and `create_task`
    /// (docs/specs/SLICE_018.md §2): a `create_task` after a `start_call`
    /// proposal (and vice versa) is a structured rejection with no
    /// backend hit.
    #[tokio::test]
    async fn the_proposal_slot_is_shared_across_start_call_and_create_task() {
        let person = Uuid::new_v4();
        let backend = FakeBackend {
            phones: vec![Uuid::new_v4()],
            create_task_outcome: Some(FakeCreateOutcome::Proposed),
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![
                    call("c1", "start_call", json!({"person_id": person.to_string()})),
                    call(
                        "c2",
                        "create_task",
                        json!({"person_id": person.to_string(), "title": "Follow up"}),
                    ),
                ])),
                ScriptedStep::Respond(ChatResponse::text("done")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(
                &ctx(),
                &backend,
                input_with_offset("call and add a task", 0),
            )
            .await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert!(matches!(out.proposal, Some(TurnProposal::StartCall(_))));
        assert_eq!(out.tool_calls.len(), 2);
        assert_eq!(out.tool_calls[1].outcome, ToolCallOutcome::InvalidArguments);
        // Only the start_call backend hit landed.
        assert_eq!(backend.seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_task_proposal_lands_in_turn_output() {
        let person = Uuid::new_v4();
        let backend = FakeBackend {
            create_task_outcome: Some(FakeCreateOutcome::Proposed),
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "create_task",
                    json!({
                        "person_id": person.to_string(),
                        "title": "Call Grace",
                        "due_date": "2026-09-12",
                    }),
                )])),
                ScriptedStep::Respond(ChatResponse::text("Ready — confirm to add the task.")),
            ],
            Limits::default(),
        );
        let out = svc
            .run_turn(&ctx(), &backend, input_with_offset("add a task", -240))
            .await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        let proposal = match out.proposal.expect("proposal on the output") {
            TurnProposal::CreateTask(view) => view,
            other => panic!("expected a create_task proposal: {other:?}"),
        };
        assert_eq!(proposal.person.id, person);
        assert_eq!(proposal.title.as_str(), "Call Grace");
        // 2026-09-12 23:59:59 at UTC-4 == 2026-09-13T03:59:59Z.
        assert_eq!(
            proposal.due_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 13, 3, 59, 59).unwrap())
        );
    }

    #[tokio::test]
    async fn create_task_needs_clarification_sets_no_proposal() {
        let person = Uuid::new_v4();
        let backend = FakeBackend {
            create_task_outcome: Some(FakeCreateOutcome::NeedsClarification),
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "create_task",
                    json!({"person_id": person.to_string(), "title": "Call Grace", "assignee": "Bob"}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("which member?")),
            ],
            Limits::default(),
        );
        let out = svc.run_turn(&ctx(), &backend, input("add a task")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert!(out.proposal.is_none());
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::Ok);
    }

    /// docs/specs/SLICE_018.md §3: a `due_date` with no client offset
    /// fails closed as `invalid_arguments` — never a UTC guess.
    #[tokio::test]
    async fn create_task_with_a_due_date_and_no_offset_is_invalid_arguments() {
        let person = Uuid::new_v4();
        let backend = FakeBackend {
            create_task_outcome: Some(FakeCreateOutcome::Proposed),
            ..Default::default()
        };
        let (svc, _) = service(
            vec![
                ScriptedStep::Respond(ChatResponse::tool_calls(vec![call(
                    "c1",
                    "create_task",
                    json!({"person_id": person.to_string(), "title": "Call Grace", "due_date": "2026-09-12"}),
                )])),
                ScriptedStep::Respond(ChatResponse::text("noted")),
            ],
            Limits::default(),
        );
        // No offset supplied at all.
        let out = svc.run_turn(&ctx(), &backend, input("add a task")).await;
        assert_eq!(out.outcome, TurnOutcome::Completed);
        assert!(out.proposal.is_none());
        assert_eq!(out.tool_calls[0].outcome, ToolCallOutcome::InvalidArguments);
        // No backend hit: composition failed before propose_create_task ran.
        assert_eq!(backend.seen.lock().unwrap().len(), 0);
    }

    /// docs/specs/SLICE_018.md §3: due-instant composition — explicit
    /// time, end-of-day default, and the ±840 offset bounds.
    #[test]
    fn compose_due_at_matrix() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();
        let noon = chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        // Explicit time, positive offset (UTC+9): 2026-09-12T12:00:00+09:00
        // == 2026-09-12T03:00:00Z.
        assert_eq!(
            compose_due_at(Some(date), Some(noon), Some(540)).unwrap(),
            Some(Utc.with_ymd_and_hms(2026, 9, 12, 3, 0, 0).unwrap())
        );

        // No due_time: end of day, local (23:59:59), negative offset
        // (UTC-8): 2026-09-13T07:59:59Z.
        assert_eq!(
            compose_due_at(Some(date), None, Some(-480)).unwrap(),
            Some(Utc.with_ymd_and_hms(2026, 9, 13, 7, 59, 59).unwrap())
        );

        // Crossing to the PREVIOUS UTC day: 00:30 local at +600 (UTC+10) ==
        // 2026-09-11T14:30:00Z.
        let half_past_midnight = chrono::NaiveTime::from_hms_opt(0, 30, 0).unwrap();
        assert_eq!(
            compose_due_at(Some(date), Some(half_past_midnight), Some(600)).unwrap(),
            Some(Utc.with_ymd_and_hms(2026, 9, 11, 14, 30, 0).unwrap())
        );

        // The ±840 bounds (14h), exact instants, not just Ok:
        // 2026-09-12T12:00:00+14:00 == 2026-09-11T22:00:00Z.
        assert_eq!(
            compose_due_at(Some(date), Some(noon), Some(840)).unwrap(),
            Some(Utc.with_ymd_and_hms(2026, 9, 11, 22, 0, 0).unwrap())
        );
        // 2026-09-12T12:00:00-14:00 == 2026-09-13T02:00:00Z.
        assert_eq!(
            compose_due_at(Some(date), Some(noon), Some(-840)).unwrap(),
            Some(Utc.with_ymd_and_hms(2026, 9, 13, 2, 0, 0).unwrap())
        );

        // BEYOND_ENVELOPE, accepted (docs/specs/SLICE_018.md §3): a FIXED
        // offset carries no DST awareness at all — the same calendar date
        // (2026-03-08, the US DST-start Sunday) composes by pure
        // arithmetic regardless, proven here with two different offsets
        // on the same date and default end-of-day time.
        let dst_date = chrono::NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        assert_eq!(
            compose_due_at(Some(dst_date), None, Some(-480)).unwrap(),
            Some(Utc.with_ymd_and_hms(2026, 3, 9, 7, 59, 59).unwrap())
        );
        assert_eq!(
            compose_due_at(Some(dst_date), None, Some(600)).unwrap(),
            Some(Utc.with_ymd_and_hms(2026, 3, 8, 13, 59, 59).unwrap())
        );

        // No due_date at all: no offset needed, composes to None.
        assert_eq!(compose_due_at(None, None, None).unwrap(), None);

        // due_date with no offset: invalid_arguments, never a UTC guess.
        assert!(matches!(
            compose_due_at(Some(date), None, None),
            Err(ToolError::InvalidArguments(_))
        ));
    }
}
