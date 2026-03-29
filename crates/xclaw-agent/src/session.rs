//! Session management helpers for the agent layer.
//!
//! Bridges between `xclaw-memory` session/transcript types and
//! `xclaw-provider` message types used by the LLM.

use std::collections::HashMap;

use xclaw_core::error::XClawError;
use xclaw_core::types::SessionKey;
use xclaw_memory::session::types::TranscriptRecord;
use xclaw_provider::types::{ChatResponse, FinishReason, Message, Role, ToolCall};

use crate::traits::UserInput;

/// Derive a `SessionKey` from a `UserInput`.
///
/// When `scope` is `None` or empty, defaults to `"cli"`.
/// Pass a custom scope (e.g. `"repl-{uuid}"`) for REPL sessions.
pub fn resolve_session_key(
    _input: &UserInput,
    scope: Option<&str>,
) -> Result<SessionKey, XClawError> {
    let effective_scope = match scope {
        Some(s) if !s.is_empty() => s,
        _ => "cli",
    };
    let raw = format!("default:{effective_scope}");
    SessionKey::parse(&raw)
}

/// Convert transcript records into provider `Message`s for the LLM.
///
/// Maps transcript roles:
/// - `"user"` → `Role::User`
/// - `"assistant"` → `Role::Assistant` (with optional tool_calls from metadata)
/// - `"tool"` → `Role::Tool` (with tool_call_id)
/// - anything else → `Role::User` (fallback)
pub fn transcript_to_messages(records: &[TranscriptRecord]) -> Vec<Message> {
    records.iter().map(record_to_message).collect()
}

fn record_to_message(record: &TranscriptRecord) -> Message {
    match record.role.as_str() {
        "user" => Message {
            role: Role::User,
            content: Some(record.content.clone()),
            tool_calls: vec![],
            tool_call_id: None,
        },
        "assistant" => {
            let tool_calls = extract_tool_calls_from_metadata(&record.metadata);
            Message {
                role: Role::Assistant,
                content: if record.content.is_empty() {
                    None
                } else {
                    Some(record.content.clone())
                },
                tool_calls,
                tool_call_id: None,
            }
        }
        "tool" => Message {
            role: Role::Tool,
            content: Some(record.content.clone()),
            tool_calls: vec![],
            tool_call_id: record.tool_call_id.clone(),
        },
        _ => Message {
            role: Role::User,
            content: Some(record.content.clone()),
            tool_calls: vec![],
            tool_call_id: None,
        },
    }
}

/// Extract tool calls from transcript metadata, if present.
///
/// Looks for `metadata["tool_calls"]` as a JSON array of `ToolCall` objects.
fn extract_tool_calls_from_metadata(
    metadata: &Option<HashMap<String, serde_json::Value>>,
) -> Vec<ToolCall> {
    let Some(meta) = metadata else {
        return vec![];
    };
    let Some(tc_value) = meta.get("tool_calls") else {
        return vec![];
    };
    serde_json::from_value::<Vec<ToolCall>>(tc_value.clone()).unwrap_or_default()
}

/// Build the assistant `TranscriptRecord` from a `ChatResponse`.
///
/// Returns the assistant-turn record only. If the response triggered tool
/// calls, they are stored in `metadata["tool_calls"]`. Tool result records
/// must be built separately via `tool_result_to_transcript`.
pub fn response_to_transcript(response: &ChatResponse) -> Vec<TranscriptRecord> {
    let Some(choice) = response.choices.first() else {
        return vec![];
    };

    let msg = &choice.message;
    let timestamp = unix_secs_now();

    let mut records = Vec::new();

    // Assistant message record
    let has_tool_calls = !msg.tool_calls.is_empty();
    let metadata = if has_tool_calls {
        let tc_json = serde_json::to_value(&msg.tool_calls).ok();
        tc_json.map(|v| {
            let mut m = HashMap::new();
            m.insert("tool_calls".to_string(), v);
            if let Some(FinishReason::ToolCalls) = &choice.finish_reason {
                m.insert(
                    "finish_reason".to_string(),
                    serde_json::Value::String("tool_calls".to_string()),
                );
            }
            m
        })
    } else {
        None
    };

    records.push(TranscriptRecord {
        role: "assistant".to_string(),
        content: msg.content.clone().unwrap_or_default(),
        timestamp: timestamp.clone(),
        tool_call_id: None,
        tool_name: None,
        metadata,
    });

    records
}

/// Build a `TranscriptRecord` for the user's input message.
pub fn user_input_to_transcript(content: &str) -> TranscriptRecord {
    TranscriptRecord {
        role: "user".to_string(),
        content: content.to_string(),
        timestamp: unix_secs_now(),
        tool_call_id: None,
        tool_name: None,
        metadata: None,
    }
}

/// Build a `TranscriptRecord` for a tool result.
pub fn tool_result_to_transcript(
    tool_call_id: &str,
    tool_name: &str,
    output: &str,
) -> TranscriptRecord {
    TranscriptRecord {
        role: "tool".to_string(),
        content: output.to_string(),
        timestamp: unix_secs_now(),
        tool_call_id: Some(tool_call_id.to_string()),
        tool_name: Some(tool_name.to_string()),
        metadata: None,
    }
}

/// Build a `TranscriptRecord` for the assistant's final text response.
pub fn assistant_output_to_transcript(content: &str) -> TranscriptRecord {
    TranscriptRecord {
        role: "assistant".to_string(),
        content: content.to_string(),
        timestamp: unix_secs_now(),
        tool_call_id: None,
        tool_name: None,
        metadata: None,
    }
}

/// Unix epoch seconds as a string timestamp.
///
/// Not ISO 8601 — produces `"1743292800"` not `"2026-03-29T00:00:00Z"`.
/// Sufficient for ordering within transcripts. A future improvement
/// could format as proper ISO 8601 via `chrono` or a shared utility.
fn unix_secs_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use xclaw_core::types::SessionId;
    use xclaw_provider::types::{ChatResponse, Choice, FinishReason, FunctionCall, ToolCall};

    // ── resolve_session_key ─────────────────────────────────────────────

    #[test]
    fn resolve_session_key_returns_default_cli() {
        let input = UserInput {
            session_id: SessionId::new("s1"),
            content: "hello".to_string(),
        };
        let key = resolve_session_key(&input, None).unwrap();
        assert_eq!(key.scope(), "cli");
    }

    #[test]
    fn resolve_session_key_with_custom_scope() {
        let input = UserInput {
            session_id: SessionId::new("s1"),
            content: "hello".to_string(),
        };
        let key = resolve_session_key(&input, Some("repl-abc123")).unwrap();
        assert_eq!(key.scope(), "repl-abc123");
    }

    #[test]
    fn resolve_session_key_with_empty_scope_uses_default() {
        let input = UserInput {
            session_id: SessionId::new("s1"),
            content: "hello".to_string(),
        };
        let key = resolve_session_key(&input, Some("")).unwrap();
        // Empty scope should fall back to default
        assert_eq!(key.scope(), "cli");
    }

    // ── transcript_to_messages ──────────────────────────────────────────

    #[test]
    fn converts_user_record_to_user_message() {
        let records = vec![TranscriptRecord {
            role: "user".into(),
            content: "hello".into(),
            timestamp: "12345".into(),
            tool_call_id: None,
            tool_name: None,
            metadata: None,
        }];
        let msgs = transcript_to_messages(&records);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].content.as_deref(), Some("hello"));
    }

    #[test]
    fn converts_assistant_record_to_assistant_message() {
        let records = vec![TranscriptRecord {
            role: "assistant".into(),
            content: "hi there".into(),
            timestamp: "12345".into(),
            tool_call_id: None,
            tool_name: None,
            metadata: None,
        }];
        let msgs = transcript_to_messages(&records);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::Assistant);
        assert_eq!(msgs[0].content.as_deref(), Some("hi there"));
        assert!(msgs[0].tool_calls.is_empty());
    }

    #[test]
    fn converts_assistant_with_tool_calls_in_metadata() {
        let tc = serde_json::json!([{
            "id": "call_1",
            "function": {"name": "file_read", "arguments": "{\"path\":\"/tmp\"}"}
        }]);
        let mut meta = HashMap::new();
        meta.insert("tool_calls".to_string(), tc);

        let records = vec![TranscriptRecord {
            role: "assistant".into(),
            content: String::new(),
            timestamp: "12345".into(),
            tool_call_id: None,
            tool_name: None,
            metadata: Some(meta),
        }];
        let msgs = transcript_to_messages(&records);
        assert_eq!(msgs[0].role, Role::Assistant);
        assert!(msgs[0].content.is_none()); // empty content → None
        assert_eq!(msgs[0].tool_calls.len(), 1);
        assert_eq!(msgs[0].tool_calls[0].id, "call_1");
        assert_eq!(msgs[0].tool_calls[0].function.name, "file_read");
    }

    #[test]
    fn converts_tool_record_to_tool_message() {
        let records = vec![TranscriptRecord {
            role: "tool".into(),
            content: "file contents here".into(),
            timestamp: "12345".into(),
            tool_call_id: Some("call_1".into()),
            tool_name: Some("file_read".into()),
            metadata: None,
        }];
        let msgs = transcript_to_messages(&records);
        assert_eq!(msgs[0].role, Role::Tool);
        assert_eq!(msgs[0].content.as_deref(), Some("file contents here"));
        assert_eq!(msgs[0].tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn unknown_role_falls_back_to_user() {
        let records = vec![TranscriptRecord {
            role: "system".into(),
            content: "something".into(),
            timestamp: "12345".into(),
            tool_call_id: None,
            tool_name: None,
            metadata: None,
        }];
        let msgs = transcript_to_messages(&records);
        assert_eq!(msgs[0].role, Role::User);
    }

    #[test]
    fn empty_records_returns_empty_messages() {
        let msgs = transcript_to_messages(&[]);
        assert!(msgs.is_empty());
    }

    #[test]
    fn multi_turn_conversation_preserves_order() {
        let records = vec![
            TranscriptRecord {
                role: "user".into(),
                content: "q1".into(),
                timestamp: "1".into(),
                tool_call_id: None,
                tool_name: None,
                metadata: None,
            },
            TranscriptRecord {
                role: "assistant".into(),
                content: "a1".into(),
                timestamp: "2".into(),
                tool_call_id: None,
                tool_name: None,
                metadata: None,
            },
            TranscriptRecord {
                role: "user".into(),
                content: "q2".into(),
                timestamp: "3".into(),
                tool_call_id: None,
                tool_name: None,
                metadata: None,
            },
        ];
        let msgs = transcript_to_messages(&records);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[2].role, Role::User);
    }

    // ── response_to_transcript ──────────────────────────────────────────

    #[test]
    fn text_response_produces_one_record() {
        let response = ChatResponse {
            id: "resp-1".into(),
            model: "gpt-4o".into(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: Role::Assistant,
                    content: Some("Hello!".into()),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
                finish_reason: Some(FinishReason::Stop),
            }],
            usage: None,
        };
        let records = response_to_transcript(&response);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].role, "assistant");
        assert_eq!(records[0].content, "Hello!");
        assert!(records[0].metadata.is_none());
    }

    #[test]
    fn tool_call_response_stores_calls_in_metadata() {
        let response = ChatResponse {
            id: "resp-2".into(),
            model: "gpt-4o".into(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: Role::Assistant,
                    content: None,
                    tool_calls: vec![ToolCall {
                        id: "call_abc".into(),
                        function: FunctionCall {
                            name: "file_read".into(),
                            arguments: r#"{"path":"/tmp"}"#.into(),
                        },
                    }],
                    tool_call_id: None,
                },
                finish_reason: Some(FinishReason::ToolCalls),
            }],
            usage: None,
        };
        let records = response_to_transcript(&response);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].role, "assistant");
        assert_eq!(records[0].content, ""); // None → empty

        let meta = records[0].metadata.as_ref().unwrap();
        assert!(meta.contains_key("tool_calls"));
        assert!(meta.contains_key("finish_reason"));

        // Verify tool_calls can be deserialized back
        let tc: Vec<ToolCall> = serde_json::from_value(meta["tool_calls"].clone()).unwrap();
        assert_eq!(tc.len(), 1);
        assert_eq!(tc[0].id, "call_abc");
    }

    #[test]
    fn empty_choices_returns_empty() {
        let response = ChatResponse {
            id: "resp-3".into(),
            model: "gpt-4o".into(),
            choices: vec![],
            usage: None,
        };
        let records = response_to_transcript(&response);
        assert!(records.is_empty());
    }

    // ── user_input_to_transcript ────────────────────────────────────────

    #[test]
    fn user_input_creates_user_record() {
        let rec = user_input_to_transcript("hello world");
        assert_eq!(rec.role, "user");
        assert_eq!(rec.content, "hello world");
        assert!(rec.tool_call_id.is_none());
        assert!(rec.tool_name.is_none());
        assert!(!rec.timestamp.is_empty());
    }

    // ── tool_result_to_transcript ───────────────────────────────────────

    #[test]
    fn tool_result_creates_tool_record() {
        let rec = tool_result_to_transcript("call_1", "file_read", "file contents");
        assert_eq!(rec.role, "tool");
        assert_eq!(rec.content, "file contents");
        assert_eq!(rec.tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(rec.tool_name.as_deref(), Some("file_read"));
    }

    // ── assistant_output_to_transcript ──────────────────────────────────

    #[test]
    fn assistant_output_creates_assistant_record() {
        let rec = assistant_output_to_transcript("Hello there!");
        assert_eq!(rec.role, "assistant");
        assert_eq!(rec.content, "Hello there!");
        assert!(rec.tool_call_id.is_none());
        assert!(rec.tool_name.is_none());
        assert!(rec.metadata.is_none());
        assert!(!rec.timestamp.is_empty());
    }
}
