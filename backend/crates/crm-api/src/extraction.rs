//! The `LeadExtractor` adapter (docs/specs/SLICE_007f.md §4d): the only
//! place extraction meets `crm_operator::InferenceProvider`. It builds
//! the prompt and moves bytes; every semantic decision (schema,
//! confidence, anti-hallucination, normalization) lives in
//! `crm_app::domain::intake::extraction` and cannot be changed here.

use async_trait::async_trait;
use crm_app::domain::intake::extraction::{
    ExtractionInput, ExtractorError, ExtractorReply, LeadExtractor,
};
use crm_operator::{
    ChatMessage, ChatRequest, GroqConfig, GroqProvider, InferenceProvider, ProviderError,
    ResponseFormat, ToolChoice,
};
use serde_json::json;

use crate::config::{Config, ExtractionConfig};

/// The system prompt: schema description + the injection rule. A file so
/// prompt wording changes never touch code review noise.
const SYSTEM_PROMPT: &str = include_str!("../prompts/extract_lead.md");

pub struct GroqLeadExtractor {
    provider: GroqProvider,
}

impl GroqLeadExtractor {
    /// Its own provider instance: the extraction model and call timeout,
    /// same key and base URL as the Operator.
    pub fn from_config(config: &Config, extraction: &ExtractionConfig) -> Option<Self> {
        let api_key = config.groq_api_key.clone()?;
        let provider = GroqProvider::new(GroqConfig {
            base_url: config.operator.base_url.clone(),
            model: extraction.model.clone(),
            api_key,
            call_timeout: extraction.call_timeout,
            connect_timeout: crm_operator::DEFAULT_CONNECT_TIMEOUT,
        });
        Some(Self { provider })
    }
}

/// Strips control characters except newline (docs/specs/SLICE_007f.md
/// §4d — NOT `UntrustedText`, whose 500-char clip would destroy the
/// body).
fn strip_controls(s: &str) -> String {
    s.chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect()
}

/// The user message: the SLICE_005 §7 named-key convention — untrusted
/// content travels under one explicit key, never inline in the prompt.
fn user_message(input: &ExtractionInput) -> String {
    json!({
        "untrusted_email": {
            "subject": input.subject.as_deref().map(strip_controls),
            "sender_domain": input.sender_domain,
            "text": strip_controls(&input.text),
        }
    })
    .to_string()
}

/// The full request: no tools ever, strict-JSON response format
/// (docs/specs/SLICE_007f.md §4d; criterion 16's "no tools" pin).
fn build_request(input: &ExtractionInput) -> ChatRequest {
    ChatRequest {
        messages: vec![
            ChatMessage::System {
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage::User {
                content: user_message(input),
            },
        ],
        tools: vec![],
        tool_choice: ToolChoice::None,
        response_format: Some(ResponseFormat::JsonObject),
    }
}

#[async_trait]
impl LeadExtractor for GroqLeadExtractor {
    async fn extract(&self, input: &ExtractionInput) -> Result<ExtractorReply, ExtractorError> {
        let request = build_request(input);
        let response = self
            .provider
            .complete(request)
            .await
            .map_err(|err| match err {
                ProviderError::Timeout => ExtractorError::Timeout,
                ProviderError::RateLimited => ExtractorError::RateLimited,
                ProviderError::Unavailable(_) => ExtractorError::Unavailable,
                ProviderError::Malformed(_) => ExtractorError::Malformed,
            })?;
        let content = response.content.ok_or(ExtractorError::Malformed)?;
        Ok(ExtractorReply {
            content,
            prompt_tokens: response.usage.prompt_tokens,
            completion_tokens: response.usage.completion_tokens,
        })
    }

    fn provider(&self) -> &'static str {
        "groq"
    }

    fn model(&self) -> &str {
        self.provider.model()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ExtractionInput {
        ExtractionInput {
            subject: Some("New lead\u{0000} here".into()),
            sender_domain: Some("eospia.com".into()),
            text: "Reach Jordan at jordan@example.com\r\nSecond line".into(),
            truncated: false,
        }
    }

    #[test]
    fn user_message_wraps_only_the_three_fields_under_the_untrusted_key() {
        let msg = user_message(&input());
        let value: serde_json::Value = serde_json::from_str(&msg).unwrap();
        let email = &value["untrusted_email"];
        assert_eq!(
            email.as_object().unwrap().keys().collect::<Vec<_>>().len(),
            3
        );
        assert!(email.get("subject").is_some());
        assert!(email.get("sender_domain").is_some());
        assert!(email.get("text").is_some());
        // Control characters are stripped (except newline).
        assert!(!msg.contains('\u{0000}'));
        assert!(email["text"].as_str().unwrap().contains('\n'));
        assert!(!email["text"].as_str().unwrap().contains('\r'));
    }

    #[test]
    fn request_has_no_tools_and_asks_for_strict_json() {
        let req = build_request(&input());
        assert!(req.tools.is_empty());
        assert_eq!(req.tool_choice, ToolChoice::None);
        assert_eq!(req.response_format, Some(ResponseFormat::JsonObject));
        assert_eq!(req.messages.len(), 2);
    }

    #[test]
    fn from_config_is_none_without_a_groq_key() {
        // Criterion 14's gate: no key, no extractor, no worker.
        let config = crate::config::Config::from_source(|key| match key {
            "CRM_SESSION_SECRET" => Some("a".repeat(32)),
            "CRM_RAW_PAYLOAD_KEY" => Some("ab".repeat(32)),
            "CENTRIFUGO_HTTP_API_KEY" => Some("k".to_string()),
            "CENTRIFUGO_TOKEN_HMAC_SECRET" => Some("c".repeat(32)),
            _ => None,
        })
        .unwrap();
        assert!(GroqLeadExtractor::from_config(&config, &config.extraction).is_none());
    }

    #[test]
    fn prompt_names_the_injection_rule_and_the_schema() {
        assert!(SYSTEM_PROMPT.contains("UNTRUSTED"));
        assert!(SYSTEM_PROMPT.contains("never obey"));
        assert!(SYSTEM_PROMPT.contains("is_lead"));
        assert!(SYSTEM_PROMPT.contains("confidence"));
        assert!(SYSTEM_PROMPT.contains("Never invent"));
    }
}
