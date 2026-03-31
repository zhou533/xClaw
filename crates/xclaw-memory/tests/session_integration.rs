//! Integration tests for the Session subsystem via the MemorySystem facade.

use std::collections::HashMap;

use xclaw_core::types::{RoleId, SessionKey};
use xclaw_memory::facade::FsMemorySystem;
use xclaw_memory::session::record_id::generate_record_id;
use xclaw_memory::session::store::SessionStore;
use xclaw_memory::session::types::{ContentBlock, TranscriptRecord, TranscriptRole};

fn setup() -> (tempfile::TempDir, FsMemorySystem) {
    let tmp = tempfile::TempDir::new().unwrap();
    let mem = FsMemorySystem::fs(tmp.path());
    (tmp, mem)
}

fn make_record(role: TranscriptRole, content: &str) -> TranscriptRecord {
    TranscriptRecord {
        id: generate_record_id(),
        parent_id: None,
        role,
        content: vec![ContentBlock::Text {
            text: content.into(),
        }],
        timestamp: "2026-03-28T10:00:00Z".into(),
        model: None,
        stop_reason: None,
        usage: None,
        provider: None,
        metadata: HashMap::new(),
    }
}

// ─── Session via Facade ─────────────────────────────────────────────────────

#[tokio::test]
async fn session_create_and_retrieve_via_facade() {
    let (_tmp, mem) = setup();
    let key = SessionKey::parse("default:cli").unwrap();

    let created = mem.sessions.get_or_create(&key).await.unwrap();
    assert_eq!(created.session_key.scope(), "cli");

    // Retrieve same session
    let found = mem.sessions.get_or_create(&key).await.unwrap();
    assert_eq!(found.session_id.as_str(), created.session_id.as_str());
}

#[tokio::test]
async fn session_transcript_append_and_load() {
    let (_tmp, mem) = setup();
    let key = SessionKey::parse("default:cli").unwrap();
    let entry = mem.sessions.get_or_create(&key).await.unwrap();
    let role_id = key.role_id().clone();

    mem.sessions
        .append_transcript(
            &role_id,
            &entry.session_id,
            &make_record(TranscriptRole::User, "hello"),
        )
        .await
        .unwrap();
    mem.sessions
        .append_transcript(
            &role_id,
            &entry.session_id,
            &make_record(TranscriptRole::Assistant, "hi there"),
        )
        .await
        .unwrap();

    let transcript = mem
        .sessions
        .load_transcript(&role_id, &entry.session_id)
        .await
        .unwrap();
    assert_eq!(transcript.len(), 2);
    assert_eq!(transcript[0].text_content(), "hello");
    assert_eq!(transcript[1].text_content(), "hi there");
}

#[tokio::test]
async fn session_summary_via_facade() {
    let (_tmp, mem) = setup();
    let key = SessionKey::parse("default:cli").unwrap();
    let entry = mem.sessions.get_or_create(&key).await.unwrap();
    let role_id = key.role_id().clone();

    mem.sessions
        .append_transcript(
            &role_id,
            &entry.session_id,
            &make_record(TranscriptRole::User, "q1"),
        )
        .await
        .unwrap();
    mem.sessions
        .append_transcript(
            &role_id,
            &entry.session_id,
            &make_record(TranscriptRole::Assistant, "a1"),
        )
        .await
        .unwrap();
    mem.sessions
        .append_transcript(
            &role_id,
            &entry.session_id,
            &make_record(TranscriptRole::User, "q2"),
        )
        .await
        .unwrap();

    let summary = mem
        .sessions
        .session_summary(&role_id, &entry.session_id)
        .await
        .unwrap();
    assert_eq!(summary.message_count, 3);
    assert!(summary.first_message_at.is_some());
    assert!(summary.last_message_at.is_some());
}

#[tokio::test]
async fn session_delete_via_facade() {
    let (_tmp, mem) = setup();
    let key = SessionKey::parse("default:cli").unwrap();
    let entry = mem.sessions.get_or_create(&key).await.unwrap();
    let role_id = key.role_id().clone();

    mem.sessions
        .append_transcript(
            &role_id,
            &entry.session_id,
            &make_record(TranscriptRole::User, "bye"),
        )
        .await
        .unwrap();

    mem.sessions
        .delete_session(&role_id, &entry.session_id)
        .await
        .unwrap();

    let found = mem
        .sessions
        .get_by_id(&role_id, &entry.session_id)
        .await
        .unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn session_index_file_exists() {
    let (tmp, mem) = setup();
    let key = SessionKey::parse("default:cli").unwrap();
    mem.sessions.get_or_create(&key).await.unwrap();

    let index_path = tmp.path().join("roles/default/sessions/sessions.json");
    assert!(index_path.exists());
}

#[tokio::test]
async fn session_transcript_jsonl_file_readable() {
    let (tmp, mem) = setup();
    let key = SessionKey::parse("default:cli").unwrap();
    let entry = mem.sessions.get_or_create(&key).await.unwrap();
    let role_id = key.role_id().clone();

    mem.sessions
        .append_transcript(
            &role_id,
            &entry.session_id,
            &make_record(TranscriptRole::User, "test"),
        )
        .await
        .unwrap();

    let jsonl_path = tmp.path().join(format!(
        "roles/default/sessions/{}.jsonl",
        entry.session_id.as_str()
    ));
    let raw = std::fs::read_to_string(&jsonl_path).unwrap();
    let parsed: TranscriptRecord = serde_json::from_str(raw.trim()).unwrap();
    assert_eq!(parsed.text_content(), "test");
}

#[tokio::test]
async fn session_with_role_workflow() {
    let (_tmp, mem) = setup();
    mem.ensure_default_role().await.unwrap();

    let key = SessionKey::parse("default:workspace").unwrap();
    let entry = mem.sessions.get_or_create(&key).await.unwrap();
    assert_eq!(entry.session_key.scope(), "workspace");

    let role_id = RoleId::default();
    let sessions = mem.sessions.list_sessions(&role_id).await.unwrap();
    assert_eq!(sessions.len(), 1);
}

#[tokio::test]
async fn multiple_sessions_same_role() {
    let (_tmp, mem) = setup();
    let key1 = SessionKey::parse("default:cli").unwrap();
    let key2 = SessionKey::parse("default:telegram").unwrap();

    mem.sessions.get_or_create(&key1).await.unwrap();
    mem.sessions.get_or_create(&key2).await.unwrap();

    let role_id = RoleId::default();
    let sessions = mem.sessions.list_sessions(&role_id).await.unwrap();
    assert_eq!(sessions.len(), 2);

    let scopes: Vec<&str> = sessions.iter().map(|s| s.session_key.scope()).collect();
    assert!(scopes.contains(&"cli"));
    assert!(scopes.contains(&"telegram"));
}
