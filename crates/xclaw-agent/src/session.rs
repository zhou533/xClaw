//! Session management helpers for the agent layer.
//!
//! Bridges between `xclaw-memory` session/transcript types and
//! `xclaw-provider` message types used by the LLM.

use std::collections::HashMap;

use xclaw_core::error::XClawError;
use xclaw_core::types::SessionKey;
use xclaw_memory::session::record_id::{RecordId, generate_record_id};
use xclaw_memory::session::types::{
    ContentBlock, ContentBlockKind, StopReason, TokenUsage, TranscriptRecord, TranscriptRole,
};
use xclaw_provider::types::{ChatResponse, FinishReason, Message, Role, ToolCall, Usage};

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

// ─── Type conversions ───────────────────────────────────────────────────────

/// Convert a provider `FinishReason` into a transcript `StopReason`.
fn finish_reason_to_stop_reason(reason: &FinishReason) -> StopReason {
    match reason {
        FinishReason::Stop => StopReason::Stop,
        FinishReason::ToolCalls => StopReason::ToolCalls,
        FinishReason::Length => StopReason::Length,
        FinishReason::ContentFilter => StopReason::ContentFilter,
    }
}

/// Convert a provider `Usage` into a transcript `TokenUsage`.
fn usage_to_token_usage(usage: &Usage) -> TokenUsage {
    TokenUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        thinking_tokens: None,
        cache_read_tokens: None,
    }
}

// ─── transcript_to_messages ─────────────────────────────────────────────────

/// Convert transcript records into provider `Message`s for the LLM.
///
/// Always includes Text, ToolCall, and ToolResult blocks. Excludes Thinking,
/// Image, and Unknown blocks. This is hardcoded — no external filter parameter.
///
/// Drops orphaned `role: tool` messages whose `tool_call_id` has no matching
/// `tool_calls` entry in any preceding `role: assistant` message. This prevents
/// API errors when `transcript_tail` truncates a tool cycle across the boundary.
pub(crate) fn transcript_to_messages(records: &[TranscriptRecord]) -> Vec<Message> {
    let mut messages: Vec<Message> = records.iter().filter_map(record_to_message).collect();
    drop_orphaned_tool_messages(&mut messages);
    messages
}

/// Remove `role: tool` messages whose `tool_call_id` was not declared by any
/// **preceding** `role: assistant` message's `tool_calls`.
///
/// Uses a forward scan so that a tool_call_id declared by a later assistant
/// does not rescue an earlier orphaned tool message.
fn drop_orphaned_tool_messages(messages: &mut Vec<Message>) {
    let mut declared_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut keep = vec![true; messages.len()];

    for (i, m) in messages.iter().enumerate() {
        match m.role {
            Role::Assistant => {
                for tc in &m.tool_calls {
                    declared_ids.insert(tc.id.clone());
                }
            }
            Role::Tool => {
                let passes = m
                    .tool_call_id
                    .as_ref()
                    .is_some_and(|id| declared_ids.contains(id));
                if !passes {
                    keep[i] = false;
                }
            }
            _ => {}
        }
    }

    let mut iter = keep.iter();
    messages.retain(|_| *iter.next().unwrap());
}

/// Returns true when `block` should be included in LLM-facing messages.
///
/// Allowlist: Text, ToolCall, ToolResult.
/// Everything else (Thinking, Image, Unknown, and any future variants) is excluded.
fn block_passes(block: &ContentBlock) -> bool {
    matches!(
        block.kind(),
        ContentBlockKind::Text | ContentBlockKind::ToolCall | ContentBlockKind::ToolResult
    )
}

fn record_to_message(record: &TranscriptRecord) -> Option<Message> {
    match &record.role {
        TranscriptRole::User => {
            let text = text_content(&record.content);
            Some(Message {
                role: Role::User,
                content: Some(text),
                tool_calls: vec![],
                tool_call_id: None,
            })
        }
        TranscriptRole::Assistant => {
            let text = text_content(&record.content);
            let tool_calls: Vec<ToolCall> = record
                .content
                .iter()
                .filter(|b| block_passes(b))
                .filter_map(|b| match b {
                    ContentBlock::ToolCall {
                        call_id,
                        name,
                        arguments,
                    } => Some(ToolCall {
                        id: call_id.clone(),
                        function: xclaw_provider::types::FunctionCall {
                            name: name.clone(),
                            arguments: arguments.clone(),
                        },
                    }),
                    _ => None,
                })
                .collect();

            // Drop assistant message if it has no content at all
            if text.is_empty() && tool_calls.is_empty() {
                return None;
            }

            Some(Message {
                role: Role::Assistant,
                content: if text.is_empty() { None } else { Some(text) },
                tool_calls,
                tool_call_id: None,
            })
        }
        TranscriptRole::Tool => {
            // Extract call_id and content from the first ToolResult block.
            let (call_id, content) =
                record
                    .content
                    .iter()
                    .filter(|b| block_passes(b))
                    .find_map(|b| match b {
                        ContentBlock::ToolResult {
                            call_id, content, ..
                        } => Some((call_id.clone(), content.clone())),
                        _ => None,
                    })?;

            Some(Message {
                role: Role::Tool,
                content: Some(content),
                tool_calls: vec![],
                tool_call_id: Some(call_id),
            })
        }
        TranscriptRole::System => {
            let text = text_content(&record.content);
            Some(Message {
                role: Role::System,
                content: Some(text),
                tool_calls: vec![],
                tool_call_id: None,
            })
        }
        TranscriptRole::Developer => {
            let text = text_content(&record.content);
            Some(Message {
                role: Role::Developer,
                content: Some(text),
                tool_calls: vec![],
                tool_call_id: None,
            })
        }
    }
}

/// Concatenate text from `Text` blocks only.
///
/// Thinking, ToolCall, and other block types never contribute text here.
fn text_content(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

// ─── Record builders ────────────────────────────────────────────────────────

/// Build a `TranscriptRecord` for the user's input message.
pub fn user_input_to_transcript(content: &str) -> TranscriptRecord {
    TranscriptRecord {
        id: generate_record_id(),
        parent_id: None,
        role: TranscriptRole::User,
        content: vec![ContentBlock::Text {
            text: content.to_string(),
        }],
        timestamp: xclaw_memory::session::time_util::now_utc(),
        model: None,
        stop_reason: None,
        usage: None,
        provider: None,
        metadata: HashMap::new(),
    }
}

/// Build a `TranscriptRecord` for the assistant's final text response.
pub fn assistant_output_to_transcript(content: &str) -> TranscriptRecord {
    TranscriptRecord {
        id: generate_record_id(),
        parent_id: None,
        role: TranscriptRole::Assistant,
        content: vec![ContentBlock::Text {
            text: content.to_string(),
        }],
        timestamp: xclaw_memory::session::time_util::now_utc(),
        model: None,
        stop_reason: None,
        usage: None,
        provider: None,
        metadata: HashMap::new(),
    }
}

/// Build a `TranscriptRecord` for a tool result.
pub fn tool_result_to_transcript(
    tool_call_id: &str,
    tool_name: &str,
    output: &str,
    parent_id: Option<&RecordId>,
) -> TranscriptRecord {
    TranscriptRecord {
        id: generate_record_id(),
        parent_id: parent_id.cloned(),
        role: TranscriptRole::Tool,
        content: vec![ContentBlock::ToolResult {
            call_id: tool_call_id.to_string(),
            name: Some(tool_name.to_string()),
            content: output.to_string(),
            is_error: false,
        }],
        timestamp: xclaw_memory::session::time_util::now_utc(),
        model: None,
        stop_reason: None,
        usage: None,
        provider: None,
        metadata: HashMap::new(),
    }
}

/// Build the assistant `TranscriptRecord` from a `ChatResponse`.
///
/// Returns the assistant-turn record only. Tool calls are stored as
/// `ContentBlock::ToolCall` entries in the content array.
pub fn response_to_transcript(response: &ChatResponse) -> Vec<TranscriptRecord> {
    let Some(choice) = response.choices.first() else {
        return vec![];
    };

    let msg = &choice.message;
    let timestamp = xclaw_memory::session::time_util::now_utc();

    let mut content_blocks = Vec::new();

    // Text content
    if let Some(text) = &msg.content
        && !text.is_empty()
    {
        content_blocks.push(ContentBlock::Text { text: text.clone() });
    }

    // Tool calls
    for tc in &msg.tool_calls {
        content_blocks.push(ContentBlock::ToolCall {
            call_id: tc.id.clone(),
            name: tc.function.name.clone(),
            arguments: tc.function.arguments.clone(),
        });
    }

    let stop_reason = choice
        .finish_reason
        .as_ref()
        .map(finish_reason_to_stop_reason);
    let usage = response.usage.as_ref().map(usage_to_token_usage);

    let mut metadata = HashMap::new();
    metadata.insert(
        "provider_message_id".to_string(),
        serde_json::Value::String(response.id.clone()),
    );

    let rec = TranscriptRecord {
        id: generate_record_id(),
        parent_id: None,
        role: TranscriptRole::Assistant,
        content: content_blocks,
        timestamp,
        model: Some(response.model.clone()),
        stop_reason,
        usage,
        provider: None,
        metadata,
    };

    vec![rec]
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use xclaw_core::types::SessionId;
    use xclaw_provider::types::{
        ChatResponse, Choice, FinishReason, FunctionCall, ToolCall, Usage,
    };

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
        assert_eq!(key.scope(), "cli");
    }

    // ── From trait conversions ──────────────────────────────────────────

    #[test]
    fn test_finish_reason_to_stop_reason() {
        assert_eq!(
            finish_reason_to_stop_reason(&FinishReason::Stop),
            StopReason::Stop
        );
        assert_eq!(
            finish_reason_to_stop_reason(&FinishReason::ToolCalls),
            StopReason::ToolCalls
        );
        assert_eq!(
            finish_reason_to_stop_reason(&FinishReason::Length),
            StopReason::Length
        );
        assert_eq!(
            finish_reason_to_stop_reason(&FinishReason::ContentFilter),
            StopReason::ContentFilter
        );
    }

    #[test]
    fn test_usage_to_token_usage() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };
        let tu = usage_to_token_usage(&usage);
        assert_eq!(tu.input_tokens, 100);
        assert_eq!(tu.output_tokens, 50);
        assert_eq!(tu.total_tokens, 150);
        assert!(tu.thinking_tokens.is_none());
        assert!(tu.cache_read_tokens.is_none());
    }

    // ── transcript_to_messages ──────────────────────────────────────────

    #[test]
    fn thinking_blocks_excluded_from_history() {
        let rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    text: "deep thought".into(),
                    thinking_id: None,
                },
                ContentBlock::Text {
                    text: "answer".into(),
                },
                ContentBlock::ToolCall {
                    call_id: "c1".into(),
                    name: "echo".into(),
                    arguments: "{}".into(),
                },
            ],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let msgs = transcript_to_messages(&[rec]);
        assert_eq!(msgs.len(), 1);
        // Text preserved, Thinking excluded
        assert_eq!(msgs[0].content.as_deref(), Some("answer"));
        // ToolCall preserved
        assert_eq!(msgs[0].tool_calls.len(), 1);
        assert_eq!(msgs[0].tool_calls[0].id, "c1");
    }

    #[test]
    fn text_tool_call_tool_result_all_preserved() {
        let user_rec = user_input_to_transcript("run it");
        let assistant_with_tool = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Running…".into(),
                },
                ContentBlock::ToolCall {
                    call_id: "c2".into(),
                    name: "run".into(),
                    arguments: "{}".into(),
                },
            ],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let tool_rec = tool_result_to_transcript("c2", "run", "done", None);

        let records = vec![user_rec, assistant_with_tool, tool_rec];
        let msgs = transcript_to_messages(&records);

        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[1].content.as_deref(), Some("Running…"));
        assert_eq!(msgs[1].tool_calls.len(), 1);
        assert_eq!(msgs[1].tool_calls[0].id, "c2");
        assert_eq!(msgs[2].role, Role::Tool);
        assert_eq!(msgs[2].tool_call_id.as_deref(), Some("c2"));
        assert_eq!(msgs[2].content.as_deref(), Some("done"));
    }

    #[test]
    fn empty_records_returns_empty() {
        let msgs = transcript_to_messages(&[]);
        assert!(msgs.is_empty());
    }

    #[test]
    fn standalone_tool_result_is_orphaned_and_dropped() {
        // A lone tool result with no preceding assistant tool_call is orphaned
        let rec = tool_result_to_transcript("call_1", "file_read", "file contents", None);
        let msgs = transcript_to_messages(&[rec]);
        assert!(msgs.is_empty());
    }

    #[test]
    fn converts_user_record_to_user_message() {
        let records = vec![user_input_to_transcript("hello")];
        let msgs = transcript_to_messages(&records);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].content.as_deref(), Some("hello"));
    }

    #[test]
    fn converts_assistant_record_to_assistant_message() {
        let records = vec![assistant_output_to_transcript("hi there")];
        let msgs = transcript_to_messages(&records);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::Assistant);
        assert_eq!(msgs[0].content.as_deref(), Some("hi there"));
        assert!(msgs[0].tool_calls.is_empty());
    }

    #[test]
    fn converts_assistant_with_tool_calls() {
        let rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![ContentBlock::ToolCall {
                call_id: "call_1".into(),
                name: "file_read".into(),
                arguments: r#"{"path":"/tmp"}"#.into(),
            }],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let msgs = transcript_to_messages(&[rec]);
        assert_eq!(msgs[0].role, Role::Assistant);
        assert!(msgs[0].content.is_none()); // no text blocks → None
        assert_eq!(msgs[0].tool_calls.len(), 1);
        assert_eq!(msgs[0].tool_calls[0].id, "call_1");
        assert_eq!(msgs[0].tool_calls[0].function.name, "file_read");
    }

    #[test]
    fn converts_tool_record_with_matching_assistant() {
        // Tool record is preserved when a preceding assistant declares the tool_call
        let assistant_rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![ContentBlock::ToolCall {
                call_id: "call_1".into(),
                name: "file_read".into(),
                arguments: r#"{"path":"/tmp"}"#.into(),
            }],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let tool_rec = tool_result_to_transcript("call_1", "file_read", "file contents", None);
        let msgs = transcript_to_messages(&[assistant_rec, tool_rec]);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].role, Role::Tool);
        assert_eq!(msgs[1].content.as_deref(), Some("file contents"));
        assert_eq!(msgs[1].tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn converts_system_record_to_system_message() {
        let rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::System,
            content: vec![ContentBlock::Text {
                text: "you are helpful".into(),
            }],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let msgs = transcript_to_messages(&[rec]);
        assert_eq!(msgs[0].role, Role::System);
    }

    #[test]
    fn converts_developer_record_to_developer_message() {
        let rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Developer,
            content: vec![ContentBlock::Text {
                text: "dev note".into(),
            }],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let msgs = transcript_to_messages(&[rec]);
        assert_eq!(msgs[0].role, Role::Developer);
    }

    #[test]
    fn thinking_blocks_filtered_in_replay() {
        let rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    text: "let me think...".into(),
                    thinking_id: None,
                },
                ContentBlock::Text {
                    text: "the answer".into(),
                },
            ],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let msgs = transcript_to_messages(&[rec]);
        assert_eq!(msgs[0].content.as_deref(), Some("the answer"));
    }

    #[test]
    fn empty_records_returns_empty_messages() {
        let msgs = transcript_to_messages(&[]);
        assert!(msgs.is_empty());
    }

    #[test]
    fn multi_turn_conversation_preserves_order() {
        let records = vec![
            user_input_to_transcript("q1"),
            assistant_output_to_transcript("a1"),
            user_input_to_transcript("q2"),
        ];
        let msgs = transcript_to_messages(&records);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[2].role, Role::User);
    }

    // ── orphaned tool message trimming ────────────────────────────────

    #[test]
    fn orphaned_tool_result_at_start_is_dropped() {
        // Simulates transcript_tail cutting a tool cycle in half:
        // the assistant (with ToolCall) was truncated, but its tool result remains.
        let orphaned_tool = tool_result_to_transcript("call_orphan", "echo", "result", None);
        let user_rec = user_input_to_transcript("hello");
        let assistant_rec = assistant_output_to_transcript("hi");

        let records = vec![orphaned_tool, user_rec, assistant_rec];
        let msgs = transcript_to_messages(&records);

        // The orphaned tool message should be dropped
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
    }

    #[test]
    fn multiple_orphaned_tool_results_at_start_are_dropped() {
        // Multiple tool results from a multi-tool call, all orphaned
        let orphan1 = tool_result_to_transcript("call_a", "read", "data1", None);
        let orphan2 = tool_result_to_transcript("call_b", "write", "ok", None);
        let user_rec = user_input_to_transcript("next question");
        let assistant_rec = assistant_output_to_transcript("answer");

        let records = vec![orphan1, orphan2, user_rec, assistant_rec];
        let msgs = transcript_to_messages(&records);

        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
    }

    #[test]
    fn complete_tool_cycle_is_preserved() {
        // Assistant with tool_call + matching tool result → both kept
        let assistant_rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "calling tool".into(),
                },
                ContentBlock::ToolCall {
                    call_id: "call_ok".into(),
                    name: "echo".into(),
                    arguments: "{}".into(),
                },
            ],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let tool_rec = tool_result_to_transcript("call_ok", "echo", "done", None);
        let final_assistant = assistant_output_to_transcript("all done");

        let records = vec![assistant_rec, tool_rec, final_assistant];
        let msgs = transcript_to_messages(&records);

        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, Role::Assistant);
        assert_eq!(msgs[0].tool_calls.len(), 1);
        assert_eq!(msgs[1].role, Role::Tool);
        assert_eq!(msgs[1].tool_call_id.as_deref(), Some("call_ok"));
        assert_eq!(msgs[2].role, Role::Assistant);
    }

    #[test]
    fn orphan_before_complete_cycle_is_dropped() {
        // Orphaned tool result followed by a complete tool cycle
        let orphan = tool_result_to_transcript("call_old", "read", "stale", None);
        let assistant_rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![ContentBlock::ToolCall {
                call_id: "call_new".into(),
                name: "echo".into(),
                arguments: "{}".into(),
            }],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let tool_rec = tool_result_to_transcript("call_new", "echo", "fresh", None);

        let records = vec![orphan, assistant_rec, tool_rec];
        let msgs = transcript_to_messages(&records);

        // Orphan dropped, complete cycle preserved
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::Assistant);
        assert_eq!(msgs[0].tool_calls[0].id, "call_new");
        assert_eq!(msgs[1].role, Role::Tool);
        assert_eq!(msgs[1].tool_call_id.as_deref(), Some("call_new"));
    }

    #[test]
    fn orphan_not_rescued_by_later_assistant_with_same_id() {
        // Orphaned tool result has same call_id as a LATER assistant's tool_call.
        // The orphan must still be dropped (positional correctness).
        let orphan = tool_result_to_transcript("call_x", "echo", "stale", None);
        let user_rec = user_input_to_transcript("next");
        let assistant_rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![ContentBlock::ToolCall {
                call_id: "call_x".into(),
                name: "echo".into(),
                arguments: "{}".into(),
            }],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let tool_rec = tool_result_to_transcript("call_x", "echo", "fresh", None);

        let records = vec![orphan, user_rec, assistant_rec, tool_rec];
        let msgs = transcript_to_messages(&records);

        // Orphan dropped; user + assistant + matched tool preserved
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
        assert_eq!(msgs[2].role, Role::Tool);
        assert_eq!(msgs[2].content.as_deref(), Some("fresh"));
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
        assert_eq!(records[0].role, TranscriptRole::Assistant);
        assert_eq!(records[0].text_content(), "Hello!");
        assert_eq!(records[0].model.as_deref(), Some("gpt-4o"));
        assert_eq!(records[0].stop_reason, Some(StopReason::Stop));
        assert!(!records[0].id.is_empty());
    }

    #[test]
    fn tool_call_response_stores_calls_in_content() {
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
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
        };
        let records = response_to_transcript(&response);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].role, TranscriptRole::Assistant);
        assert!(records[0].has_tool_calls());
        assert_eq!(records[0].stop_reason, Some(StopReason::ToolCalls));

        // Usage should be mapped
        let usage = records[0].usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 20);

        // ToolCall block
        let tc = &records[0].tool_calls();
        assert_eq!(tc.len(), 1);
        assert!(
            matches!(&tc[0], ContentBlock::ToolCall { call_id, name, .. } if call_id == "call_abc" && name == "file_read")
        );
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
        assert_eq!(rec.role, TranscriptRole::User);
        assert_eq!(rec.text_content(), "hello world");
        assert!(!rec.id.is_empty());
        assert!(rec.parent_id.is_none());
        assert!(!rec.timestamp.is_empty());
    }

    // ── tool_result_to_transcript ───────────────────────────────────────

    #[test]
    fn tool_result_creates_tool_record() {
        let rec = tool_result_to_transcript("call_1", "file_read", "file contents", None);
        assert_eq!(rec.role, TranscriptRole::Tool);
        assert!(rec.parent_id.is_none());
        assert!(
            matches!(&rec.content[0], ContentBlock::ToolResult { call_id, name, content, is_error } if call_id == "call_1" && *name == Some("file_read".into()) && content == "file contents" && !is_error)
        );
    }

    #[test]
    fn tool_result_with_parent_id() {
        let parent = "abcd1234".to_string();
        let rec = tool_result_to_transcript("call_1", "echo", "ok", Some(&parent));
        assert_eq!(rec.parent_id.as_deref(), Some("abcd1234"));
    }

    // ── assistant_output_to_transcript ──────────────────────────────────

    #[test]
    fn assistant_output_creates_assistant_record() {
        let rec = assistant_output_to_transcript("Hello there!");
        assert_eq!(rec.role, TranscriptRole::Assistant);
        assert_eq!(rec.text_content(), "Hello there!");
        assert!(!rec.id.is_empty());
        assert!(rec.model.is_none());
        assert!(rec.usage.is_none());
        assert!(!rec.timestamp.is_empty());
    }
}
