//! Data types for the session subsystem.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use xclaw_core::types::{SessionId, SessionKey};

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

/// A single line in a JSONL transcript file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptRecord {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
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

    #[test]
    fn transcript_record_serde_minimal() {
        let rec = TranscriptRecord {
            role: "user".into(),
            content: "hello".into(),
            timestamp: "2026-03-28T10:00:00Z".into(),
            tool_call_id: None,
            tool_name: None,
            metadata: None,
        };
        let json = serde_json::to_string(&rec).unwrap();
        let back: TranscriptRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, "user");
        assert_eq!(back.content, "hello");
    }

    #[test]
    fn transcript_record_serde_full() {
        let mut meta = HashMap::new();
        meta.insert("model".into(), serde_json::json!("gpt-4o"));
        let rec = TranscriptRecord {
            role: "assistant".into(),
            content: "hi".into(),
            timestamp: "2026-03-28T10:01:00Z".into(),
            tool_call_id: Some("tc-1".into()),
            tool_name: Some("file_read".into()),
            metadata: Some(meta),
        };
        let json = serde_json::to_string(&rec).unwrap();
        let back: TranscriptRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tool_call_id.as_deref(), Some("tc-1"));
        assert_eq!(back.tool_name.as_deref(), Some("file_read"));
        assert!(back.metadata.unwrap().contains_key("model"));
    }

    #[test]
    fn transcript_record_skip_none_fields() {
        let rec = TranscriptRecord {
            role: "user".into(),
            content: "test".into(),
            timestamp: "2026-03-28T10:00:00Z".into(),
            tool_call_id: None,
            tool_name: None,
            metadata: None,
        };
        let json = serde_json::to_string(&rec).unwrap();
        assert!(!json.contains("tool_call_id"));
        assert!(!json.contains("tool_name"));
        assert!(!json.contains("metadata"));
    }

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
