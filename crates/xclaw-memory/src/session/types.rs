//! Data types for the session subsystem.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use xclaw_core::types::{SessionId, SessionKey};

use crate::session::record_id::RecordId;

/// A single session entry in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub session_id: SessionId,
    pub session_key: SessionKey,
    pub transcript_path: String,
    pub created_at: String,
    pub updated_at: String,
}

/// The on-disk session index (`sessions.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionIndex {
    pub version: u32,
    pub sessions: Vec<SessionEntry>,
}

impl SessionIndex {
    /// An empty index with version 1.
    pub fn empty() -> Self {
        Self {
            version: 1,
            sessions: Vec::new(),
        }
    }
}

// ─── TranscriptRole ─────────────────────────────────────────────────────────

/// Protocol role of the message sender.
///
/// Aligned with LLM provider role semantics. This is the *protocol role*,
/// not the application-level "Role" from xclaw-memory's role management.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptRole {
    System,
    User,
    Assistant,
    Tool,
    Developer,
}

// ─── ContentBlock ───────────────────────────────────────────────────────────

/// A single content block within a transcript record.
///
/// Modeled as an internally-tagged enum (`"type"` field) for clean JSON:
/// `{"type": "text", "text": "Hello"}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text content.
    Text { text: String },

    /// Model's internal reasoning / chain-of-thought.
    Thinking {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        thinking_id: Option<String>,
    },

    /// Model requests to invoke a tool / function.
    ToolCall {
        /// Tool call ID assigned by the model.
        call_id: String,
        /// Tool / function name.
        name: String,
        /// Raw JSON arguments string.
        arguments: String,
    },

    /// Result returned from executing a tool.
    ToolResult {
        /// Matches the `call_id` of the corresponding ToolCall.
        call_id: String,
        /// Tool / function name (for display and filtering).
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Tool output content.
        content: String,
        /// Whether the tool execution resulted in an error.
        #[serde(default, skip_serializing_if = "is_false")]
        is_error: bool,
    },

    /// Image content (base64-encoded or URL reference).
    Image {
        /// MIME type, e.g. "image/png".
        media_type: String,
        /// Image source.
        source: ImageSource,
    },

    /// Catch-all for provider-specific blocks not yet modeled.
    Unknown {
        /// The original `type` string from the provider.
        original_type: String,
        /// Raw JSON data preserved as a string.
        data: String,
    },
}

/// Helper for `skip_serializing_if` on bool fields.
fn is_false(v: &bool) -> bool {
    !v
}

// ─── ImageSource ────────────────────────────────────────────────────────────

/// Image source representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64-encoded image data.
    Base64 { data: String },
    /// External URL reference.
    Url { url: String },
}

// ─── TokenUsage ─────────────────────────────────────────────────────────────

/// Token usage statistics for a single LLM interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
}

// ─── StopReason ─────────────────────────────────────────────────────────────

/// Reason the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Natural end of response.
    Stop,
    /// Model wants to call one or more tools.
    ToolCalls,
    /// Hit max_tokens / output length limit.
    Length,
    /// Content was filtered by safety systems.
    ContentFilter,
    /// Unknown / provider-specific reason.
    Other(String),
}

// ─── TranscriptRecord ───────────────────────────────────────────────────────

/// A single record in a JSONL transcript file.
///
/// Each record represents one message turn in a conversation.
/// Records form a linked list via `id` / `parent_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptRecord {
    // ── Identity ──
    /// Unique identifier (8 alphanumeric chars, nanoid).
    pub id: RecordId,

    /// Parent record identifier. Forms a reply chain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<RecordId>,

    // ── Message ──
    /// Protocol role of the message sender.
    pub role: TranscriptRole,

    /// Ordered content blocks.
    pub content: Vec<ContentBlock>,

    /// ISO 8601 timestamp.
    pub timestamp: String,

    // ── Model metadata (populated for assistant turns) ──
    /// Model identifier that generated this response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Why the model stopped generating.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,

    /// Token usage for this interaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,

    // ── Provider lineage ──
    /// Provider name that handled this message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    // ── Extensibility ──
    /// Arbitrary key-value metadata for future extensions.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

// ─── Convenience methods ────────────────────────────────────────────────────

impl TranscriptRecord {
    /// Extract concatenated text from all `Text` content blocks.
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>()
    }

    /// Return all `ToolCall` content blocks.
    pub fn tool_calls(&self) -> Vec<&ContentBlock> {
        self.content
            .iter()
            .filter(|b| matches!(b, ContentBlock::ToolCall { .. }))
            .collect()
    }

    /// Whether this record contains any tool calls.
    pub fn has_tool_calls(&self) -> bool {
        self.content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolCall { .. }))
    }
}

/// Summary statistics for a session (used by Agent layer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub session_key: SessionKey,
    pub message_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_message_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_at: Option<String>,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::record_id::generate_record_id;

    // ── SessionIndex / SessionEntry (unchanged) ─────────────────────────

    #[test]
    fn session_index_empty() {
        let idx = SessionIndex::empty();
        assert_eq!(idx.version, 1);
        assert!(idx.sessions.is_empty());
    }

    #[test]
    fn session_entry_serde_roundtrip() {
        let entry = SessionEntry {
            session_id: SessionId::new("sess-1"),
            session_key: SessionKey::parse("default:cli").unwrap(),
            transcript_path: "sess-1.jsonl".into(),
            created_at: "2026-03-28T10:00:00Z".into(),
            updated_at: "2026-03-28T10:00:00Z".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: SessionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id.as_str(), "sess-1");
        assert_eq!(back.session_key.scope(), "cli");
    }

    // ── TranscriptRole ──────────────────────────────────────────────────

    #[test]
    fn transcript_role_serde_roundtrip() {
        for (role, expected_json) in [
            (TranscriptRole::System, "\"system\""),
            (TranscriptRole::User, "\"user\""),
            (TranscriptRole::Assistant, "\"assistant\""),
            (TranscriptRole::Tool, "\"tool\""),
            (TranscriptRole::Developer, "\"developer\""),
        ] {
            let json = serde_json::to_string(&role).unwrap();
            assert_eq!(json, expected_json);
            let back: TranscriptRole = serde_json::from_str(&json).unwrap();
            assert_eq!(back, role);
        }
    }

    // ── ContentBlock ────────────────────────────────────────────────────

    #[test]
    fn content_block_text_serde_roundtrip() {
        let block = ContentBlock::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"text""#));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ContentBlock::Text { text } if text == "hello"));
    }

    #[test]
    fn content_block_thinking_serde_roundtrip() {
        let block = ContentBlock::Thinking {
            text: "let me think...".into(),
            thinking_id: Some("tk_123".into()),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"thinking""#));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(back, ContentBlock::Thinking { text, thinking_id } if text == "let me think..." && thinking_id == Some("tk_123".into()))
        );
    }

    #[test]
    fn content_block_thinking_without_id() {
        let block = ContentBlock::Thinking {
            text: "hmm".into(),
            thinking_id: None,
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(!json.contains("thinking_id"));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(back, ContentBlock::Thinking { thinking_id, .. } if thinking_id.is_none())
        );
    }

    #[test]
    fn content_block_tool_call_serde_roundtrip() {
        let block = ContentBlock::ToolCall {
            call_id: "call_abc".into(),
            name: "file_read".into(),
            arguments: r#"{"path":"/tmp"}"#.into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"tool_call""#));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(back, ContentBlock::ToolCall { call_id, name, arguments } if call_id == "call_abc" && name == "file_read" && arguments == r#"{"path":"/tmp"}"#)
        );
    }

    #[test]
    fn content_block_tool_result_serde_roundtrip() {
        let block = ContentBlock::ToolResult {
            call_id: "call_abc".into(),
            name: Some("file_read".into()),
            content: "file contents".into(),
            is_error: false,
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"tool_result""#));
        // is_error=false should be skipped
        assert!(!json.contains("is_error"));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(&back, ContentBlock::ToolResult { call_id, name, content, is_error } if call_id == "call_abc" && *name == Some("file_read".into()) && content == "file contents" && !is_error)
        );
    }

    #[test]
    fn content_block_tool_result_with_error() {
        let block = ContentBlock::ToolResult {
            call_id: "call_err".into(),
            name: None,
            content: "not found".into(),
            is_error: true,
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("is_error"));
        assert!(!json.contains("\"name\""));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ContentBlock::ToolResult { is_error, .. } if is_error));
    }

    #[test]
    fn content_block_image_serde_roundtrip() {
        let block = ContentBlock::Image {
            media_type: "image/png".into(),
            source: ImageSource::Base64 {
                data: "abc123".into(),
            },
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"image""#));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(back, ContentBlock::Image { media_type, .. } if media_type == "image/png")
        );
    }

    #[test]
    fn content_block_image_url_source() {
        let block = ContentBlock::Image {
            media_type: "image/jpeg".into(),
            source: ImageSource::Url {
                url: "https://example.com/img.jpg".into(),
            },
        };
        let json = serde_json::to_string(&block).unwrap();
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(&back, ContentBlock::Image { source: ImageSource::Url { url }, .. } if url == "https://example.com/img.jpg")
        );
    }

    #[test]
    fn content_block_unknown_serde_roundtrip() {
        let block = ContentBlock::Unknown {
            original_type: "audio".into(),
            data: r#"{"format":"wav"}"#.into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains(r#""type":"unknown""#));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert!(
            matches!(&back, ContentBlock::Unknown { original_type, data } if original_type == "audio" && data.contains("wav"))
        );
    }

    // ── TokenUsage ──────────────────────────────────────────────────────

    #[test]
    fn token_usage_serde_roundtrip() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            thinking_tokens: Some(20),
            cache_read_tokens: None,
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("thinking_tokens"));
        assert!(!json.contains("cache_read_tokens"));
        let back: TokenUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.input_tokens, 100);
        assert_eq!(back.thinking_tokens, Some(20));
        assert!(back.cache_read_tokens.is_none());
    }

    // ── StopReason ──────────────────────────────────────────────────────

    #[test]
    fn stop_reason_serde_roundtrip() {
        for (reason, expected) in [
            (StopReason::Stop, "\"stop\""),
            (StopReason::ToolCalls, "\"tool_calls\""),
            (StopReason::Length, "\"length\""),
            (StopReason::ContentFilter, "\"content_filter\""),
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(json, expected);
            let back: StopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(back, reason);
        }
    }

    #[test]
    fn stop_reason_other_serde_roundtrip() {
        let reason = StopReason::Other("custom_reason".into());
        let json = serde_json::to_string(&reason).unwrap();
        let back: StopReason = serde_json::from_str(&json).unwrap();
        assert_eq!(back, reason);
    }

    // ── TranscriptRecord ────────────────────────────────────────────────

    #[test]
    fn transcript_record_serde_minimal() {
        let rec = TranscriptRecord {
            id: generate_record_id(),
            parent_id: None,
            role: TranscriptRole::User,
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
            timestamp: "2026-03-28T10:00:00Z".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&rec).unwrap();
        // Optional None fields should be skipped
        assert!(!json.contains("parent_id"));
        assert!(!json.contains("\"model\""));
        assert!(!json.contains("stop_reason"));
        assert!(!json.contains("usage"));
        assert!(!json.contains("provider"));
        assert!(!json.contains("metadata"));

        let back: TranscriptRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, TranscriptRole::User);
        assert_eq!(back.text_content(), "hello");
    }

    #[test]
    fn transcript_record_serde_full() {
        let mut meta = HashMap::new();
        meta.insert("provider_message_id".into(), serde_json::json!("resp-123"));

        let rec = TranscriptRecord {
            id: "abcd1234".into(),
            parent_id: Some("wxyz5678".into()),
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Text { text: "hi".into() },
                ContentBlock::ToolCall {
                    call_id: "call_1".into(),
                    name: "file_read".into(),
                    arguments: "{}".into(),
                },
            ],
            timestamp: "2026-03-28T10:01:00Z".into(),
            model: Some("gpt-4o".into()),
            stop_reason: Some(StopReason::ToolCalls),
            usage: Some(TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                total_tokens: 150,
                thinking_tokens: None,
                cache_read_tokens: None,
            }),
            provider: Some("openai".into()),
            metadata: meta,
        };
        let json = serde_json::to_string(&rec).unwrap();
        let back: TranscriptRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "abcd1234");
        assert_eq!(back.parent_id.as_deref(), Some("wxyz5678"));
        assert_eq!(back.role, TranscriptRole::Assistant);
        assert_eq!(back.content.len(), 2);
        assert_eq!(back.model.as_deref(), Some("gpt-4o"));
        assert_eq!(back.stop_reason, Some(StopReason::ToolCalls));
        assert!(back.usage.is_some());
        assert_eq!(back.provider.as_deref(), Some("openai"));
        assert!(back.metadata.contains_key("provider_message_id"));
    }

    // ── Convenience methods ─────────────────────────────────────────────

    #[test]
    fn text_content_joins_text_blocks() {
        let rec = TranscriptRecord {
            id: "test1234".into(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Hello ".into(),
                },
                ContentBlock::Thinking {
                    text: "hmm".into(),
                    thinking_id: None,
                },
                ContentBlock::Text {
                    text: "world".into(),
                },
            ],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        assert_eq!(rec.text_content(), "Hello world");
    }

    #[test]
    fn tool_calls_filters_correctly() {
        let rec = TranscriptRecord {
            id: "test1234".into(),
            parent_id: None,
            role: TranscriptRole::Assistant,
            content: vec![
                ContentBlock::Text { text: "ok".into() },
                ContentBlock::ToolCall {
                    call_id: "c1".into(),
                    name: "echo".into(),
                    arguments: "{}".into(),
                },
                ContentBlock::ToolCall {
                    call_id: "c2".into(),
                    name: "read".into(),
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
        assert!(rec.has_tool_calls());
        assert_eq!(rec.tool_calls().len(), 2);
    }

    #[test]
    fn no_tool_calls() {
        let rec = TranscriptRecord {
            id: "test1234".into(),
            parent_id: None,
            role: TranscriptRole::User,
            content: vec![ContentBlock::Text { text: "hi".into() }],
            timestamp: "t".into(),
            model: None,
            stop_reason: None,
            usage: None,
            provider: None,
            metadata: HashMap::new(),
        };
        assert!(!rec.has_tool_calls());
        assert!(rec.tool_calls().is_empty());
    }

    // ── SessionSummary ──────────────────────────────────────────────────

    #[test]
    fn session_summary_serde_roundtrip() {
        let summary = SessionSummary {
            session_id: SessionId::new("sess-1"),
            session_key: SessionKey::parse("default:cli").unwrap(),
            message_count: 42,
            first_message_at: Some("2026-03-28T10:00:00Z".into()),
            last_message_at: Some("2026-03-28T11:00:00Z".into()),
        };
        let json = serde_json::to_string(&summary).unwrap();
        let back: SessionSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.message_count, 42);
        assert_eq!(
            back.first_message_at.as_deref(),
            Some("2026-03-28T10:00:00Z")
        );
    }
}
