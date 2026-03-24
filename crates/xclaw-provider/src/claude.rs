//! Anthropic Claude provider.

// ─── Tests ───────────────────────────────────────────────────────────────────
// Written FIRST (RED), implementation follows below.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ChatRequest, FinishReason, FunctionCall, Message, Role, ToolCall, ToolDefinition,
    };
    use mockito::Server;
    use serde_json::json;

    fn make_request(stream: bool) -> ChatRequest {
        ChatRequest {
            model: "claude-sonnet-4-5-20250929".to_string(),
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

    // ── ClaudeProvider::new ─────────────────────────────────────────────────

    #[test]
    fn provider_name_is_claude() {
        let p = ClaudeProvider::new("key", None);
        assert_eq!(p.name(), "claude");
    }

    #[test]
    fn provider_uses_default_base_url() {
        let p = ClaudeProvider::new("key", None);
        assert!(p.base_url.contains("anthropic.com"));
    }

    #[test]
    fn provider_accepts_custom_base_url() {
        let p = ClaudeProvider::new("key", Some("https://my-proxy.example.com/v1"));
        assert_eq!(p.base_url, "https://my-proxy.example.com/v1");
    }

    // ── convert_messages ────────────────────────────────────────────────────

    #[test]
    fn convert_extracts_system_message() {
        let messages = vec![
            Message {
                role: Role::System,
                content: Some("You are helpful.".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: Some("Hi".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
        ];
        let (system, claude_msgs) = convert_messages(&messages).unwrap();
        assert_eq!(system, Some("You are helpful.".to_string()));
        assert_eq!(claude_msgs.len(), 1);
        assert_eq!(claude_msgs[0].role, "user");
    }

    #[test]
    fn convert_merges_multiple_system_messages() {
        let messages = vec![
            Message {
                role: Role::System,
                content: Some("Be helpful.".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
            Message {
                role: Role::System,
                content: Some("Be concise.".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: Some("Hi".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
        ];
        let (system, _) = convert_messages(&messages).unwrap();
        let sys = system.unwrap();
        assert!(sys.contains("Be helpful."));
        assert!(sys.contains("Be concise."));
    }

    #[test]
    fn convert_maps_developer_to_system() {
        let messages = vec![
            Message {
                role: Role::Developer,
                content: Some("Dev instructions.".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: Some("Hi".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
        ];
        let (system, claude_msgs) = convert_messages(&messages).unwrap();
        assert_eq!(system, Some("Dev instructions.".to_string()));
        assert_eq!(claude_msgs.len(), 1);
    }

    #[test]
    fn convert_folds_tool_messages_into_user_message() {
        let messages = vec![
            Message {
                role: Role::User,
                content: Some("What's the weather?".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
            Message {
                role: Role::Assistant,
                content: None,
                tool_calls: vec![ToolCall {
                    id: "call_1".to_string(),
                    function: FunctionCall {
                        name: "get_weather".to_string(),
                        arguments: r#"{"loc":"NYC"}"#.to_string(),
                    },
                }],
                tool_call_id: None,
            },
            Message {
                role: Role::Tool,
                content: Some("Sunny, 72F".to_string()),
                tool_calls: vec![],
                tool_call_id: Some("call_1".to_string()),
            },
        ];
        let (_, claude_msgs) = convert_messages(&messages).unwrap();
        // user, assistant (with tool_use), user (with tool_result)
        assert_eq!(claude_msgs.len(), 3);
        assert_eq!(claude_msgs[2].role, "user");
        // The last message should contain a tool_result content block
        let has_tool_result = claude_msgs[2]
            .content
            .iter()
            .any(|b| matches!(b, CRequestContentBlock::ToolResult { .. }));
        assert!(has_tool_result, "expected tool_result content block");
    }

    #[test]
    fn convert_assistant_tool_calls_to_tool_use_blocks() {
        let messages = vec![
            Message {
                role: Role::User,
                content: Some("Hi".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
            Message {
                role: Role::Assistant,
                content: Some("Let me check.".to_string()),
                tool_calls: vec![ToolCall {
                    id: "call_abc".to_string(),
                    function: FunctionCall {
                        name: "search".to_string(),
                        arguments: r#"{"q":"rust"}"#.to_string(),
                    },
                }],
                tool_call_id: None,
            },
        ];
        let (_, claude_msgs) = convert_messages(&messages).unwrap();
        assert_eq!(claude_msgs.len(), 2);
        assert_eq!(claude_msgs[1].role, "assistant");
        let has_text = claude_msgs[1]
            .content
            .iter()
            .any(|b| matches!(b, CRequestContentBlock::Text { .. }));
        let has_tool_use = claude_msgs[1]
            .content
            .iter()
            .any(|b| matches!(b, CRequestContentBlock::ToolUse { .. }));
        assert!(has_text, "expected text block for assistant content");
        assert!(has_tool_use, "expected tool_use block for tool call");
    }

    #[test]
    fn convert_empty_messages() {
        let messages: Vec<Message> = vec![];
        let (system, claude_msgs) = convert_messages(&messages).unwrap();
        assert!(system.is_none());
        assert!(claude_msgs.is_empty());
    }

    #[test]
    fn convert_rejects_tool_message_without_tool_call_id() {
        let messages = vec![
            Message {
                role: Role::User,
                content: Some("Hi".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
            Message {
                role: Role::Tool,
                content: Some("result".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
        ];
        let result = convert_messages(&messages);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, crate::error::ProviderError::InvalidRequest(_)),
            "expected InvalidRequest, got: {err:?}"
        );
    }

    // ── From: ClaudeStopReason -> FinishReason ──────────────────────────────

    #[test]
    fn stop_reason_end_turn_converts_to_stop() {
        let r = CStopReason::EndTurn;
        let converted: FinishReason = r.into();
        assert!(matches!(converted, FinishReason::Stop));
    }

    #[test]
    fn stop_reason_tool_use_converts_to_tool_calls() {
        let r = CStopReason::ToolUse;
        let converted: FinishReason = r.into();
        assert!(matches!(converted, FinishReason::ToolCalls));
    }

    #[test]
    fn stop_reason_max_tokens_converts_to_length() {
        let r = CStopReason::MaxTokens;
        let converted: FinishReason = r.into();
        assert!(matches!(converted, FinishReason::Length));
    }

    #[test]
    fn stop_reason_stop_sequence_converts_to_stop() {
        let r = CStopReason::StopSequence;
        let converted: FinishReason = r.into();
        assert!(matches!(converted, FinishReason::Stop));
    }

    // ── From: CUsage -> Usage ───────────────────────────────────────────────

    #[test]
    fn usage_computes_total_tokens() {
        let u = CUsage {
            input_tokens: 10,
            output_tokens: 20,
        };
        let converted: crate::types::Usage = u.into();
        assert_eq!(converted.prompt_tokens, 10);
        assert_eq!(converted.completion_tokens, 20);
        assert_eq!(converted.total_tokens, 30);
    }

    // ── From: CResponse -> ChatResponse ─────────────────────────────────────

    #[test]
    fn claude_response_converts_to_chat_response_text() {
        let resp = CResponse {
            id: "msg_123".to_string(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            content: vec![CResponseContentBlock::Text {
                text: "Hello!".to_string(),
            }],
            stop_reason: Some(CStopReason::EndTurn),
            usage: CUsage {
                input_tokens: 5,
                output_tokens: 3,
            },
        };
        let chat: ChatResponse = resp.into();
        assert_eq!(chat.id, "msg_123");
        assert_eq!(chat.choices.len(), 1);
        assert_eq!(chat.choices[0].message.content, Some("Hello!".to_string()));
        assert!(matches!(
            chat.choices[0].finish_reason,
            Some(FinishReason::Stop)
        ));
        let usage = chat.usage.unwrap();
        assert_eq!(usage.total_tokens, 8);
    }

    #[test]
    fn claude_response_converts_tool_use_blocks() {
        let resp = CResponse {
            id: "msg_456".to_string(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            content: vec![
                CResponseContentBlock::Text {
                    text: "Let me check.".to_string(),
                },
                CResponseContentBlock::ToolUse {
                    id: "toolu_abc".to_string(),
                    name: "get_weather".to_string(),
                    input: json!({"location": "NYC"}),
                },
            ],
            stop_reason: Some(CStopReason::ToolUse),
            usage: CUsage {
                input_tokens: 10,
                output_tokens: 20,
            },
        };
        let chat: ChatResponse = resp.into();
        let msg = &chat.choices[0].message;
        assert_eq!(msg.content, Some("Let me check.".to_string()));
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id, "toolu_abc");
        assert_eq!(msg.tool_calls[0].function.name, "get_weather");
        assert_eq!(
            msg.tool_calls[0].function.arguments,
            r#"{"location":"NYC"}"#
        );
        assert!(matches!(
            chat.choices[0].finish_reason,
            Some(FinishReason::ToolCalls)
        ));
    }

    #[test]
    fn claude_response_multiple_text_blocks_concatenated() {
        let resp = CResponse {
            id: "msg_789".to_string(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            content: vec![
                CResponseContentBlock::Text {
                    text: "Hello ".to_string(),
                },
                CResponseContentBlock::Text {
                    text: "world!".to_string(),
                },
            ],
            stop_reason: Some(CStopReason::EndTurn),
            usage: CUsage {
                input_tokens: 1,
                output_tokens: 2,
            },
        };
        let chat: ChatResponse = resp.into();
        assert_eq!(
            chat.choices[0].message.content,
            Some("Hello world!".to_string())
        );
    }

    // ── SSE parsing ─────────────────────────────────────────────────────────

    #[test]
    fn parse_sse_extracts_event_and_data() {
        let lines = "event: message_start\ndata: {\"type\":\"message_start\"}\n\n";
        let events = parse_claude_sse_events(lines);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "message_start");
        assert!(events[0].1.contains("message_start"));
    }

    #[test]
    fn parse_sse_skips_empty_lines_and_comments() {
        let lines = ": keep-alive\n\nevent: ping\ndata: {}\n\n";
        let events = parse_claude_sse_events(lines);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "ping");
    }

    #[test]
    fn parse_sse_handles_multiple_events() {
        let lines = concat!(
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\"}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\"}\n\n",
        );
        let events = parse_claude_sse_events(lines);
        assert_eq!(events.len(), 2);
    }

    // ── chat() — happy path ─────────────────────────────────────────────────

    #[tokio::test]
    async fn chat_returns_response_on_success() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "msg_01",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hi!"}],
            "model": "claude-sonnet-4-5-20250929",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 5, "output_tokens": 3}
        }"#;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("test-key", Some(&server.url()));
        let resp = provider.chat(&make_request(false)).await.unwrap();

        assert_eq!(resp.id, "msg_01");
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
    async fn chat_sends_x_api_key_header() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "msg_02",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "ok"}],
            "model": "claude-sonnet-4-5-20250929",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }"#;
        let _mock = server
            .mock("POST", "/v1/messages")
            .match_header("x-api-key", "test-key-123")
            .match_header("anthropic-version", "2023-06-01")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("test-key-123", Some(&server.url()));
        let resp = provider.chat(&make_request(false)).await;
        assert!(resp.is_ok(), "expected ok, got: {:?}", resp.err());
    }

    #[tokio::test]
    async fn chat_uses_default_max_tokens_when_none() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "msg_03",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "ok"}],
            "model": "claude-sonnet-4-5-20250929",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }"#;
        let _mock = server
            .mock("POST", "/v1/messages")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"max_tokens":4096}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("key", Some(&server.url()));
        let req = make_request(false);
        assert!(req.max_tokens.is_none());
        let resp = provider.chat(&req).await;
        assert!(resp.is_ok(), "expected ok, got: {:?}", resp.err());
    }

    // ── chat() — error mapping ──────────────────────────────────────────────

    #[tokio::test]
    async fn chat_maps_401_to_auth_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(401)
            .with_body(r#"{"type":"error","error":{"type":"authentication_error","message":"Invalid API key"}}"#)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("bad-key", Some(&server.url()));
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
            .mock("POST", "/v1/messages")
            .with_status(429)
            .with_body(
                r#"{"type":"error","error":{"type":"rate_limit_error","message":"Rate limited"}}"#,
            )
            .create_async()
            .await;

        let provider = ClaudeProvider::new("key", Some(&server.url()));
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
            .mock("POST", "/v1/messages")
            .with_status(400)
            .with_body(r#"{"type":"error","error":{"type":"invalid_request_error","message":"Bad request"}}"#)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("key", Some(&server.url()));
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
            .mock("POST", "/v1/messages")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let provider = ClaudeProvider::new("key", Some(&server.url()));
        let err = provider.chat(&make_request(false)).await.unwrap_err();
        assert!(
            matches!(
                err,
                crate::error::ProviderError::ServerError { status: 500, .. }
            ),
            "expected ServerError(500), got: {err:?}"
        );
    }

    // ── chat() — tool_calls in response ─────────────────────────────────────

    #[tokio::test]
    async fn chat_returns_tool_calls_in_response() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "msg_tc",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "toolu_abc", "name": "get_weather", "input": {"location": "NYC"}}
            ],
            "model": "claude-sonnet-4-5-20250929",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 20}
        }"#;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("key", Some(&server.url()));
        let resp = provider.chat(&make_request(false)).await.unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.content.is_none());
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id, "toolu_abc");
        assert_eq!(msg.tool_calls[0].function.name, "get_weather");
        assert_eq!(
            msg.tool_calls[0].function.arguments,
            r#"{"location":"NYC"}"#
        );
    }

    // ── chat() — tool definitions in request ────────────────────────────────

    #[tokio::test]
    async fn chat_sends_tools_with_input_schema() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "id": "msg_td",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "ok"}],
            "model": "claude-sonnet-4-5-20250929",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 1, "output_tokens": 1}
        }"#;
        let _mock = server
            .mock("POST", "/v1/messages")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"tools":[{"name":"get_weather","description":"Get weather","input_schema":{"type":"object"}}]}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("key", Some(&server.url()));
        let req = ChatRequest {
            model: "claude-sonnet-4-5-20250929".to_string(),
            messages: vec![Message {
                role: Role::User,
                content: Some("Hi".to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            tools: vec![ToolDefinition {
                name: "get_weather".to_string(),
                description: "Get weather".to_string(),
                parameters: json!({"type": "object"}),
            }],
            temperature: None,
            max_tokens: Some(100),
            stream: false,
        };
        let resp = provider.chat(&req).await;
        assert!(resp.is_ok(), "expected ok, got: {:?}", resp.err());
    }

    // ── list_models() ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_models_returns_model_list() {
        let mut server = Server::new_async().await;
        let body = r#"{
            "data": [
                {"id": "claude-sonnet-4-5-20250929", "type": "model", "display_name": "Claude Sonnet 4.5", "created_at": "2025-09-29T00:00:00Z"},
                {"id": "claude-haiku-4-5-20251001", "type": "model", "display_name": "Claude Haiku 4.5", "created_at": "2025-10-01T00:00:00Z"}
            ],
            "has_more": false,
            "first_id": "claude-sonnet-4-5-20250929",
            "last_id": "claude-haiku-4-5-20251001"
        }"#;
        let _mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("key", Some(&server.url()));
        let models = provider.list_models().await.unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "claude-sonnet-4-5-20250929");
        assert_eq!(models[1].id, "claude-haiku-4-5-20251001");
    }

    #[tokio::test]
    async fn list_models_maps_401_to_auth_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/v1/models")
            .with_status(401)
            .with_body(r#"{"type":"error","error":{"type":"authentication_error","message":"Unauthorized"}}"#)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("bad-key", Some(&server.url()));
        let err = provider.list_models().await.unwrap_err();
        assert!(matches!(err, crate::error::ProviderError::Auth(_)));
    }

    // ── chat_stream() ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn chat_stream_yields_content_deltas() {
        use futures::StreamExt;

        let mut server = Server::new_async().await;
        let sse_body = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_s1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-5-20250929\",\"stop_reason\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("key", Some(&server.url()));
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
            "expected 'Hello' in stream, got: {:?}",
            contents
        );
        assert!(
            contents.iter().any(|c| c == " world"),
            "expected ' world' in stream, got: {:?}",
            contents
        );
        assert_eq!(finish_reasons.len(), 1);
        assert!(matches!(finish_reasons[0], FinishReason::Stop));
    }

    #[tokio::test]
    async fn chat_stream_yields_tool_use_deltas() {
        use futures::StreamExt;

        let mut server = Server::new_async().await;
        let sse_body = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_tu\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-5-20250929\",\"stop_reason\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"get_weather\",\"input\":{}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"loc\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"ation\\\":\\\"NYC\\\"}\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":15}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(sse_body)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("key", Some(&server.url()));
        let mut stream = provider.chat_stream(&make_request(true)).await.unwrap();

        let mut tool_call_ids = vec![];
        let mut tool_call_names = vec![];
        let mut tool_call_args = vec![];
        let mut finish_reasons = vec![];

        while let Some(result) = stream.next().await {
            let delta = result.unwrap();
            for choice in &delta.choices {
                for tc in &choice.delta.tool_calls {
                    if let Some(id) = &tc.id {
                        tool_call_ids.push(id.clone());
                    }
                    if let Some(f) = &tc.function {
                        if let Some(n) = &f.name {
                            tool_call_names.push(n.clone());
                        }
                        if let Some(a) = &f.arguments {
                            tool_call_args.push(a.clone());
                        }
                    }
                }
                if let Some(fr) = &choice.finish_reason {
                    finish_reasons.push(fr.clone());
                }
            }
        }

        assert_eq!(tool_call_ids, vec!["toolu_1"]);
        assert_eq!(tool_call_names, vec!["get_weather"]);
        assert!(
            !tool_call_args.is_empty(),
            "expected tool call argument chunks"
        );
        assert_eq!(finish_reasons.len(), 1);
        assert!(matches!(finish_reasons[0], FinishReason::ToolCalls));
    }

    #[tokio::test]
    async fn chat_stream_maps_401_to_auth_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(401)
            .with_body(r#"{"type":"error","error":{"type":"authentication_error","message":"Invalid API key"}}"#)
            .create_async()
            .await;

        let provider = ClaudeProvider::new("bad-key", Some(&server.url()));
        let result = provider.chat_stream(&make_request(true)).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, crate::error::ProviderError::Auth(_)));
    }
}

// ─── Implementation ─────────────────────────────────────────────────────────
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

// ─── Constants ──────────────────────────────────────────────────────────────

/// Default max_tokens when ChatRequest.max_tokens is None.
/// Claude API requires this field. 4096 is safe for all Claude models.
const DEFAULT_MAX_TOKENS: u32 = 4096;
const ANTHROPIC_VERSION: &str = "2023-06-01";

// ─── Claude request serde types ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub(crate) enum CRequestContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CRequestMessage {
    pub role: String,
    pub content: Vec<CRequestContentBlock>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CToolDef<'a> {
    pub name: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub description: &'a str,
    pub input_schema: &'a serde_json::Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct CChatRequest<'a> {
    pub model: &'a str,
    pub messages: Vec<CRequestMessage>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<CToolDef<'a>>,
}

// ─── Claude response serde types ────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CStopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum CResponseContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CResponse {
    pub id: String,
    pub model: String,
    pub content: Vec<CResponseContentBlock>,
    pub stop_reason: Option<CStopReason>,
    pub usage: CUsage,
}

// ─── Claude stream serde types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct CStreamMessageStart {
    pub message: CStreamMessageInfo,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CStreamMessageInfo {
    pub id: String,
    pub model: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CStreamContentBlockStart {
    pub index: u32,
    pub content_block: CStreamContentBlock,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum CStreamContentBlock {
    #[serde(rename = "text")]
    Text {
        #[allow(dead_code)]
        text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        #[allow(dead_code)]
        input: serde_json::Value,
    },
}

#[derive(Debug, Deserialize)]
pub(crate) struct CStreamContentBlockDelta {
    pub index: u32,
    pub delta: CStreamDelta,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum CStreamDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Deserialize)]
pub(crate) struct CStreamMessageDelta {
    pub delta: CStreamMessageDeltaBody,
    pub usage: Option<CStreamMessageDeltaUsage>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CStreamMessageDeltaBody {
    pub stop_reason: Option<CStopReason>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CStreamMessageDeltaUsage {
    pub output_tokens: u32,
}

// ─── Models endpoint serde types ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CModelInfo {
    pub id: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub display_name: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CModelList {
    pub data: Vec<CModelInfo>,
}

// ─── From conversions ───────────────────────────────────────────────────────

impl From<CStopReason> for FinishReason {
    fn from(r: CStopReason) -> Self {
        match r {
            CStopReason::EndTurn | CStopReason::StopSequence | CStopReason::Unknown => Self::Stop,
            CStopReason::ToolUse => Self::ToolCalls,
            CStopReason::MaxTokens => Self::Length,
        }
    }
}

impl From<CUsage> for Usage {
    fn from(u: CUsage) -> Self {
        Self {
            prompt_tokens: u.input_tokens,
            completion_tokens: u.output_tokens,
            total_tokens: u.input_tokens + u.output_tokens,
        }
    }
}

impl From<CResponse> for ChatResponse {
    fn from(r: CResponse) -> Self {
        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for block in r.content {
            match block {
                CResponseContentBlock::Text { text } => text_parts.push(text),
                CResponseContentBlock::ToolUse { id, name, input } => {
                    let arguments = serde_json::to_string(&input).unwrap_or_else(|e| {
                        tracing::warn!(error = %e, "failed to serialize tool_use input, defaulting to empty object");
                        "{}".to_string()
                    });
                    tool_calls.push(ToolCall {
                        id,
                        function: FunctionCall { name, arguments },
                    });
                }
            }
        }

        let content = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join(""))
        };

        Self {
            id: r.id,
            model: r.model,
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: Role::Assistant,
                    content,
                    tool_calls,
                    tool_call_id: None,
                },
                finish_reason: r.stop_reason.map(Into::into),
            }],
            usage: Some(r.usage.into()),
        }
    }
}

impl From<CModelInfo> for ModelInfo {
    fn from(m: CModelInfo) -> Self {
        Self {
            id: m.id,
            owned_by: "anthropic".to_string(),
            // Claude API returns created_at as RFC3339; default to 0 to avoid adding chrono dep.
            created: 0,
        }
    }
}

// ─── Message conversion ─────────────────────────────────────────────────────

/// Convert unified `Message` list to Claude format.
///
/// Returns `Ok((system_prompt, claude_messages))`:
/// - System/Developer messages are extracted and merged into a single system prompt.
/// - Tool messages are folded into user messages with `tool_result` content blocks.
/// - Assistant messages with tool_calls become assistant messages with `tool_use` content blocks.
///
/// Returns `Err` if a Tool message is missing `tool_call_id` or has invalid JSON arguments.
pub(crate) fn convert_messages(
    messages: &[Message],
) -> Result<(Option<String>, Vec<CRequestMessage>), ProviderError> {
    let mut system_parts: Vec<String> = Vec::new();
    let mut claude_msgs: Vec<CRequestMessage> = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System | Role::Developer => {
                if let Some(text) = &msg.content {
                    system_parts.push(text.clone());
                }
            }
            Role::User => {
                claude_msgs.push(CRequestMessage {
                    role: "user".to_string(),
                    content: vec![CRequestContentBlock::Text {
                        text: msg.content.clone().unwrap_or_default(),
                    }],
                });
            }
            Role::Assistant => {
                let mut blocks: Vec<CRequestContentBlock> = Vec::new();
                if let Some(text) = &msg.content {
                    blocks.push(CRequestContentBlock::Text { text: text.clone() });
                }
                for tc in &msg.tool_calls {
                    let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or_else(|e| {
                            tracing::warn!(
                                error = %e,
                                raw = %tc.function.arguments,
                                "invalid tool-call arguments JSON, defaulting to null"
                            );
                            serde_json::Value::Null
                        });
                    blocks.push(CRequestContentBlock::ToolUse {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        input,
                    });
                }
                claude_msgs.push(CRequestMessage {
                    role: "assistant".to_string(),
                    content: blocks,
                });
            }
            Role::Tool => {
                let tool_use_id = msg
                    .tool_call_id
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        ProviderError::InvalidRequest(
                            "tool message missing tool_call_id".to_string(),
                        )
                    })?
                    .clone();
                claude_msgs.push(CRequestMessage {
                    role: "user".to_string(),
                    content: vec![CRequestContentBlock::ToolResult {
                        tool_use_id,
                        content: msg.content.clone().unwrap_or_default(),
                    }],
                });
            }
        }
    }

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n"))
    };

    Ok((system, claude_msgs))
}

// ─── SSE parsing ────────────────────────────────────────────────────────────

/// Parse a single SSE block (lines between blank-line separators) into `(event_type, data)`.
///
/// Returns `None` for comment-only blocks or blocks missing event/data fields.
fn parse_sse_block(block: &str) -> Option<(String, String)> {
    let mut event_type = String::new();
    let mut data = String::new();

    for line in block.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(':') {
            continue;
        }
        if let Some(evt) = trimmed.strip_prefix("event:") {
            event_type = evt.trim().to_string();
        } else if let Some(d) = trimmed.strip_prefix("data:") {
            data = d.trim().to_string();
        }
    }

    if event_type.is_empty() || data.is_empty() {
        return None;
    }
    Some((event_type, data))
}

/// Parse Claude SSE text into `(event_type, data_json)` pairs.
#[cfg(test)]
pub(crate) fn parse_claude_sse_events(text: &str) -> Vec<(String, String)> {
    text.split("\n\n")
        .filter_map(|block| {
            let trimmed = block.trim();
            if trimmed.is_empty() {
                return None;
            }
            parse_sse_block(trimmed)
        })
        .collect()
}

// ─── HTTP error mapping ─────────────────────────────────────────────────────

fn extract_claude_error_message(body: &str, fallback: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.get("message")?.as_str().map(String::from))
        .unwrap_or_else(|| fallback.to_string())
}

async fn map_http_error(resp: reqwest::Response) -> ProviderError {
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(std::time::Duration::from_secs);
    let body = resp.text().await.unwrap_or_default();
    match status {
        401 => ProviderError::Auth(extract_claude_error_message(&body, "authentication failed")),
        429 => ProviderError::RateLimit { retry_after },
        400..=499 => {
            ProviderError::InvalidRequest(extract_claude_error_message(&body, "invalid request"))
        }
        _ => ProviderError::ServerError {
            status,
            body: extract_claude_error_message(&body, "server error"),
        },
    }
}

// ─── Stream delta helpers ───────────────────────────────────────────────────

/// Build a `ChatStreamDelta` with a single choice.
fn make_delta(
    id: &str,
    model: &str,
    delta: DeltaMessage,
    finish_reason: Option<FinishReason>,
    usage: Option<Usage>,
) -> ChatStreamDelta {
    ChatStreamDelta {
        id: id.to_string(),
        model: model.to_string(),
        choices: vec![DeltaChoice {
            index: 0,
            delta,
            finish_reason,
        }],
        usage,
    }
}

fn text_delta(content: String) -> DeltaMessage {
    DeltaMessage {
        role: None,
        content: Some(content),
        tool_calls: vec![],
    }
}

fn tool_call_delta(index: u32, id: Option<String>, function: DeltaFunctionCall) -> DeltaMessage {
    DeltaMessage {
        role: None,
        content: None,
        tool_calls: vec![DeltaToolCall {
            index,
            id,
            function: Some(function),
        }],
    }
}

fn empty_delta() -> DeltaMessage {
    DeltaMessage {
        role: None,
        content: None,
        tool_calls: vec![],
    }
}

// ─── Stream state machine ───────────────────────────────────────────────────

/// Immutable state carried across SSE events to produce unified `ChatStreamDelta` items.
struct StreamState {
    message_id: String,
    model: String,
}

impl StreamState {
    fn new() -> Self {
        Self {
            message_id: String::new(),
            model: String::new(),
        }
    }

    /// Process one SSE event pair. Returns the (possibly updated) state and an optional delta.
    fn process_event(
        self,
        event_type: &str,
        data: &str,
    ) -> Result<(Self, Option<ChatStreamDelta>), ProviderError> {
        match event_type {
            "message_start" => {
                let parsed: CStreamMessageStart = serde_json::from_str(data)?;
                let next = Self {
                    message_id: parsed.message.id,
                    model: parsed.message.model,
                };
                Ok((next, None))
            }
            "content_block_start" => {
                let parsed: CStreamContentBlockStart = serde_json::from_str(data)?;
                let delta = match parsed.content_block {
                    CStreamContentBlock::Text { .. } => make_delta(
                        &self.message_id,
                        &self.model,
                        DeltaMessage {
                            role: Some(Role::Assistant),
                            content: Some(String::new()),
                            tool_calls: vec![],
                        },
                        None,
                        None,
                    ),
                    CStreamContentBlock::ToolUse { id, name, .. } => make_delta(
                        &self.message_id,
                        &self.model,
                        tool_call_delta(
                            parsed.index,
                            Some(id),
                            DeltaFunctionCall {
                                name: Some(name),
                                arguments: Some(String::new()),
                            },
                        ),
                        None,
                        None,
                    ),
                };
                Ok((self, Some(delta)))
            }
            "content_block_delta" => {
                let parsed: CStreamContentBlockDelta = serde_json::from_str(data)?;
                let delta = match parsed.delta {
                    CStreamDelta::TextDelta { text } => {
                        make_delta(&self.message_id, &self.model, text_delta(text), None, None)
                    }
                    CStreamDelta::InputJsonDelta { partial_json } => make_delta(
                        &self.message_id,
                        &self.model,
                        tool_call_delta(
                            parsed.index,
                            None,
                            DeltaFunctionCall {
                                name: None,
                                arguments: Some(partial_json),
                            },
                        ),
                        None,
                        None,
                    ),
                };
                Ok((self, Some(delta)))
            }
            "content_block_stop" => Ok((self, None)),
            "message_delta" => {
                let parsed: CStreamMessageDelta = serde_json::from_str(data)?;
                let finish_reason = parsed.delta.stop_reason.map(Into::into);
                let usage = parsed.usage.map(|u| Usage {
                    prompt_tokens: 0,
                    completion_tokens: u.output_tokens,
                    total_tokens: u.output_tokens,
                });
                let delta = make_delta(
                    &self.message_id,
                    &self.model,
                    empty_delta(),
                    finish_reason,
                    usage,
                );
                Ok((self, Some(delta)))
            }
            _ => Ok((self, None)),
        }
    }
}

// ─── Stream buffer helpers ──────────────────────────────────────────────────

/// Extract the next complete SSE block from the buffer, consuming it via `split_off`.
/// Returns `None` if no complete block (terminated by `\n\n`) is available.
fn take_next_block(buf: &mut String) -> Option<String> {
    let block_end = buf.find("\n\n")?;
    let block = buf[..block_end].to_string();
    let remainder = buf.split_off(block_end + 2);
    *buf = remainder;
    Some(block)
}

// ─── ClaudeProvider ─────────────────────────────────────────────────────────

/// Anthropic Claude LLM provider.
pub struct ClaudeProvider {
    pub(crate) base_url: String,
    api_key: String,
    client: Client,
}

impl ClaudeProvider {
    /// Create a new Claude provider.
    ///
    /// - `api_key`: Anthropic API key for `x-api-key` header.
    /// - `base_url`: Override base URL (default: `https://api.anthropic.com`).
    pub fn new(api_key: impl Into<String>, base_url: Option<&str>) -> Self {
        use std::time::Duration;

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build reqwest client");

        Self {
            api_key: api_key.into(),
            base_url: base_url
                .unwrap_or("https://api.anthropic.com")
                .trim_end_matches('/')
                .to_string(),
            client,
        }
    }

    fn build_request_body<'a>(
        &self,
        req: &'a ChatRequest,
        stream_override: Option<bool>,
    ) -> Result<CChatRequest<'a>, ProviderError> {
        let (system, messages) = convert_messages(&req.messages)?;

        let tools: Vec<CToolDef<'a>> = req
            .tools
            .iter()
            .map(|td| CToolDef {
                name: &td.name,
                description: &td.description,
                input_schema: &td.parameters,
            })
            .collect();

        Ok(CChatRequest {
            model: &req.model,
            messages,
            max_tokens: req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            system,
            temperature: req.temperature,
            stream: stream_override.unwrap_or(req.stream),
            tools,
        })
    }

    fn add_auth_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("Content-Type", "application/json")
    }
}

impl LlmProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = self.build_request_body(request, Some(false))?;

        let http_resp = self
            .add_auth_headers(self.client.post(&url))
            .json(&body)
            .send()
            .await
            .map_err(ProviderError::from)?;

        if !http_resp.status().is_success() {
            return Err(map_http_error(http_resp).await);
        }

        let claude_resp: CResponse = http_resp.json().await.map_err(ProviderError::from)?;
        Ok(claude_resp.into())
    }

    async fn chat_stream(&self, request: &ChatRequest) -> Result<ChatStream, ProviderError> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = self.build_request_body(request, Some(true))?;

        let http_resp = self
            .add_auth_headers(self.client.post(&url))
            .json(&body)
            .send()
            .await
            .map_err(ProviderError::from)?;

        if !http_resp.status().is_success() {
            return Err(map_http_error(http_resp).await);
        }

        let byte_stream = http_resp.bytes_stream();
        let buffered_stream = futures::stream::unfold(
            (Box::pin(byte_stream), String::new(), StreamState::new()),
            |(mut stream, mut buf, state)| async move {
                use futures::StreamExt as _;

                let mut current_state = state;
                loop {
                    if let Some(block) = take_next_block(&mut buf) {
                        if let Some((event_type, data)) = parse_sse_block(&block) {
                            if event_type == "message_stop" {
                                return None;
                            }
                            match current_state.process_event(&event_type, &data) {
                                Ok((next_state, Some(delta))) => {
                                    return Some((Ok(delta), (stream, buf, next_state)));
                                }
                                Ok((next_state, None)) => {
                                    current_state = next_state;
                                    continue;
                                }
                                Err(e) => {
                                    return Some((Err(e), (stream, buf, StreamState::new())));
                                }
                            }
                        }
                        continue;
                    }

                    match stream.next().await {
                        Some(Ok(bytes)) => match String::from_utf8(bytes.to_vec()) {
                            Ok(text) => buf.push_str(&text),
                            Err(e) => {
                                return Some((
                                    Err(ProviderError::Deserialize(e.to_string())),
                                    (stream, buf, current_state),
                                ));
                            }
                        },
                        Some(Err(e)) => {
                            return Some((
                                Err(ProviderError::from(e)),
                                (stream, buf, current_state),
                            ));
                        }
                        None => return None,
                    }
                }
            },
        );

        Ok(Box::pin(buffered_stream))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = format!("{}/v1/models", self.base_url);

        let http_resp = self
            .add_auth_headers(self.client.get(&url))
            .send()
            .await
            .map_err(ProviderError::from)?;

        if !http_resp.status().is_success() {
            return Err(map_http_error(http_resp).await);
        }

        let list: CModelList = http_resp.json().await.map_err(ProviderError::from)?;
        Ok(list.data.into_iter().map(Into::into).collect())
    }
}
