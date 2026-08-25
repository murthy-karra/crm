//! The provider-neutral inference interface (docs/specs/SLICE_005.md §3;
//! D-001). Shapes follow the OpenAI-compatible chat-completions model
//! closely enough that `GroqProvider` is a thin mapping, but nothing here
//! names a vendor.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// One tool the model may call: an OpenAI-style function definition with
/// a JSON-schema `parameters` object (`additionalProperties: false`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-assigned id echoed back on the tool result message.
    pub id: String,
    pub name: String,
    /// Raw JSON text exactly as the model produced it; parsed and validated
    /// by the loop. Never logged (D-029).
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    None,
}

/// Structured-output request (docs/specs/SLICE_007f.md §4d — a declared
/// additive extension, AGENTS.md §11). `JsonObject` maps to the
/// OpenAI-compatible `response_format: {"type": "json_object"}`. The
/// Operator's callers pass `None`; `ScriptedProvider` ignores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFormat {
    JsonObject,
}

/// `Debug` is redacted: messages carry user and tool text (D-029).
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: ToolChoice,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
}

impl std::fmt::Debug for ChatRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatRequest")
            .field(
                "messages",
                &format_args!("[{} redacted]", self.messages.len()),
            )
            .field("tools", &self.tools.len())
            .field("tool_choice", &self.tool_choice)
            .field("response_format", &self.response_format)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

impl Usage {
    pub fn add(&mut self, other: Usage) {
        self.prompt_tokens = match (self.prompt_tokens, other.prompt_tokens) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0).saturating_add(b.unwrap_or(0))),
        };
        self.completion_tokens = match (self.completion_tokens, other.completion_tokens) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0).saturating_add(b.unwrap_or(0))),
        };
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub usage: Usage,
}

impl ChatResponse {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            tool_calls: Vec::new(),
            usage: Usage::default(),
        }
    }

    pub fn tool_calls(calls: Vec<ToolCall>) -> Self {
        Self {
            content: None,
            tool_calls: calls,
            usage: Usage::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// A single call exceeded its per-call budget.
    #[error("provider call timed out")]
    Timeout,
    /// HTTP 429. Never retried.
    #[error("provider rate limited")]
    RateLimited,
    /// 5xx or a connection failure. Retried once if budget remains.
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    /// The body could not be parsed into a `ChatResponse`. The string is a
    /// reason, never body text.
    #[error("provider response malformed: {0}")]
    Malformed(String),
}

#[async_trait]
pub trait InferenceProvider: Send + Sync {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError>;
    /// `'groq'`, `'scripted'` — the ledger's `provider` column.
    fn name(&self) -> &'static str;
    /// The ledger's `model` column.
    fn model(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_add_keeps_none_only_when_both_absent() {
        let mut u = Usage::default();
        u.add(Usage::default());
        assert_eq!(u, Usage::default());
        u.add(Usage {
            prompt_tokens: Some(10),
            completion_tokens: None,
        });
        u.add(Usage {
            prompt_tokens: Some(5),
            completion_tokens: Some(3),
        });
        assert_eq!(u.prompt_tokens, Some(15));
        assert_eq!(u.completion_tokens, Some(3));
    }

    /// A hostile endpoint controls these numbers; `run_turn` must stay
    /// infallible (no overflow panic in debug builds).
    #[test]
    fn usage_add_saturates() {
        let mut u = Usage {
            prompt_tokens: Some(u32::MAX),
            completion_tokens: Some(1),
        };
        u.add(Usage {
            prompt_tokens: Some(u32::MAX),
            completion_tokens: Some(u32::MAX),
        });
        assert_eq!(u.prompt_tokens, Some(u32::MAX));
        assert_eq!(u.completion_tokens, Some(u32::MAX));
    }
}
