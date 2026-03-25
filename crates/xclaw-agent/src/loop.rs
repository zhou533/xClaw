//! Agent loop: the core execution engine that drives conversations.
//!
//! Receives user input, loads session context, injects memory,
//! builds prompts, calls the LLM, dispatches tool/skill calls,
//! and returns the final response.

use xclaw_core::error::XClawError;
use xclaw_provider::traits::LlmProvider;

use crate::prompt::build_chat_request;
use crate::traits::{AgentLoop, AgentResponse, UserInput};

/// Minimal agent that sends a single user message to an LLM provider
/// and returns the text response. No tool calls, no memory, no looping.
pub struct SimpleAgent<P: LlmProvider> {
    provider: P,
    model: String,
}

impl<P: LlmProvider> SimpleAgent<P> {
    pub fn new(provider: P, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
        }
    }
}

impl<P: LlmProvider> AgentLoop for SimpleAgent<P> {
    async fn process(&self, input: UserInput) -> Result<AgentResponse, XClawError> {
        let request = build_chat_request(&self.model, &input.content);

        let response = self
            .provider
            .chat(&request)
            .await
            .map_err(|e| XClawError::Agent(e.to_string()))?;

        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| XClawError::Agent("empty response from provider".to_string()))?;

        Ok(AgentResponse {
            content,
            tool_calls_count: 0,
        })
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use xclaw_core::types::SessionId;
    use xclaw_provider::error::ProviderError;
    use xclaw_provider::traits::ChatStream;
    use xclaw_provider::types::{
        ChatRequest, ChatResponse, Choice, FinishReason, Message, ModelInfo, Role,
    };

    // ── Stub provider ───────────────────────────────────────────────────

    struct OkProvider {
        reply: String,
    }

    impl OkProvider {
        fn new(reply: &str) -> Self {
            Self {
                reply: reply.to_string(),
            }
        }
    }

    impl LlmProvider for OkProvider {
        fn name(&self) -> &str {
            "ok-stub"
        }

        async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            Ok(ChatResponse {
                id: "resp-1".to_string(),
                model: "stub".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message {
                        role: Role::Assistant,
                        content: Some(self.reply.clone()),
                        tool_calls: vec![],
                        tool_call_id: None,
                    },
                    finish_reason: Some(FinishReason::Stop),
                }],
                usage: None,
            })
        }

        async fn chat_stream(&self, _req: &ChatRequest) -> Result<ChatStream, ProviderError> {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
            Ok(vec![])
        }
    }

    struct EmptyChoicesProvider;

    impl LlmProvider for EmptyChoicesProvider {
        fn name(&self) -> &str {
            "empty-stub"
        }

        async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            Ok(ChatResponse {
                id: "resp-2".to_string(),
                model: "stub".to_string(),
                choices: vec![],
                usage: None,
            })
        }

        async fn chat_stream(&self, _req: &ChatRequest) -> Result<ChatStream, ProviderError> {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
            Ok(vec![])
        }
    }

    struct ErrorProvider;

    impl LlmProvider for ErrorProvider {
        fn name(&self) -> &str {
            "error-stub"
        }

        async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            Err(ProviderError::Auth("invalid key".to_string()))
        }

        async fn chat_stream(&self, _req: &ChatRequest) -> Result<ChatStream, ProviderError> {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
            Ok(vec![])
        }
    }

    // ── Tests ───────────────────────────────────────────────────────────

    fn make_input(content: &str) -> UserInput {
        UserInput {
            session_id: SessionId::new("test-session"),
            content: content.to_string(),
        }
    }

    #[tokio::test]
    async fn returns_provider_reply() {
        let agent = SimpleAgent::new(OkProvider::new("Hello!"), "gpt-4o");
        let resp = agent.process(make_input("hi")).await.unwrap();
        assert_eq!(resp.content, "Hello!");
        assert_eq!(resp.tool_calls_count, 0);
    }

    #[tokio::test]
    async fn returns_unicode_reply() {
        let agent = SimpleAgent::new(OkProvider::new("你好世界"), "gpt-4o");
        let resp = agent.process(make_input("hello")).await.unwrap();
        assert_eq!(resp.content, "你好世界");
    }

    #[tokio::test]
    async fn errors_on_empty_choices() {
        let agent = SimpleAgent::new(EmptyChoicesProvider, "gpt-4o");
        let result = agent.process(make_input("hi")).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("empty response"), "error: {err}");
    }

    #[tokio::test]
    async fn maps_provider_error_to_xclaw_error() {
        let agent = SimpleAgent::new(ErrorProvider, "gpt-4o");
        let result = agent.process(make_input("hi")).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid key"), "error: {err}");
    }
}
