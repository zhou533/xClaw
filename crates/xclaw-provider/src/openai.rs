//! OpenAI-compatible provider.

// ─── Tests ───────────────────────────────────────────────────────────────────
// Written FIRST (RED), implementation follows below.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatRequest, FinishReason, Message, Role};
    use mockito::Server;

    fn make_request(stream: bool) -> ChatRequest {
        ChatRequest {
            model: "gpt-4o".to_string(),
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

    // ── OpenAiProvider::new ───────────────────────────────────────────────────

    #[test]
    fn provider_name_is_openai() {
        let p = OpenAiProvider::new("key", None, None);
        assert_eq!(p.name(), "openai");
    }

    #[test]
    fn provider_uses_default_base_url() {
        let p = OpenAiProvider::new("key", None, None);
        assert!(p.base_url.contains("openai.com"));
    }

    #[test]
    fn provider_accepts_custom_base_url() {
        let p = OpenAiProvider::new("key", Some("https://my-proxy.example.com/v1"), None);
        assert_eq!(p.base_url, "https://my-proxy.example.com/v1");
    }

    #[test]
    fn provider_stores_organization() {
        let p = OpenAiProvider::new("key", None, Some("org-123"));
        assert_eq!(p.organization, Some("org-123".to_string()));
    }

    // ── chat() — happy path ───────────────────────────────────────────────────

    #[tokio::test]
    async fn chat_returns_response_on_success() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1694268190,
            "model": "gpt-4o",
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

        let provider = OpenAiProvider::new("test-key", Some(&server.url()), None);
        let resp = provider.chat(&make_request(false)).await.unwrap();

        assert_eq!(resp.id, "chatcmpl-1");
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
            "id": "chatcmpl-2",
            "object": "chat.completion",
            "created": 1694268190,
            "model": "gpt-4o",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        }"#;
        let _mock = server
            .mock("POST", "/chat/completions")
            .match_header("authorization", "Bearer test-key-123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = OpenAiProvider::new("test-key-123", Some(&server.url()), None);
        let resp = provider.chat(&make_request(false)).await;
        assert!(resp.is_ok(), "expected ok, got: {:?}", resp.err());
    }

    // ── chat() — error mapping ────────────────────────────────────────────────

    #[tokio::test]
    async fn chat_maps_401_to_auth_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(401)
            .with_body(r#"{"error":{"message":"Invalid API key","type":"invalid_request_error"}}"#)
            .create_async()
            .await;

        let provider = OpenAiProvider::new("bad-key", Some(&server.url()), None);
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

        let provider = OpenAiProvider::new("key", Some(&server.url()), None);
        let err = provider.chat(&make_request(false)).await.unwrap_err();
        assert!(
            matches!(err, crate::error::ProviderError::RateLimit { .. }),
            "expected RateLimit, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn chat_maps_400_to_invalid_request_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(400)
            .with_body(r#"{"error":{"message":"Bad request"}}"#)
            .create_async()
            .await;

        let provider = OpenAiProvider::new("key", Some(&server.url()), None);
        let err = provider.chat(&make_request(false)).await.unwrap_err();
        assert!(
            matches!(err, crate::error::ProviderError::InvalidRequest(_)),
            "expected InvalidRequest, got: {err:?}"
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

        let provider = OpenAiProvider::new("key", Some(&server.url()), None);
        let err = provider.chat(&make_request(false)).await.unwrap_err();
        assert!(
            matches!(
                err,
                crate::error::ProviderError::ServerError { status: 500, .. }
            ),
            "expected ServerError(500), got: {err:?}"
        );
    }

    // ── chat() — tool_calls in response ──────────────────────────────────────

    #[tokio::test]
    async fn chat_returns_tool_calls_in_response() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "chatcmpl-tc",
            "object": "chat.completion",
            "created": 1694268190,
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": {"name": "get_weather", "arguments": "{\"location\":\"NYC\"}"}
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

        let provider = OpenAiProvider::new("key", Some(&server.url()), None);
        let resp = provider.chat(&make_request(false)).await.unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.content.is_none());
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id, "call_abc");
        assert_eq!(msg.tool_calls[0].function.name, "get_weather");
        assert_eq!(
            msg.tool_calls[0].function.arguments,
            r#"{"location":"NYC"}"#
        );
    }

    // ── list_models() ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_models_returns_model_list() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "object": "list",
            "data": [
                {"id": "gpt-4o", "object": "model", "created": 1694268190, "owned_by": "openai"},
                {"id": "gpt-3.5-turbo", "object": "model", "created": 1677858242, "owned_by": "openai"}
            ]
        }"#;
        let _mock = server
            .mock("GET", "/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = OpenAiProvider::new("key", Some(&server.url()), None);
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "gpt-4o");
        assert_eq!(models[1].id, "gpt-3.5-turbo");
    }

    #[tokio::test]
    async fn list_models_maps_401_to_auth_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/models")
            .with_status(401)
            .with_body(r#"{"error":{"message":"Unauthorized"}}"#)
            .create_async()
            .await;

        let provider = OpenAiProvider::new("bad-key", Some(&server.url()), None);
        let err = provider.list_models().await.unwrap_err();
        assert!(matches!(err, crate::error::ProviderError::Auth(_)));
    }

    // ── chat_stream() ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn chat_stream_yields_content_deltas() {
        use futures::StreamExt;

        let mut server = Server::new_async().await;
        let sse_body = concat!(
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-4o\",",
            "\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-4o\",",
            "\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-4o\",",
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

        let provider = OpenAiProvider::new("key", Some(&server.url()), None);
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
    async fn chat_stream_handles_done_sentinel() {
        use futures::StreamExt;

        let mut server = Server::new_async().await;
        let sse_body = "data: [DONE]\n\n";
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        let provider = OpenAiProvider::new("key", Some(&server.url()), None);
        let mut stream = provider.chat_stream(&make_request(true)).await.unwrap();
        let item = stream.next().await;
        assert!(item.is_none(), "stream should be empty after [DONE]");
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

        let provider = OpenAiProvider::new("bad-key", Some(&server.url()), None);
        let result = provider.chat_stream(&make_request(true)).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, crate::error::ProviderError::Auth(_)));
    }

    // ── SSE parsing helpers ───────────────────────────────────────────────────

    #[test]
    fn parse_sse_line_extracts_json() {
        let line = r#"data: {"id":"x","choices":[],"model":"gpt-4o"}"#;
        let json = parse_sse_data(line).unwrap();
        assert!(json.contains("\"id\""));
    }

    #[test]
    fn parse_sse_line_returns_none_for_done() {
        assert!(parse_sse_data("data: [DONE]").is_none());
    }

    #[test]
    fn parse_sse_line_returns_none_for_empty_line() {
        assert!(parse_sse_data("").is_none());
        assert!(parse_sse_data("  ").is_none());
    }

    #[test]
    fn parse_sse_line_returns_none_for_comment() {
        assert!(parse_sse_data(": keep-alive").is_none());
    }

    // ── Type conversion ───────────────────────────────────────────────────────

    #[test]
    fn openai_finish_reason_stop_converts() {
        let raw = OaiFinishReason::Stop;
        let converted: FinishReason = raw.into();
        assert!(matches!(converted, FinishReason::Stop));
    }

    #[test]
    fn openai_finish_reason_tool_calls_converts() {
        let raw = OaiFinishReason::ToolCalls;
        let converted: FinishReason = raw.into();
        assert!(matches!(converted, FinishReason::ToolCalls));
    }

    #[test]
    fn openai_role_user_converts() {
        let raw = OaiRole::User;
        let converted: Role = raw.into();
        assert!(matches!(converted, Role::User));
    }

    #[test]
    fn openai_response_converts_to_chat_response() {
        let oai = OaiChatResponse {
            id: "chatcmpl-xyz".to_string(),
            model: "gpt-4o".to_string(),
            choices: vec![OaiChoice {
                index: 0,
                message: OaiMessage {
                    role: OaiRole::Assistant,
                    content: Some("Hello".to_string()),
                    tool_calls: None,
                },
                finish_reason: Some(OaiFinishReason::Stop),
            }],
            usage: Some(OaiUsage {
                prompt_tokens: 5,
                completion_tokens: 3,
                total_tokens: 8,
            }),
        };
        let resp: crate::types::ChatResponse = oai.into();
        assert_eq!(resp.id, "chatcmpl-xyz");
        assert_eq!(resp.choices[0].message.content, Some("Hello".to_string()));
    }
}

// ─── Implementation ───────────────────────────────────────────────────────────
// (GREEN — minimal code to pass the tests above)

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    error::ProviderError,
    traits::{ChatStream, LlmProvider},
    types::{
        ChatRequest, ChatResponse, ChatStreamDelta, Choice, DeltaChoice, DeltaFunctionCall,
        DeltaMessage, DeltaToolCall, FinishReason, FunctionCall, Message, ModelInfo, Role,
        ToolCall, Usage,
    },
};

// ─── OpenAI serde types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OaiRole {
    System,
    User,
    Assistant,
    Tool,
    Developer,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OaiFinishReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: OaiFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiMessage {
    pub role: OaiRole,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OaiToolCall>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiChoice {
    pub index: u32,
    pub message: OaiMessage,
    pub finish_reason: Option<OaiFinishReason>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiChatResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<OaiChoice>,
    #[serde(default)]
    pub usage: Option<OaiUsage>,
}

// ── Stream serde types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiDeltaFunctionCall {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiDeltaToolCall {
    pub index: u32,
    pub id: Option<String>,
    pub function: Option<OaiDeltaFunctionCall>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiDeltaMessage {
    pub role: Option<OaiRole>,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OaiDeltaToolCall>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiDeltaChoice {
    pub index: u32,
    pub delta: OaiDeltaMessage,
    pub finish_reason: Option<OaiFinishReason>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OaiStreamChunk {
    pub id: String,
    pub model: String,
    pub choices: Vec<OaiDeltaChoice>,
    #[serde(default)]
    pub usage: Option<OaiUsage>,
}

// ── Models endpoint serde types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OaiModelInfo {
    pub id: String,
    pub owned_by: String,
    pub created: i64,
}

#[derive(Debug, Deserialize)]
struct OaiModelList {
    pub data: Vec<OaiModelInfo>,
}

// ── Request serde types ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OaiFunctionDef<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OaiTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OaiFunctionDef<'a>,
}

#[derive(Debug, Serialize)]
struct OaiRequestMessage<'a> {
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<&'a ToolCall>,
}

#[derive(Debug, Serialize)]
struct OaiChatRequest<'a> {
    model: &'a str,
    messages: Vec<OaiRequestMessage<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OaiTool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

// ─── From conversions ─────────────────────────────────────────────────────────

impl From<OaiFinishReason> for FinishReason {
    fn from(r: OaiFinishReason) -> Self {
        match r {
            OaiFinishReason::Stop | OaiFinishReason::Unknown => Self::Stop,
            OaiFinishReason::ToolCalls => Self::ToolCalls,
            OaiFinishReason::Length => Self::Length,
            OaiFinishReason::ContentFilter => Self::ContentFilter,
        }
    }
}

impl From<OaiRole> for Role {
    fn from(r: OaiRole) -> Self {
        match r {
            OaiRole::System => Self::System,
            OaiRole::User | OaiRole::Unknown => Self::User,
            OaiRole::Assistant => Self::Assistant,
            OaiRole::Tool => Self::Tool,
            OaiRole::Developer => Self::Developer,
        }
    }
}

impl From<OaiUsage> for Usage {
    fn from(u: OaiUsage) -> Self {
        Self {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }
    }
}

impl From<OaiFunctionCall> for FunctionCall {
    fn from(f: OaiFunctionCall) -> Self {
        Self {
            name: f.name,
            arguments: f.arguments,
        }
    }
}

impl From<OaiToolCall> for ToolCall {
    fn from(tc: OaiToolCall) -> Self {
        Self {
            id: tc.id,
            function: tc.function.into(),
        }
    }
}

impl From<OaiMessage> for Message {
    fn from(m: OaiMessage) -> Self {
        Self {
            role: m.role.into(),
            content: m.content,
            tool_calls: m
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            tool_call_id: None,
        }
    }
}

impl From<OaiChoice> for Choice {
    fn from(c: OaiChoice) -> Self {
        Self {
            index: c.index,
            message: c.message.into(),
            finish_reason: c.finish_reason.map(Into::into),
        }
    }
}

impl From<OaiChatResponse> for ChatResponse {
    fn from(r: OaiChatResponse) -> Self {
        Self {
            id: r.id,
            model: r.model,
            choices: r.choices.into_iter().map(Into::into).collect(),
            usage: r.usage.map(Into::into),
        }
    }
}

impl From<OaiDeltaFunctionCall> for DeltaFunctionCall {
    fn from(f: OaiDeltaFunctionCall) -> Self {
        Self {
            name: f.name,
            arguments: f.arguments,
        }
    }
}

impl From<OaiDeltaToolCall> for DeltaToolCall {
    fn from(tc: OaiDeltaToolCall) -> Self {
        Self {
            index: tc.index,
            id: tc.id,
            function: tc.function.map(Into::into),
        }
    }
}

impl From<OaiDeltaMessage> for DeltaMessage {
    fn from(m: OaiDeltaMessage) -> Self {
        Self {
            role: m.role.map(Into::into),
            content: m.content,
            tool_calls: m
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<OaiDeltaChoice> for DeltaChoice {
    fn from(c: OaiDeltaChoice) -> Self {
        Self {
            index: c.index,
            delta: c.delta.into(),
            finish_reason: c.finish_reason.map(Into::into),
        }
    }
}

impl From<OaiStreamChunk> for ChatStreamDelta {
    fn from(chunk: OaiStreamChunk) -> Self {
        Self {
            id: chunk.id,
            model: chunk.model,
            choices: chunk.choices.into_iter().map(Into::into).collect(),
            usage: chunk.usage.map(Into::into),
        }
    }
}

impl From<OaiModelInfo> for ModelInfo {
    fn from(m: OaiModelInfo) -> Self {
        Self {
            id: m.id,
            owned_by: m.owned_by,
            created: m.created,
        }
    }
}

// ─── SSE parsing ─────────────────────────────────────────────────────────────

/// Extract the JSON payload from a `data: <json>` SSE line.
///
/// Returns `None` for `[DONE]`, empty lines, and comments.
pub(crate) fn parse_sse_data(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with(':') {
        return None;
    }
    let payload = trimmed.strip_prefix("data:")?.trim_start();
    if payload.is_empty() || payload == "[DONE]" {
        return None;
    }
    Some(payload)
}

// ─── HTTP error mapping ───────────────────────────────────────────────────────

/// Extract the user-facing message from an OpenAI error JSON body.
///
/// Returns a generic fallback when the body is not valid JSON or lacks `error.message`.
fn extract_error_message(body: &str, fallback: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.get("message")?.as_str().map(String::from))
        .unwrap_or_else(|| fallback.to_string())
}

async fn map_http_error(resp: reqwest::Response) -> ProviderError {
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    match status {
        401 => ProviderError::Auth(extract_error_message(&body, "authentication failed")),
        429 => ProviderError::RateLimit { retry_after: None },
        400..=499 => ProviderError::InvalidRequest(extract_error_message(&body, "invalid request")),
        _ => ProviderError::ServerError {
            status,
            body: extract_error_message(&body, "server error"),
        },
    }
}

// ─── Role to string ───────────────────────────────────────────────────────────

fn role_str(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
        Role::Developer => "developer",
    }
}

// ─── OpenAiProvider ───────────────────────────────────────────────────────────

/// OpenAI-compatible LLM provider.
///
/// Compatible with any OpenAI-format API including Azure OpenAI and local proxies.
pub struct OpenAiProvider {
    pub(crate) base_url: String,
    pub(crate) organization: Option<String>,
    api_key: String,
    client: Client,
}

impl OpenAiProvider {
    /// Create a new provider.
    ///
    /// - `api_key`: Bearer token for `Authorization` header.
    /// - `base_url`: Override base URL (default: `https://api.openai.com/v1`).
    /// - `organization`: Optional `OpenAI-Organization` header value.
    pub fn new(
        api_key: impl Into<String>,
        base_url: Option<&str>,
        organization: Option<&str>,
    ) -> Self {
        use std::time::Duration;

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build reqwest client");

        Self {
            api_key: api_key.into(),
            base_url: base_url
                .unwrap_or("https://api.openai.com/v1")
                .trim_end_matches('/')
                .to_string(),
            organization: organization.map(str::to_string),
            client,
        }
    }

    fn build_request_body<'a>(
        &self,
        req: &'a ChatRequest,
        stream_override: Option<bool>,
    ) -> OaiChatRequest<'a> {
        let messages = req
            .messages
            .iter()
            .map(|m| OaiRequestMessage {
                role: role_str(&m.role),
                content: m.content.as_deref(),
                tool_call_id: m.tool_call_id.as_deref(),
                tool_calls: m.tool_calls.iter().collect(),
            })
            .collect();

        let tools = req
            .tools
            .iter()
            .map(|td| OaiTool {
                kind: "function",
                function: OaiFunctionDef {
                    name: &td.name,
                    description: &td.description,
                    parameters: &td.parameters,
                },
            })
            .collect();

        OaiChatRequest {
            model: &req.model,
            messages,
            stream: stream_override.unwrap_or(req.stream),
            tools,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
        }
    }

    fn add_auth_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let builder = builder
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");
        if let Some(org) = &self.organization {
            builder.header("OpenAI-Organization", org)
        } else {
            builder
        }
    }
}

impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_request_body(request, None);

        let http_resp = self
            .add_auth_headers(self.client.post(&url))
            .json(&body)
            .send()
            .await
            .map_err(ProviderError::from)?;

        if !http_resp.status().is_success() {
            return Err(map_http_error(http_resp).await);
        }

        let oai_resp: OaiChatResponse = http_resp.json().await.map_err(ProviderError::from)?;
        Ok(oai_resp.into())
    }

    async fn chat_stream(&self, request: &ChatRequest) -> Result<ChatStream, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_request_body(request, Some(true));

        let http_resp = self
            .add_auth_headers(self.client.post(&url))
            .json(&body)
            .send()
            .await
            .map_err(ProviderError::from)?;

        if !http_resp.status().is_success() {
            return Err(map_http_error(http_resp).await);
        }

        // Use a line buffer to handle SSE lines split across byte chunks.
        let byte_stream = http_resp.bytes_stream();
        let buffered_stream = futures::stream::unfold(
            (Box::pin(byte_stream), String::new()),
            |(mut stream, mut buf)| async move {
                use futures::StreamExt as _;

                loop {
                    // Drain complete lines from the buffer first.
                    if let Some(newline_pos) = buf.find('\n') {
                        let line = buf[..newline_pos].to_string();
                        buf = buf[newline_pos + 1..].to_string();
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Some(json_str) = parse_sse_data(trimmed) {
                            match serde_json::from_str::<OaiStreamChunk>(json_str) {
                                Ok(chunk) => {
                                    return Some((Ok(chunk.into()), (stream, buf)));
                                }
                                Err(e) => {
                                    return Some((
                                        Err(ProviderError::Deserialize(e.to_string())),
                                        (stream, buf),
                                    ));
                                }
                            }
                        }
                        continue;
                    }

                    // Buffer has no complete line; read more bytes.
                    match stream.next().await {
                        Some(Ok(bytes)) => match String::from_utf8(bytes.to_vec()) {
                            Ok(text) => buf.push_str(&text),
                            Err(e) => {
                                return Some((
                                    Err(ProviderError::Deserialize(e.to_string())),
                                    (stream, buf),
                                ));
                            }
                        },
                        Some(Err(e)) => {
                            return Some((Err(ProviderError::from(e)), (stream, buf)));
                        }
                        None => return None,
                    }
                }
            },
        );

        Ok(Box::pin(buffered_stream))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!("{}/models", self.base_url);

        let http_resp = self
            .add_auth_headers(self.client.get(&url))
            .send()
            .await
            .map_err(ProviderError::from)?;

        if !http_resp.status().is_success() {
            return Err(map_http_error(http_resp).await);
        }

        let list: OaiModelList = http_resp.json().await.map_err(ProviderError::from)?;
        Ok(list.data.into_iter().map(Into::into).collect())
    }
}
