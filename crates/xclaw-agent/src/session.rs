//! Session management helpers for the agent layer.
//!
//! Bridges between `xclaw-memory` session/transcript types and
//! `xclaw-provider` message types used by the LLM.

use std::collections::{BTreeSet, HashMap};

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
/// `filter` controls which `ContentBlockKind`s are included during conversion:
/// - Empty set → no filtering (all block types pass through, backward compatible).
/// - Non-empty set → only blocks whose `kind()` is in the set are considered.
///
/// Thinking blocks are always absent from the final LLM-facing text because
/// `text_content()` only collects `Text` variants. When Thinking is excluded
/// via `filter`, the behaviour is identical; the parameter makes the intent
/// explicit at the call site.
pub(crate) fn transcript_to_messages(
    records: &[TranscriptRecord],
    filter: &BTreeSet<ContentBlockKind>,
) -> Vec<Message> {
    records
        .iter()
        .map(|r| record_to_message(r, filter))
        .collect()
}

/// Returns true when `block` should be included given `filter`.
///
/// An empty filter means "include everything".
fn block_passes(block: &ContentBlock, filter: &BTreeSet<ContentBlockKind>) -> bool {
    filter.is_empty() || filter.contains(&block.kind())
}

fn record_to_message(record: &TranscriptRecord, filter: &BTreeSet<ContentBlockKind>) -> Message {
    match &record.role {
        TranscriptRole::User => {
            let text = filtered_text_content(&record.content, filter);
            Message {
                role: Role::User,
                content: Some(text),
                tool_calls: vec![],
                tool_call_id: None,
            }
        }
        TranscriptRole::Assistant => {
            let text = filtered_text_content(&record.content, filter);
            let tool_calls: Vec<ToolCall> = record
                .content
                .iter()
                .filter(|b| block_passes(b, filter))
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

            Message {
                role: Role::Assistant,
                content: if text.is_empty() { None } else { Some(text) },
                tool_calls,
                tool_call_id: None,
            }
        }
        TranscriptRole::Tool => {
            // Extract call_id and content from the first ToolResult block that passes filter
            let (call_id, content) = record
                .content
                .iter()
                .filter(|b| block_passes(b, filter))
                .find_map(|b| match b {
                    ContentBlock::ToolResult {
                        call_id, content, ..
                    } => Some((Some(call_id.clone()), content.clone())),
                    _ => None,
                })
                .unwrap_or_else(|| (None, filtered_text_content(&record.content, filter)));

            Message {
                role: Role::Tool,
                content: Some(content),
                tool_calls: vec![],
                tool_call_id: call_id,
            }
        }
        TranscriptRole::System => {
            let text = filtered_text_content(&record.content, filter);
            Message {
                role: Role::System,
                content: Some(text),
                tool_calls: vec![],
                tool_call_id: None,
            }
        }
        TranscriptRole::Developer => {
            let text = filtered_text_content(&record.content, filter);
            Message {
                role: Role::Developer,
                content: Some(text),
                tool_calls: vec![],
                tool_call_id: None,
            }
        }
    }
}

/// Concatenate text from `Text` blocks that are allowed by `filter`.
///
/// Only `ContentBlock::Text` variants contribute to the output. Other variants
/// (e.g. `Thinking`, `ToolCall`) never produce text here, even when they pass
/// the filter — use dedicated extraction for those block types.
fn filtered_text_content(blocks: &[ContentBlock], filter: &BTreeSet<ContentBlockKind>) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text }
                if filter.is_empty() || filter.contains(&ContentBlockKind::Text) =>
            {
                Some(text.as_str())
            }
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

    // ── transcript_to_messages with filter ─────────────────────────────

    #[test]
    fn empty_filter_returns_all_records() {
        let records = vec![
            user_input_to_transcript("hello"),
            assistant_output_to_transcript("hi there"),
        ];
        let filter = BTreeSet::new();
        let msgs = transcript_to_messages(&records, &filter);
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn filter_text_only_excludes_tool_call_content() {
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
        // Only Text allowed — ToolCall and Thinking should be excluded
        let filter: BTreeSet<ContentBlockKind> = [ContentBlockKind::Text].into_iter().collect();
        let msgs = transcript_to_messages(&[rec], &filter);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_deref(), Some("answer"));
        assert!(msgs[0].tool_calls.is_empty());
    }

    #[test]
    fn filter_thinking_only_returns_empty_text_and_no_tool_calls() {
        let rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "visible".into(),
                },
                ContentBlock::Thinking {
                    text: "private".into(),
                    thinking_id: None,
                },
            ],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        // Only Thinking allowed — Text should be excluded
        let filter: BTreeSet<ContentBlockKind> = [ContentBlockKind::Thinking].into_iter().collect();
        let msgs = transcript_to_messages(&[rec], &filter);
        assert_eq!(msgs.len(), 1);
        // text_content from filtered blocks: Text block excluded, so empty
        assert!(msgs[0].content.is_none() || msgs[0].content.as_deref() == Some(""));
        assert!(msgs[0].tool_calls.is_empty());
    }

    #[test]
    fn filter_tool_call_kind_includes_tool_calls_excludes_text() {
        let rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "preamble".into(),
                },
                ContentBlock::ToolCall {
                    call_id: "c1".into(),
                    name: "file_read".into(),
                    arguments: r#"{"path":"/tmp"}"#.into(),
                },
            ],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let filter: BTreeSet<ContentBlockKind> = [ContentBlockKind::ToolCall].into_iter().collect();
        let msgs = transcript_to_messages(&[rec], &filter);
        assert_eq!(msgs.len(), 1);
        // Text excluded → no text content
        assert!(msgs[0].content.is_none() || msgs[0].content.as_deref() == Some(""));
        // ToolCall included
        assert_eq!(msgs[0].tool_calls.len(), 1);
        assert_eq!(msgs[0].tool_calls[0].id, "c1");
    }

    #[test]
    fn filter_multiple_kinds_text_and_tool_call() {
        let rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "summary".into(),
                },
                ContentBlock::Thinking {
                    text: "hidden".into(),
                    thinking_id: None,
                },
                ContentBlock::ToolCall {
                    call_id: "c2".into(),
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
        let filter: BTreeSet<ContentBlockKind> =
            [ContentBlockKind::Text, ContentBlockKind::ToolCall]
                .into_iter()
                .collect();
        let msgs = transcript_to_messages(&[rec], &filter);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_deref(), Some("summary"));
        assert_eq!(msgs[0].tool_calls.len(), 1);
    }

    #[test]
    fn filter_empty_records_returns_empty() {
        let filter: BTreeSet<ContentBlockKind> = [ContentBlockKind::Text].into_iter().collect();
        let msgs = transcript_to_messages(&[], &filter);
        assert!(msgs.is_empty());
    }

    #[test]
    fn filter_tool_result_in_tool_role_record() {
        let rec = tool_result_to_transcript("call_1", "file_read", "file contents", None);
        // ToolResult kind — should pass through
        let filter: BTreeSet<ContentBlockKind> =
            [ContentBlockKind::ToolResult].into_iter().collect();
        let msgs = transcript_to_messages(&[rec], &filter);
        assert_eq!(msgs[0].role, Role::Tool);
        assert_eq!(msgs[0].content.as_deref(), Some("file contents"));
    }

    #[test]
    fn filter_excludes_tool_result_from_tool_role_record() {
        let rec = tool_result_to_transcript("call_1", "file_read", "file contents", None);
        // Only Text kind — ToolResult should be excluded, fallback to text_content("")
        let filter: BTreeSet<ContentBlockKind> = [ContentBlockKind::Text].into_iter().collect();
        let msgs = transcript_to_messages(&[rec], &filter);
        // Tool role with no ToolResult block found → falls back to empty text_content
        assert_eq!(msgs[0].role, Role::Tool);
        assert!(msgs[0].content.as_deref() == Some("") || msgs[0].content.is_none());
        assert!(msgs[0].tool_call_id.is_none());
    }

    #[test]
    fn converts_user_record_to_user_message() {
        let records = vec![user_input_to_transcript("hello")];
        let msgs = transcript_to_messages(&records, &BTreeSet::new());
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[0].content.as_deref(), Some("hello"));
    }

    #[test]
    fn converts_assistant_record_to_assistant_message() {
        let records = vec![assistant_output_to_transcript("hi there")];
        let msgs = transcript_to_messages(&records, &BTreeSet::new());
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
        let msgs = transcript_to_messages(&[rec], &BTreeSet::new());
        assert_eq!(msgs[0].role, Role::Assistant);
        assert!(msgs[0].content.is_none()); // no text blocks → None
        assert_eq!(msgs[0].tool_calls.len(), 1);
        assert_eq!(msgs[0].tool_calls[0].id, "call_1");
        assert_eq!(msgs[0].tool_calls[0].function.name, "file_read");
    }

    #[test]
    fn converts_tool_record_to_tool_message() {
        let rec = tool_result_to_transcript("call_1", "file_read", "file contents", None);
        let msgs = transcript_to_messages(&[rec], &BTreeSet::new());
        assert_eq!(msgs[0].role, Role::Tool);
        assert_eq!(msgs[0].content.as_deref(), Some("file contents"));
        assert_eq!(msgs[0].tool_call_id.as_deref(), Some("call_1"));
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
        let msgs = transcript_to_messages(&[rec], &BTreeSet::new());
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
        let msgs = transcript_to_messages(&[rec], &BTreeSet::new());
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
        let msgs = transcript_to_messages(&[rec], &BTreeSet::new());
        // text_content only joins Text blocks, skipping Thinking
        assert_eq!(msgs[0].content.as_deref(), Some("the answer"));
    }

    #[test]
    fn empty_records_returns_empty_messages() {
        let msgs = transcript_to_messages(&[], &BTreeSet::new());
        assert!(msgs.is_empty());
    }

    #[test]
    fn multi_turn_conversation_preserves_order() {
        let records = vec![
            user_input_to_transcript("q1"),
            assistant_output_to_transcript("a1"),
            user_input_to_transcript("q2"),
        ];
        let msgs = transcript_to_messages(&records, &BTreeSet::new());
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
