//! The bounded tool loop (docs/specs/SLICE_005.md §3, §4, §9).
//!
//! `OperatorService::run_turn` is infallible: every path — timeouts,
//! provider failures, backend failures — returns a `TurnOutput` so the
//! caller can always write the ledger row from the same value.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::Instrument;
use uuid::Uuid;

use crate::backend::{ToolBackend, ToolError};
use crate::context::OperatorContext;
use crate::provider::{
    ChatMessage, ChatRequest, ChatResponse, InferenceProvider, ProviderError, ToolCall, ToolChoice,
    Usage,
};
use crate::tools::{self, parse_invocation, tool_definitions, ArgumentError, ToolInvocation};
use crate::views::{PersonCard, ProposalView, StartCallProposalOutcome};
use crate::SYSTEM_PROMPT;

/// Total characters of replayed history (§4).
pub const MAX_HISTORY_CHARS: usize = 6000;
/// Reference cards returned per turn (§4, §14 item 4).
pub const MAX_REFERENCES: usize = 10;
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
    /// The turn's inserted `start_call` proposal, if any (at most one per
    /// turn, docs/specs/SLICE_006b.md §3). The wire renders the card from
    /// this object only, never from model prose.
    pub proposal: Option<ProposalView>,
    pub outcome: TurnOutcome,
    pub usage: Usage,
    pub model_call_count: u32,
}

impl std::fmt::Debug for TurnOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnOutput")
            .field("reply", &self.reply.as_ref().map(|r| r.chars().count()))
            .field("references", &self.references.people.len())
            .field("tool_calls", &self.tool_calls)
            .field("proposal", &self.proposal.as_ref().map(|p| p.proposal_id))
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
    /// At most one inserted proposal per turn (docs/specs/SLICE_006b.md
    /// §3); `NeedsNumberChoice`/`NoPhone` do not set this.
    proposal: Option<ProposalView>,
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
        _ => "unknown",
    }
}

fn clip_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
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
                ctx.now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
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

        // A 503 outcome never surfaces its proposal (docs/specs/
        // SLICE_006b.md §10: "never shown; expires inert").
        let proposal = if outcome.is_reply() {
            state.proposal
        } else {
            None
        };

        TurnOutput {
            reply,
            references: state.refs.finish(),
            tool_calls: state.tool_calls,
            proposal,
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
        let span = tracing::info_span!(
            "operator.tool_call",
            tool = name,
            outcome = tracing::field::Empty,
            duration_ms = tracing::field::Empty
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

        // One inserted proposal per turn (docs/specs/SLICE_006b.md §3):
        // a second `start_call` after a `Proposed` outcome is a structured
        // rejection with no backend hit.
        if matches!(invocation, ToolInvocation::StartCall { .. }) && state.proposal.is_some() {
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
                    "a call is already proposed in this turn; the user must confirm or dismiss it first",
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

        let result = dispatch(ctx, backend, &invocation)
            .instrument(span.clone())
            .await;
        let duration_ms = elapsed_ms(started);
        span.record("duration_ms", duration_ms);

        match result {
            Ok((value, bucket, cards, proposal)) => {
                span.record("outcome", "ok");
                state.tool_calls[slot] = ToolCallRecord {
                    name,
                    outcome: ToolCallOutcome::Ok,
                    duration_ms,
                    person_ids: cards.iter().map(|c| c.id).collect(),
                };
                if proposal.is_some() {
                    state.proposal = proposal;
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
                    content: tool_error_json("not_found", "no such person in your Organization"),
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

fn non_empty(content: Option<String>) -> Option<String> {
    content.filter(|c| !c.trim().is_empty())
}

fn elapsed_ms(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

/// Runs one validated invocation and returns the JSON the model sees, the
/// reference bucket, and the cards for `references` / `person_ids`.
async fn dispatch(
    ctx: &OperatorContext,
    backend: &dyn ToolBackend,
    invocation: &ToolInvocation,
) -> Result<(Value, RefBucket, Vec<PersonCard>, Option<ProposalView>), ToolError> {
    fn to_value<T: Serialize>(v: &T) -> Result<Value, ToolError> {
        serde_json::to_value(v).map_err(|e| ToolError::Backend(format!("serialize: {e}")))
    }

    match invocation {
        ToolInvocation::SearchPeople { query, limit } => {
            let result = backend.search_people(ctx, query, *limit).await?;
            let value = to_value(&result)?;
            Ok((value, RefBucket::Search, result.matches, None))
        }
        ToolInvocation::GetPerson { person_id } => {
            let detail = backend.get_person(ctx, *person_id).await?;
            let value = to_value(&detail)?;
            Ok((value, RefBucket::Primary, vec![detail.person], None))
        }
        ToolInvocation::GetToday { limit } => {
            let view = backend.get_today(ctx, *limit).await?;
            let value = to_value(&view)?;
            let cards = view.items.into_iter().map(|i| i.person).collect();
            Ok((value, RefBucket::Today, cards, None))
        }
        ToolInvocation::GetNextWorkItem => {
            let next = backend.get_next_work_item(ctx).await?;
            let value = to_value(&next)?;
            let cards = next.item.map(|i| i.person).into_iter().collect();
            Ok((value, RefBucket::Primary, cards, None))
        }
        ToolInvocation::ExplainPriority { person_id } => {
            let explanation = backend.explain_priority(ctx, *person_id).await?;
            let value = to_value(&explanation)?;
            let card = explanation.person().clone();
            Ok((value, RefBucket::Primary, vec![card], None))
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
                    Ok((value, RefBucket::Primary, vec![card], Some(view)))
                }
                StartCallProposalOutcome::NeedsNumberChoice { phones } => {
                    let value = json!({
                        "status": "choice_required",
                        "phones": to_value(&phones)?,
                    });
                    Ok((value, RefBucket::Primary, Vec::new(), None))
                }
                StartCallProposalOutcome::NoPhone => {
                    let value = json!({
                        "status": "no_phone",
                    });
                    Ok((value, RefBucket::Primary, Vec::new(), None))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            waiting_since: Utc.with_ymd_and_hms(2026, 8, 22, 11, 0, 0).unwrap(),
            last_contact_attempt: None,
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
                waiting_since: ctx.now,
                recommended_action: "call".to_string(),
                ordering_rule: ORDERING_RULE,
                ahead: Ahead {
                    high: 0,
                    normal: 0,
                    low: 0,
                },
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
        let asked = Uuid::new_v4();
        let today_ids: Vec<Uuid> = (0..15).map(|_| Uuid::new_v4()).collect();
        let search_ids = vec![Uuid::new_v4(), today_ids[3]];
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
        assert_eq!(ids[1], search_ids[0]);
        assert_eq!(ids[2], search_ids[1]);
        // today_ids[3] already appeared via search_people, so it is
        // deduplicated out of the Today tail.
        let expected_tail: Vec<Uuid> = today_ids
            .iter()
            .copied()
            .filter(|id| *id != today_ids[3])
            .take(7)
            .collect();
        assert_eq!(&ids[3..], &expected_tail[..]);
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
        let proposal = out.proposal.expect("proposal on the output");
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
}
