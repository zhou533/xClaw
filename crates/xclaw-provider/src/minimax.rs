//! MiniMax LLM provider.
//!
//! MiniMax exposes an OpenAI-compatible chat completions API at
//! `https://api.minimax.io/v1`, so this provider delegates to [`OpenAiProvider`]
//! internally and only overrides behaviour that differs (provider name,
//! hardcoded model list, etc.).

// ─── Tests ───────────────────────────────────────────────────────────────────
// Written FIRST (RED), implementation follows below.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatRequest, FinishReason, Message, Role};
    use futures::StreamExt;
    use mockito::Server;

    fn make_request(stream: bool) -> ChatRequest {
        ChatRequest {
            model: "MiniMax-M2".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: Some("Hello".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            stream,
        }
    }

    // ── MiniMaxProvider::new ────────────────────────────────────────────────

    #[test]
    fn provider_name_is_minimax() {
        let p = MiniMaxProvider::new("key", None).unwrap();
        assert_eq!(p.name(), "minimax");
    }

    #[test]
    fn provider_uses_default_base_url() {
        let p = MiniMaxProvider::new("key", None).unwrap();
        assert!(p.base_url().contains("api.minimax.io"));
    }

    #[test]
    fn provider_accepts_custom_base_url() {
        let p = MiniMaxProvider::new("key", Some("https://my-proxy.example.com/v1")).unwrap();
        assert_eq!(p.base_url(), "https://my-proxy.example.com/v1");
    }

    #[test]
    fn provider_rejects_empty_api_key() {
        let err = MiniMaxProvider::new("", None).unwrap_err();
        assert!(
            matches!(err, crate::error::ProviderError::InvalidRequest(_)),
            "expected InvalidRequest, got: {err:?}"
        );
    }

    #[test]
    fn provider_rejects_whitespace_only_api_key() {
        let err = MiniMaxProvider::new("   ", None).unwrap_err();
        assert!(matches!(
            err,
            crate::error::ProviderError::InvalidRequest(_)
        ));
    }

    // ── list_models() ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_models_returns_hardcoded_minimax_models() {
        let p = MiniMaxProvider::new("key", None).unwrap();
        let models = p.list_models().await.unwrap();

        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"MiniMax-M1"));
        assert!(ids.contains(&"MiniMax-M2"));
        assert!(ids.contains(&"MiniMax-M2.1"));

        for m in &models {
            assert_eq!(m.owned_by, "minimax");
        }
    }

    // ── chat() — happy path ────────────────────────────────────────────────

    #[tokio::test]
    async fn chat_returns_response_on_success() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "chatcmpl-mm-1",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "MiniMax-M2",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hi!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
        }"#;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = MiniMaxProvider::new("test-key", Some(&server.url())).unwrap();
        let resp = provider.chat(&make_request(false)).await.unwrap();

        assert_eq!(resp.id, "chatcmpl-mm-1");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content, Some("Hi!".to_string()));
        assert!(matches!(
            resp.choices[0].finish_reason,
            Some(FinishReason::Stop)
        ));
        let usage = resp.usage.unwrap();
        assert_eq!(usage.total_tokens, 8);
    }

    #[tokio::test]
    async fn chat_sends_authorization_header() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "chatcmpl-mm-2",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "MiniMax-M2",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        }"#;
        let _mock = server
            .mock("POST", "/chat/completions")
            .match_header("authorization", "Bearer test-key-mm")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = MiniMaxProvider::new("test-key-mm", Some(&server.url())).unwrap();
        let resp = provider.chat(&make_request(false)).await;
        assert!(resp.is_ok(), "expected ok, got: {:?}", resp.err());
    }

    // ── chat() — error mapping ─────────────────────────────────────────────

    #[tokio::test]
    async fn chat_maps_401_to_auth_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(401)
            .with_body(r#"{"error":{"message":"Invalid API key"}}"#)
            .create_async()
            .await;

        let provider = MiniMaxProvider::new("bad-key", Some(&server.url())).unwrap();
        let err = provider.chat(&make_request(false)).await.unwrap_err();
        assert!(
            matches!(err, crate::error::ProviderError::Auth(_)),
            "expected Auth, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn chat_maps_429_to_rate_limit_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(429)
            .with_body(r#"{"error":{"message":"Rate limit exceeded"}}"#)
            .create_async()
            .await;

        let provider = MiniMaxProvider::new("key", Some(&server.url())).unwrap();
        let err = provider.chat(&make_request(false)).await.unwrap_err();
        assert!(
            matches!(err, crate::error::ProviderError::RateLimit { .. }),
            "expected RateLimit, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn chat_maps_500_to_server_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let provider = MiniMaxProvider::new("key", Some(&server.url())).unwrap();
        let err = provider.chat(&make_request(false)).await.unwrap_err();
        assert!(
            matches!(
                err,
                crate::error::ProviderError::ServerError { status: 500, .. }
            ),
            "expected ServerError(500), got: {err:?}"
        );
    }

    // ── chat() — tool_calls in response ────────────────────────────────────

    #[tokio::test]
    async fn chat_returns_tool_calls_in_response() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "chatcmpl-mm-tc",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "MiniMax-M2",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": {"name": "get_weather", "arguments": "{\"city\":\"Beijing\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}
        }"#;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = MiniMaxProvider::new("key", Some(&server.url())).unwrap();
        let resp = provider.chat(&make_request(false)).await.unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.content.is_none());
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id, "call_abc");
        assert_eq!(msg.tool_calls[0].function.name, "get_weather");
    }

    // ── chat_stream() ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn chat_stream_yields_content_deltas() {
        let mut server = Server::new_async().await;
        let sse_body = concat!(
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"MiniMax-M2\",",
            "\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"MiniMax-M2\",",
            "\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"MiniMax-M2\",",
            "\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        let provider = MiniMaxProvider::new("key", Some(&server.url())).unwrap();
        let mut stream = provider.chat_stream(&make_request(true)).await.unwrap();

        let mut contents: Vec<String> = vec![];
        let mut finish_reasons = vec![];

        while let Some(result) = stream.next().await {
            let delta = result.unwrap();
            for choice in &delta.choices {
                if let Some(c) = &choice.delta.content {
                    contents.push(c.clone());
                }
                if let Some(fr) = &choice.finish_reason {
                    finish_reasons.push(fr.clone());
                }
            }
        }

        assert!(
            contents.iter().any(|c| c == "Hello"),
            "expected 'Hello' in stream"
        );
        assert_eq!(finish_reasons.len(), 1);
        assert!(matches!(finish_reasons[0], FinishReason::Stop));
    }

    #[tokio::test]
    async fn chat_stream_maps_401_to_auth_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(401)
            .with_body(r#"{"error":{"message":"Invalid API key"}}"#)
            .create_async()
            .await;

        let provider = MiniMaxProvider::new("bad-key", Some(&server.url())).unwrap();
        let result = provider.chat_stream(&make_request(true)).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, crate::error::ProviderError::Auth(_)));
    }
}

// ─── Implementation ──────────────────────────────────────────────────────────
// (GREEN — minimal code to pass the tests above)

use crate::{
    error::ProviderError,
    openai::OpenAiProvider,
    traits::{ChatStream, LlmProvider},
    types::{ChatRequest, ChatResponse, ModelInfo},
};

const DEFAULT_BASE_URL: &str = "https://api.minimax.io/v1";

const KNOWN_MODELS: &[&str] = &["MiniMax-M1", "MiniMax-M2", "MiniMax-M2.1"];

/// MiniMax LLM provider.
///
/// Wraps [`OpenAiProvider`] because MiniMax exposes an OpenAI-compatible API.
/// Overrides `name()` and `list_models()` for MiniMax-specific behaviour.
pub struct MiniMaxProvider {
    inner: OpenAiProvider,
}

impl std::fmt::Debug for MiniMaxProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiniMaxProvider")
            .field("base_url", &self.inner.base_url)
            .finish_non_exhaustive()
    }
}

impl MiniMaxProvider {
    /// Create a new MiniMax provider.
    ///
    /// - `api_key`: Bearer token for MiniMax API authentication (must not be empty).
    /// - `base_url`: Override base URL (default: `https://api.minimax.io/v1`).
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::InvalidRequest`] if `api_key` is empty.
    pub fn new(api_key: impl Into<String>, base_url: Option<&str>) -> Result<Self, ProviderError> {
        let key = api_key.into();
        if key.trim().is_empty() {
            return Err(ProviderError::InvalidRequest(
                "MiniMax API key must not be empty".to_string(),
            ));
        }
        Ok(Self {
            inner: OpenAiProvider::new(key, Some(base_url.unwrap_or(DEFAULT_BASE_URL)), None),
        })
    }
}

#[cfg(test)]
impl MiniMaxProvider {
    fn base_url(&self) -> &str {
        &self.inner.base_url
    }
}

impl LlmProvider for MiniMaxProvider {
    fn name(&self) -> &str {
        "minimax"
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.inner.chat(request).await
    }

    async fn chat_stream(&self, request: &ChatRequest) -> Result<ChatStream, ProviderError> {
        self.inner.chat_stream(request).await
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(KNOWN_MODELS
            .iter()
            .map(|id| ModelInfo {
                id: (*id).to_string(),
                owned_by: "minimax".to_string(),
                created: 0,
            })
            .collect())
    }
}
