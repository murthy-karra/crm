//! `ScriptedProvider`: a queue of canned steps for tests (docs/specs/
//! SLICE_005.md §3, §14 item 8). Records every request it receives so a
//! test can assert what the prompt contained (e.g. that an inquiry message
//! reached the model only under `untrusted_text`). Behind the
//! `test-support` feature for dependent crates.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use crate::provider::{ChatRequest, ChatResponse, InferenceProvider, ProviderError};

#[derive(Debug, Clone)]
pub enum ScriptedStep {
    Respond(ChatResponse),
    Fail(ProviderError),
    /// Sleep, then respond — for turn-timeout and concurrency tests. A
    /// sleep longer than the turn budget is simply cut off by the loop's
    /// deadline.
    SleepThenRespond(Duration, ChatResponse),
}

#[derive(Clone)]
pub struct ScriptedProvider {
    steps: Arc<Mutex<std::collections::VecDeque<ScriptedStep>>>,
    requests: Arc<Mutex<Vec<ChatRequest>>>,
    model: String,
}

impl ScriptedProvider {
    pub fn new(steps: Vec<ScriptedStep>) -> Self {
        Self {
            steps: Arc::new(Mutex::new(steps.into())),
            requests: Arc::new(Mutex::new(Vec::new())),
            model: "scripted-model".to_string(),
        }
    }

    /// Every request received so far, in order (cloned).
    pub fn requests(&self) -> Vec<ChatRequest> {
        self.requests
            .lock()
            .expect("scripted provider lock")
            .clone()
    }

    pub fn remaining_steps(&self) -> usize {
        self.steps.lock().expect("scripted provider lock").len()
    }
}

#[async_trait]
impl InferenceProvider for ScriptedProvider {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.requests
            .lock()
            .expect("scripted provider lock")
            .push(req);
        let step = self
            .steps
            .lock()
            .expect("scripted provider lock")
            .pop_front();
        match step {
            Some(ScriptedStep::Respond(response)) => Ok(response),
            Some(ScriptedStep::Fail(err)) => Err(err),
            Some(ScriptedStep::SleepThenRespond(delay, response)) => {
                tokio::time::sleep(delay).await;
                Ok(response)
            }
            None => Err(ProviderError::Malformed("script exhausted".to_string())),
        }
    }

    fn name(&self) -> &'static str {
        "scripted"
    }

    fn model(&self) -> &str {
        &self.model
    }
}
