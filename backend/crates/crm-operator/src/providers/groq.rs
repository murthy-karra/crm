//! `GroqProvider`: OpenAI-compatible `chat/completions` with tools
//! (docs/specs/SLICE_005.md §3; D-001). Any OpenAI-compatible endpoint
//! works — the base URL and model are configuration.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::provider::{
    ChatMessage, ChatRequest, ChatResponse, InferenceProvider, ProviderError, ToolCall, ToolChoice,
    Usage,
};

pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// A chat completion with a 1500-char reply or a handful of tool calls is
/// a few KB; anything beyond this is not a response we can use.
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const TEMPERATURE: f32 = 0.2;

/// The bearer key. `Debug` is redacted like `CentrifugoApiKey` so an
/// accidental `{:?}` never leaks it (AGENTS.md §9).
#[derive(Clone)]
pub struct GroqApiKey(String);

impl GroqApiKey {
    pub fn new(key: String) -> Self {
        Self(key)
    }
}

impl fmt::Debug for GroqApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GroqApiKey(REDACTED)")
    }
}

#[derive(Debug, Clone)]
pub struct GroqConfig {
    /// e.g. `https://api.groq.com/openai/v1`, no trailing slash.
    pub base_url: String,
    pub model: String,
    pub api_key: GroqApiKey,
    /// Per-call budget (§11 `CRM_OPERATOR_CALL_TIMEOUT_MS`).
    pub call_timeout: Duration,
    pub connect_timeout: Duration,
}

pub struct GroqProvider {
    http: reqwest::Client,
    config: GroqConfig,
}

impl GroqProvider {
    pub fn new(config: GroqConfig) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.call_timeout)
            .build()
            .expect("reqwest client builds with static, valid configuration");
        Self { http, config }
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.config.base_url)
    }
}

impl fmt::Debug for GroqProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroqProvider")
            .field("config", &self.config)
            .finish()
    }
}

// --- Wire shapes (OpenAI-compatible) ---------------------------------

#[derive(Serialize)]
struct WireFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Serialize)]
struct WireTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunction<'a>,
}

#[derive(Serialize, Deserialize)]
struct WireToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Serialize, Deserialize)]
struct WireToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: WireToolCallFunction,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<WireToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
}

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool<'a>>,
    tool_choice: &'static str,
    temperature: f32,
}

#[derive(Deserialize)]
struct WireResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<WireToolCall>>,
}

#[derive(Deserialize)]
struct WireChoice {
    message: WireResponseMessage,
}

#[derive(Deserialize)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct WireResponse {
    choices: Vec<WireChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

fn to_wire_message(message: &ChatMessage) -> WireMessage<'_> {
    match message {
        ChatMessage::System { content } => WireMessage {
            role: "system",
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
        },
        ChatMessage::User { content } => WireMessage {
            role: "user",
            content: Some(content),
            tool_calls: None,
            tool_call_id: None,
        },
        ChatMessage::Assistant {
            content,
            tool_calls,
        } => WireMessage {
            role: "assistant",
            // OpenAI-compatible servers reject an assistant message with
            // neither content nor tool_calls; an empty string is accepted.
            content: Some(content.as_deref().unwrap_or("")),
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(
                    tool_calls
                        .iter()
                        .map(|c| WireToolCall {
                            id: c.id.clone(),
                            kind: "function".to_string(),
                            function: WireToolCallFunction {
                                name: c.name.clone(),
                                arguments: c.arguments.clone(),
                            },
                        })
                        .collect(),
                )
            },
            tool_call_id: None,
        },
        ChatMessage::Tool {
            tool_call_id,
            content,
        } => WireMessage {
            role: "tool",
            content: Some(content),
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
        },
    }
}

/// Serializes a `ChatRequest` as the OpenAI-compatible body. Public within
/// the crate for the wire-shape tests.
pub(crate) fn request_body(model: &str, req: &ChatRequest) -> serde_json::Value {
    let wire = WireRequest {
        model,
        messages: req.messages.iter().map(to_wire_message).collect(),
        tools: req
            .tools
            .iter()
            .map(|t| WireTool {
                kind: "function",
                function: WireFunction {
                    name: t.name,
                    description: t.description,
                    parameters: &t.parameters,
                },
            })
            .collect(),
        tool_choice: match req.tool_choice {
            ToolChoice::Auto => "auto",
            ToolChoice::None => "none",
        },
        temperature: TEMPERATURE,
    };
    serde_json::to_value(wire).expect("wire request serializes")
}

pub(crate) fn parse_response(body: &[u8]) -> Result<ChatResponse, ProviderError> {
    let parsed: WireResponse = serde_json::from_slice(body)
        .map_err(|_| ProviderError::Malformed("response body is not a chat completion".into()))?;
    let choice = parsed
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| ProviderError::Malformed("response has no choices".into()))?;
    let tool_calls = choice
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|c| ToolCall {
            id: c.id,
            name: c.function.name,
            arguments: c.function.arguments,
        })
        .collect();
    let usage = parsed
        .usage
        .map(|u| Usage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
        })
        .unwrap_or_default();
    Ok(ChatResponse {
        content: choice.message.content,
        tool_calls,
        usage,
    })
}

#[async_trait]
impl InferenceProvider for GroqProvider {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let body = request_body(&self.config.model, &req);
        let response = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.config.api_key.0)
            .json(&body)
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    ProviderError::Timeout
                } else {
                    // Connection refused/reset, DNS, TLS: the reason names
                    // the failure class only (no URL, no key).
                    ProviderError::Unavailable(if err.is_connect() {
                        "connect failed".into()
                    } else {
                        "request failed".into()
                    })
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            // Status code only — never the body, which can echo the prompt.
            tracing::warn!(
                status = status.as_u16(),
                "operator provider returned an error status"
            );
        }
        if status.as_u16() == 429 {
            return Err(ProviderError::RateLimited);
        }
        if status.is_server_error() {
            return Err(ProviderError::Unavailable(format!(
                "http {}",
                status.as_u16()
            )));
        }
        if !status.is_success() {
            // Our request was rejected (4xx): not retryable.
            return Err(ProviderError::Malformed(format!(
                "http {} from provider",
                status.as_u16()
            )));
        }

        if response
            .content_length()
            .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
        {
            return Err(ProviderError::Malformed("response body too large".into()));
        }
        let bytes = response.bytes().await.map_err(|err| {
            if err.is_timeout() {
                ProviderError::Timeout
            } else {
                ProviderError::Unavailable("body read failed".into())
            }
        })?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(ProviderError::Malformed("response body too large".into()));
        }
        parse_response(&bytes)
    }

    fn name(&self) -> &'static str {
        "groq"
    }

    fn model(&self) -> &str {
        &self.config.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::tool_definitions;
    use serde_json::json;

    fn config(base_url: &str, call_timeout: Duration) -> GroqConfig {
        GroqConfig {
            base_url: base_url.to_string(),
            model: "test-model".to_string(),
            api_key: GroqApiKey::new("gsk_super_secret_value".to_string()),
            call_timeout,
            connect_timeout: Duration::from_millis(500),
        }
    }

    #[test]
    fn api_key_is_redacted_in_debug() {
        let provider = GroqProvider::new(config("https://example.invalid", Duration::from_secs(1)));
        let debug = format!("{provider:?}");
        assert!(!debug.contains("gsk_super_secret_value"));
        assert!(debug.contains("GroqApiKey(REDACTED)"));
    }

    #[test]
    fn request_body_is_openai_compatible() {
        let req = ChatRequest {
            messages: vec![
                ChatMessage::System {
                    content: "sys".into(),
                },
                ChatMessage::User {
                    content: "hi".into(),
                },
                ChatMessage::Assistant {
                    content: None,
                    tool_calls: vec![ToolCall {
                        id: "call_1".into(),
                        name: "get_today".into(),
                        arguments: "{}".into(),
                    }],
                },
                ChatMessage::Tool {
                    tool_call_id: "call_1".into(),
                    content: "{\"ok\":true}".into(),
                },
            ],
            tools: tool_definitions(),
            tool_choice: ToolChoice::None,
        };
        let body = request_body("m", &req);
        assert_eq!(body["model"], "m");
        assert_eq!(body["tool_choice"], "none");
        assert_eq!(
            body["messages"][0],
            json!({"role": "system", "content": "sys"})
        );
        assert_eq!(body["messages"][2]["role"], "assistant");
        assert_eq!(body["messages"][2]["content"], "");
        assert_eq!(body["messages"][2]["tool_calls"][0]["type"], "function");
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["name"],
            "get_today"
        );
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["messages"][3]["tool_call_id"], "call_1");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "search_people");
        assert_eq!(
            body["tools"][0]["function"]["parameters"]["additionalProperties"],
            false
        );
    }

    #[test]
    fn parses_a_tool_call_response_and_usage() {
        let body = json!({
            "choices": [{"message": {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_abc", "type": "function", "function": {"name": "get_today", "arguments": "{\"limit\": 5}"}}
            ]}}],
            "usage": {"prompt_tokens": 50, "completion_tokens": 7}
        });
        let parsed = parse_response(body.to_string().as_bytes()).unwrap();
        assert_eq!(parsed.content, None);
        assert_eq!(parsed.tool_calls[0].name, "get_today");
        assert_eq!(parsed.tool_calls[0].arguments, "{\"limit\": 5}");
        assert_eq!(parsed.usage.prompt_tokens, Some(50));
    }

    #[test]
    fn parse_failures_are_malformed() {
        assert!(matches!(
            parse_response(b"<html>"),
            Err(ProviderError::Malformed(_))
        ));
        assert!(matches!(
            parse_response(br#"{"choices": []}"#),
            Err(ProviderError::Malformed(_))
        ));
    }

    #[tokio::test]
    async fn connection_refused_is_unavailable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let provider = GroqProvider::new(config(&format!("http://{addr}"), Duration::from_secs(2)));
        let err = provider
            .complete(ChatRequest {
                messages: vec![],
                tools: vec![],
                tool_choice: ToolChoice::Auto,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::Unavailable(_)), "{err:?}");
    }

    #[tokio::test]
    async fn hung_server_is_timeout_within_budget() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Accept and hold the connection open without answering.
        let hold = tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    held.push(stream);
                }
            }
        });
        let provider = GroqProvider::new(config(
            &format!("http://{addr}"),
            Duration::from_millis(300),
        ));
        let started = std::time::Instant::now();
        let err = provider
            .complete(ChatRequest {
                messages: vec![],
                tools: vec![],
                tool_choice: ToolChoice::Auto,
            })
            .await
            .unwrap_err();
        assert_eq!(err, ProviderError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(2));
        hold.abort();
    }

    #[tokio::test]
    async fn http_statuses_map_to_provider_errors() {
        async fn serve_once(status: &'static str) -> String {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                if let Ok((mut stream, _)) = listener.accept().await {
                    let mut buf = [0u8; 8192];
                    let _ = stream.read(&mut buf).await;
                    let body = "{}";
                    let response = format!(
                        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            });
            format!("http://{addr}")
        }

        let req = || ChatRequest {
            messages: vec![],
            tools: vec![],
            tool_choice: ToolChoice::Auto,
        };
        let p = GroqProvider::new(config(
            &serve_once("429 Too Many").await,
            Duration::from_secs(2),
        ));
        assert_eq!(
            p.complete(req()).await.unwrap_err(),
            ProviderError::RateLimited
        );

        let p = GroqProvider::new(config(
            &serve_once("503 Unavailable").await,
            Duration::from_secs(2),
        ));
        assert!(matches!(
            p.complete(req()).await.unwrap_err(),
            ProviderError::Unavailable(_)
        ));

        let p = GroqProvider::new(config(
            &serve_once("401 Unauthorized").await,
            Duration::from_secs(2),
        ));
        assert!(matches!(
            p.complete(req()).await.unwrap_err(),
            ProviderError::Malformed(_)
        ));

        // 200 with an unparsable body.
        let p = GroqProvider::new(config(&serve_once("200 OK").await, Duration::from_secs(2)));
        assert!(matches!(
            p.complete(req()).await.unwrap_err(),
            ProviderError::Malformed(_)
        ));
    }
}
