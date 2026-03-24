//! Provider-agnostic types shared by all LLM backends.

use serde::{Deserialize, Serialize};

// ─── Role ─────────────────────────────────────────────────────────────────────

/// Speaker role in a conversation message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
    Developer,
}

// ─── FinishReason ─────────────────────────────────────────────────────────────

/// Reason the model stopped generating tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
}

// ─── FunctionCall ─────────────────────────────────────────────────────────────

/// A function invocation within a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// Raw JSON string — not parsed.
    pub arguments: String,
}

// ─── ToolCall ────────────────────────────────────────────────────────────────

/// A single tool (function) call emitted by the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

// ─── ToolDefinition ──────────────────────────────────────────────────────────

/// Schema describing a tool the model may call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema object for the function parameters.
    pub parameters: serde_json::Value,
}

// ─── Message ─────────────────────────────────────────────────────────────────

/// A single message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

// ─── ChatRequest ─────────────────────────────────────────────────────────────

/// Request to a chat completions endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    pub stream: bool,
}

// ─── Usage ───────────────────────────────────────────────────────────────────

/// Token usage reported by the API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ─── Choice ──────────────────────────────────────────────────────────────────

/// A single completion choice in a non-streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<FinishReason>,
}

// ─── ChatResponse ─────────────────────────────────────────────────────────────

/// Full non-streaming response from a chat completions endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

// ─── DeltaFunctionCall ───────────────────────────────────────────────────────

/// Incremental function call fields in a stream chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaFunctionCall {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

// ─── DeltaToolCall ───────────────────────────────────────────────────────────

/// Incremental tool call fields in a stream chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaToolCall {
    pub index: u32,
    pub id: Option<String>,
    pub function: Option<DeltaFunctionCall>,
}

// ─── DeltaMessage ────────────────────────────────────────────────────────────

/// Incremental message content in a stream chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaMessage {
    pub role: Option<Role>,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<DeltaToolCall>,
}

// ─── DeltaChoice ─────────────────────────────────────────────────────────────

/// A single choice in a streaming chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaChoice {
    pub index: u32,
    pub delta: DeltaMessage,
    pub finish_reason: Option<FinishReason>,
}

// ─── ChatStreamDelta ─────────────────────────────────────────────────────────

/// One SSE chunk from a streaming chat completions response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStreamDelta {
    pub id: String,
    pub model: String,
    pub choices: Vec<DeltaChoice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

// ─── ModelInfo ───────────────────────────────────────────────────────────────

/// Basic metadata about a model returned by the models endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub owned_by: String,
    pub created: i64,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Role ──────────────────────────────────────────────────────────────────

    #[test]
    fn role_serializes_to_lowercase_string() {
        assert_eq!(serde_json::to_string(&Role::System).unwrap(), "\"system\"");
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&Role::Assistant).unwrap(),
            "\"assistant\""
        );
        assert_eq!(serde_json::to_string(&Role::Tool).unwrap(), "\"tool\"");
        assert_eq!(
            serde_json::to_string(&Role::Developer).unwrap(),
            "\"developer\""
        );
    }

    #[test]
    fn role_deserializes_from_lowercase_string() {
        let r: Role = serde_json::from_str("\"user\"").unwrap();
        assert!(matches!(r, Role::User));
    }

    #[test]
    fn role_round_trips() {
        for role in [
            Role::System,
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Developer,
        ] {
            let s = serde_json::to_string(&role).unwrap();
            let back: Role = serde_json::from_str(&s).unwrap();
            assert_eq!(
                serde_json::to_string(&back).unwrap(),
                s,
                "round-trip failed for {:?}",
                role
            );
        }
    }

    // ── FinishReason ──────────────────────────────────────────────────────────

    #[test]
    fn finish_reason_serializes_correctly() {
        assert_eq!(
            serde_json::to_string(&FinishReason::Stop).unwrap(),
            "\"stop\""
        );
        assert_eq!(
            serde_json::to_string(&FinishReason::ToolCalls).unwrap(),
            "\"tool_calls\""
        );
        assert_eq!(
            serde_json::to_string(&FinishReason::Length).unwrap(),
            "\"length\""
        );
        assert_eq!(
            serde_json::to_string(&FinishReason::ContentFilter).unwrap(),
            "\"content_filter\""
        );
    }

    #[test]
    fn finish_reason_deserializes_correctly() {
        let r: FinishReason = serde_json::from_str("\"tool_calls\"").unwrap();
        assert!(matches!(r, FinishReason::ToolCalls));
    }

    // ── Message ───────────────────────────────────────────────────────────────

    #[test]
    fn message_with_content_serializes() {
        let msg = Message {
            role: Role::User,
            content: Some("Hello".to_string()),
            tool_calls: vec![],
            tool_call_id: None,
        };
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"], "Hello");
    }

    #[test]
    fn message_with_null_content_serializes() {
        let msg = Message {
            role: Role::Assistant,
            content: None,
            tool_calls: vec![],
            tool_call_id: None,
        };
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["role"], "assistant");
        assert!(v["content"].is_null());
    }

    #[test]
    fn message_deserializes_with_defaults() {
        let json = json!({"role": "user", "content": "Hi"});
        let msg: Message = serde_json::from_value(json).unwrap();
        assert!(matches!(msg.role, Role::User));
        assert_eq!(msg.content, Some("Hi".to_string()));
        assert!(msg.tool_calls.is_empty());
        assert!(msg.tool_call_id.is_none());
    }

    // ── ToolCall / FunctionCall ───────────────────────────────────────────────

    #[test]
    fn tool_call_round_trips() {
        let tc = ToolCall {
            id: "call_abc".to_string(),
            function: FunctionCall {
                name: "get_weather".to_string(),
                arguments: r#"{"location":"NYC"}"#.to_string(),
            },
        };
        let s = serde_json::to_string(&tc).unwrap();
        let back: ToolCall = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, "call_abc");
        assert_eq!(back.function.name, "get_weather");
        assert_eq!(back.function.arguments, r#"{"location":"NYC"}"#);
    }

    // ── ToolDefinition ────────────────────────────────────────────────────────

    #[test]
    fn tool_definition_round_trips() {
        let td = ToolDefinition {
            name: "fn_name".to_string(),
            description: "does stuff".to_string(),
            parameters: json!({"type": "object"}),
        };
        let s = serde_json::to_string(&td).unwrap();
        let back: ToolDefinition = serde_json::from_str(&s).unwrap();
        assert_eq!(back.name, "fn_name");
        assert_eq!(back.description, "does stuff");
    }

    // ── ChatRequest ───────────────────────────────────────────────────────────

    #[test]
    fn chat_request_defaults() {
        let req = ChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            stream: false,
        };
        assert_eq!(req.model, "gpt-4o");
        assert!(!req.stream);
    }

    #[test]
    fn chat_request_serializes() {
        let req = ChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: Some("hi".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            tools: vec![],
            temperature: Some(0.7),
            max_tokens: Some(100),
            stream: false,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["model"], "gpt-4o");
        let temp = v["temperature"].as_f64().expect("temperature is a number");
        assert!(
            (temp - 0.7).abs() < 0.001,
            "temperature approx 0.7, got {temp}"
        );
        assert_eq!(v["max_tokens"], 100);
    }

    // ── ChatResponse / Choice / Usage ─────────────────────────────────────────

    #[test]
    fn chat_response_deserializes() {
        let json = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}
        });
        let resp: ChatResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.id, "chatcmpl-123");
        assert_eq!(resp.choices.len(), 1);
        let choice = &resp.choices[0];
        assert_eq!(choice.index, 0);
        assert!(matches!(choice.finish_reason, Some(FinishReason::Stop)));
        let usage = resp.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.total_tokens, 30);
    }

    #[test]
    fn chat_response_with_null_usage() {
        let json = json!({
            "id": "x",
            "model": "gpt-4o",
            "choices": []
        });
        let resp: ChatResponse = serde_json::from_value(json).unwrap();
        assert!(resp.usage.is_none());
    }

    // ── ChatStreamDelta ───────────────────────────────────────────────────────

    #[test]
    fn stream_delta_deserializes_partial_chunk() {
        let json = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": {"content": "Hello"},
                "finish_reason": null
            }]
        });
        let delta: ChatStreamDelta = serde_json::from_value(json).unwrap();
        assert_eq!(delta.choices.len(), 1);
        let choice = &delta.choices[0];
        assert_eq!(choice.delta.content, Some("Hello".to_string()));
        assert!(choice.finish_reason.is_none());
    }

    #[test]
    fn stream_delta_deserializes_final_chunk() {
        let json = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": "stop"
            }]
        });
        let delta: ChatStreamDelta = serde_json::from_value(json).unwrap();
        let choice = &delta.choices[0];
        assert!(matches!(choice.finish_reason, Some(FinishReason::Stop)));
        assert!(choice.delta.content.is_none());
    }

    #[test]
    fn stream_delta_with_tool_call_chunk() {
        let json = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_abc",
                        "function": {"name": "get_weather", "arguments": ""}
                    }]
                },
                "finish_reason": null
            }]
        });
        let delta: ChatStreamDelta = serde_json::from_value(json).unwrap();
        let choice = &delta.choices[0];
        assert_eq!(choice.delta.tool_calls.len(), 1);
        let tc = &choice.delta.tool_calls[0];
        assert_eq!(tc.index, 0);
        assert_eq!(tc.id, Some("call_abc".to_string()));
        assert_eq!(
            tc.function.as_ref().unwrap().name,
            Some("get_weather".to_string())
        );
    }

    // ── ModelInfo ─────────────────────────────────────────────────────────────

    #[test]
    fn model_info_deserializes() {
        let json = json!({"id": "gpt-4o", "owned_by": "openai", "created": 1694268190i64});
        let info: ModelInfo = serde_json::from_value(json).unwrap();
        assert_eq!(info.id, "gpt-4o");
        assert_eq!(info.owned_by, "openai");
        assert_eq!(info.created, 1694268190);
    }

    // ── Edge cases ────────────────────────────────────────────────────────────

    #[test]
    fn message_with_empty_tool_calls_vec() {
        let msg = Message {
            role: Role::Assistant,
            content: Some("text".to_string()),
            tool_calls: vec![],
            tool_call_id: None,
        };
        assert!(msg.tool_calls.is_empty());
    }

    #[test]
    fn usage_zero_tokens() {
        let u = Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        };
        assert_eq!(u.prompt_tokens, 0);
    }

    #[test]
    fn chat_request_with_unicode_content() {
        let msg = Message {
            role: Role::User,
            content: Some("你好 🌍".to_string()),
            tool_calls: vec![],
            tool_call_id: None,
        };
        let s = serde_json::to_string(&msg).unwrap();
        let back: Message = serde_json::from_str(&s).unwrap();
        assert_eq!(back.content.unwrap(), "你好 🌍");
    }

    #[test]
    fn function_call_arguments_are_raw_string_not_parsed() {
        let raw = r#"{"location":"NYC","unit":"celsius"}"#;
        let fc = FunctionCall {
            name: "fn".to_string(),
            arguments: raw.to_string(),
        };
        assert_eq!(fc.arguments, raw);
    }
}
