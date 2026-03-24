//! LLM Provider trait definitions.

use std::pin::Pin;

use futures::Stream;

use crate::{
    error::ProviderError,
    types::{ChatRequest, ChatResponse, ChatStreamDelta, ModelInfo},
};

/// Convenience alias for the streaming response type returned by [`LlmProvider::chat_stream`].
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatStreamDelta, ProviderError>> + Send>>;

/// Unified interface for all LLM backends (OpenAI, Claude, Ollama, …).
///
/// Implementations are `Send + Sync` so they can be shared across async tasks.
///
/// Methods return `impl Future + Send` (desugared from `async fn`) to ensure
/// futures are `Send`, which is required when using the provider across threads.
pub trait LlmProvider: Send + Sync {
    /// Human-readable backend identifier, e.g. `"openai"`, `"claude"`, `"ollama"`.
    fn name(&self) -> &str;

    /// Non-streaming chat completions request.
    fn chat(
        &self,
        request: &ChatRequest,
    ) -> impl std::future::Future<Output = Result<ChatResponse, ProviderError>> + Send;

    /// Streaming chat completions request.
    ///
    /// Returns a pinned boxed `Stream` that yields one `ChatStreamDelta` per SSE chunk.
    fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> impl std::future::Future<Output = Result<ChatStream, ProviderError>> + Send;

    /// List models available on this backend.
    fn list_models(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<ModelInfo>, ProviderError>> + Send;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Message, Role};
    use futures::StreamExt;

    // Minimal stub that satisfies the trait to verify trait object construction.
    struct StubProvider;

    impl LlmProvider for StubProvider {
        fn name(&self) -> &str {
            "stub"
        }

        async fn chat(&self, _request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
            Err(ProviderError::InvalidRequest("stub".to_string()))
        }

        async fn chat_stream(&self, _request: &ChatRequest) -> Result<ChatStream, ProviderError> {
            let stream = futures::stream::empty();
            Ok(Box::pin(stream))
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
            Ok(vec![])
        }
    }

    #[test]
    fn provider_name_returns_identifier() {
        let p = StubProvider;
        assert_eq!(p.name(), "stub");
    }

    #[tokio::test]
    async fn chat_returns_error_for_stub() {
        let p = StubProvider;
        let req = ChatRequest {
            model: "m".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: Some("hi".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            stream: false,
        };
        let result = p.chat(&req).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn chat_stream_returns_empty_stream_for_stub() {
        let p = StubProvider;
        let req = ChatRequest {
            model: "m".to_string(),
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            stream: true,
        };
        let mut stream = p.chat_stream(&req).await.unwrap();
        let item = stream.next().await;
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn list_models_returns_empty_vec_for_stub() {
        let p = StubProvider;
        let models = p.list_models().await.unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn stub_provider_name_is_accessible_via_concrete_type() {
        // Note: LlmProvider uses async fn which makes it not dyn-compatible.
        // Callers use generics (`impl LlmProvider`) or concrete types.
        let p = StubProvider;
        assert_eq!(p.name(), "stub");
    }
}
