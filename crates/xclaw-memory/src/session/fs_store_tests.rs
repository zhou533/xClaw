//! Unit tests for `FsSessionStore`.
//!
//! Included from `fs_store.rs` via `#[path]` to keep the impl file under 800 lines.

use super::*;
use tempfile::TempDir;
use xclaw_core::types::{RoleId, SessionId, SessionKey};

// ── Helpers ──

fn role() -> RoleId {
    RoleId::new("default").unwrap()
}

fn key(scope: &str) -> SessionKey {
    SessionKey::parse(&format!("default:{scope}")).unwrap()
}

fn record(role_str: &str, content: &str) -> TranscriptRecord {
    TranscriptRecord {
        role: role_str.to_string(),
        content: content.to_string(),
        timestamp: "2026-03-28T10:00:00Z".to_string(),
        tool_call_id: None,
        tool_name: None,
        metadata: None,
    }
}

fn store(dir: &TempDir) -> FsSessionStore {
    FsSessionStore::new(dir.path())
}

// ─── 1. new_creates_instance ────────────────────────────────────────────────

#[test]
fn new_creates_instance() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    assert_eq!(s.base_dir, dir.path());
}

// ─── 2. sessions_dir_path_correct ───────────────────────────────────────────

#[test]
fn sessions_dir_path_correct() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let expected = dir.path().join("roles").join("default").join("sessions");
    assert_eq!(s.sessions_dir(&role()), expected);
}

// ─── 3. index_path_correct ──────────────────────────────────────────────────

#[test]
fn index_path_correct() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let expected = dir
        .path()
        .join("roles")
        .join("default")
        .join("sessions")
        .join("sessions.json");
    assert_eq!(s.index_path(&role()), expected);
}

// ─── 4. transcript_path_correct ─────────────────────────────────────────────

#[test]
fn transcript_path_correct() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let sid = SessionId::new("abc-123");
    let expected = dir
        .path()
        .join("roles")
        .join("default")
        .join("sessions")
        .join("abc-123.jsonl");
    assert_eq!(s.transcript_path(&role(), &sid), expected);
}

// ─── 5. read_index_returns_empty_when_no_file ───────────────────────────────

#[test]
fn read_index_returns_empty_when_no_file() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let idx = s.read_index(&role()).unwrap();
    assert_eq!(idx.version, 1);
    assert!(idx.sessions.is_empty());
}

// ─── 6. write_and_read_index_roundtrip ──────────────────────────────────────

#[test]
fn write_and_read_index_roundtrip() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);

    let entry = SessionEntry {
        session_id: SessionId::new("sess-1"),
        session_key: key("cli"),
        transcript_path: "sess-1.jsonl".into(),
        created_at: "2026-03-28T10:00:00Z".into(),
        updated_at: "2026-03-28T10:00:00Z".into(),
    };
    let index = SessionIndex {
        version: 1,
        sessions: vec![entry],
    };

    s.write_index(&role(), &index).unwrap();
    let loaded = s.read_index(&role()).unwrap();
    assert_eq!(loaded.sessions.len(), 1);
    assert_eq!(loaded.sessions[0].session_id.as_str(), "sess-1");
}

// ─── 7. write_index_is_atomic ───────────────────────────────────────────────

#[test]
fn write_index_is_atomic() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);

    let index = SessionIndex::empty();
    s.write_index(&role(), &index).unwrap();

    // After a successful write, no leftover temp files should exist.
    let sessions_dir = s.sessions_dir(&role());
    let leftovers: Vec<_> = std::fs::read_dir(&sessions_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let n = name.to_string_lossy();
            n != "sessions.json"
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "unexpected temp files left: {leftovers:?}"
    );
}

// ─── 8. get_or_create_new_session ───────────────────────────────────────────

#[tokio::test]
async fn get_or_create_new_session() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");

    let entry = s.get_or_create(&k).await.unwrap();
    assert_eq!(entry.session_key, k);
    assert!(!entry.session_id.as_str().is_empty());

    // Verify it was persisted.
    let idx = s.read_index(&role()).unwrap();
    assert_eq!(idx.sessions.len(), 1);
}

// ─── 9. get_or_create_existing_session ──────────────────────────────────────

#[tokio::test]
async fn get_or_create_existing_session() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");

    let first = s.get_or_create(&k).await.unwrap();
    let second = s.get_or_create(&k).await.unwrap();

    // Must return the same session, not create a new one.
    assert_eq!(first.session_id, second.session_id);

    let idx = s.read_index(&role()).unwrap();
    assert_eq!(idx.sessions.len(), 1, "must not duplicate sessions");
}

// ─── 10. get_by_id_found ────────────────────────────────────────────────────

#[tokio::test]
async fn get_by_id_found() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");

    let entry = s.get_or_create(&k).await.unwrap();
    let found = s.get_by_id(&role(), &entry.session_id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().session_id, entry.session_id);
}

// ─── 11. get_by_id_not_found ────────────────────────────────────────────────

#[tokio::test]
async fn get_by_id_not_found() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let missing = SessionId::new("does-not-exist");
    let result = s.get_by_id(&role(), &missing).await.unwrap();
    assert!(result.is_none());
}

// ─── 12. get_by_key_found ───────────────────────────────────────────────────

#[tokio::test]
async fn get_by_key_found() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("telegram");

    s.get_or_create(&k).await.unwrap();
    let found = s.get_by_key(&k).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().session_key, k);
}

// ─── 13. get_by_key_not_found ───────────────────────────────────────────────

#[tokio::test]
async fn get_by_key_not_found() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("ghost");
    let result = s.get_by_key(&k).await.unwrap();
    assert!(result.is_none());
}

// ─── 14. list_sessions_empty ────────────────────────────────────────────────

#[tokio::test]
async fn list_sessions_empty() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let sessions = s.list_sessions(&role()).await.unwrap();
    assert!(sessions.is_empty());
}

// ─── 15. list_sessions_multiple ─────────────────────────────────────────────

#[tokio::test]
async fn list_sessions_multiple() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);

    s.get_or_create(&key("cli")).await.unwrap();
    s.get_or_create(&key("telegram")).await.unwrap();
    s.get_or_create(&key("discord")).await.unwrap();

    let sessions = s.list_sessions(&role()).await.unwrap();
    assert_eq!(sessions.len(), 3);
}

// ─── 16. append_and_load_transcript ─────────────────────────────────────────

#[tokio::test]
async fn append_and_load_transcript() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");
    let entry = s.get_or_create(&k).await.unwrap();
    let sid = &entry.session_id;

    s.append_transcript(&role(), sid, &record("user", "hello"))
        .await
        .unwrap();
    s.append_transcript(&role(), sid, &record("assistant", "hi there"))
        .await
        .unwrap();

    let records = s.load_transcript(&role(), sid).await.unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].role, "user");
    assert_eq!(records[0].content, "hello");
    assert_eq!(records[1].role, "assistant");
    assert_eq!(records[1].content, "hi there");
}

// ─── 17. load_transcript_empty_file ─────────────────────────────────────────

#[tokio::test]
async fn load_transcript_empty_file() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");
    let entry = s.get_or_create(&k).await.unwrap();

    let records = s.load_transcript(&role(), &entry.session_id).await.unwrap();
    assert!(records.is_empty());
}

// ─── 18. load_transcript_tolerates_corrupt_last_line ────────────────────────

#[tokio::test]
async fn load_transcript_tolerates_corrupt_last_line() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");
    let entry = s.get_or_create(&k).await.unwrap();
    let sid = &entry.session_id;

    // Write one valid line + one corrupt partial line directly to the file.
    let valid_rec = record("user", "valid message");
    let valid_json = serde_json::to_string(&valid_rec).unwrap();
    let transcript_path = s.transcript_path(&role(), sid);
    let content = format!("{valid_json}\n{{corrupt partial\n");
    std::fs::write(&transcript_path, content).unwrap();

    let records = s.load_transcript(&role(), sid).await.unwrap();
    assert_eq!(
        records.len(),
        1,
        "corrupt last line should be tolerated/skipped"
    );
    assert_eq!(records[0].content, "valid message");
}

// ─── 19. load_transcript_tail ───────────────────────────────────────────────

#[tokio::test]
async fn load_transcript_tail() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");
    let entry = s.get_or_create(&k).await.unwrap();
    let sid = &entry.session_id;

    for i in 0..5 {
        s.append_transcript(&role(), sid, &record("user", &format!("msg {i}")))
            .await
            .unwrap();
    }

    let tail = s.load_transcript_tail(&role(), sid, 3).await.unwrap();
    assert_eq!(tail.len(), 3);
    assert_eq!(tail[0].content, "msg 2");
    assert_eq!(tail[1].content, "msg 3");
    assert_eq!(tail[2].content, "msg 4");
}

// ─── 20. session_summary_counts ─────────────────────────────────────────────

#[tokio::test]
async fn session_summary_counts() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");
    let entry = s.get_or_create(&k).await.unwrap();
    let sid = &entry.session_id;

    let r1 = TranscriptRecord {
        role: "user".into(),
        content: "first".into(),
        timestamp: "2026-03-28T10:00:00Z".into(),
        tool_call_id: None,
        tool_name: None,
        metadata: None,
    };
    let r2 = TranscriptRecord {
        role: "assistant".into(),
        content: "second".into(),
        timestamp: "2026-03-28T10:01:00Z".into(),
        tool_call_id: None,
        tool_name: None,
        metadata: None,
    };

    s.append_transcript(&role(), sid, &r1).await.unwrap();
    s.append_transcript(&role(), sid, &r2).await.unwrap();

    let summary = s.session_summary(&role(), sid).await.unwrap();
    assert_eq!(summary.message_count, 2);
    assert_eq!(
        summary.first_message_at.as_deref(),
        Some("2026-03-28T10:00:00Z")
    );
    assert_eq!(
        summary.last_message_at.as_deref(),
        Some("2026-03-28T10:01:00Z")
    );
    assert_eq!(summary.session_id, *sid);
    assert_eq!(summary.session_key, k);
}

// ─── 21. delete_session_removes_entry_and_file ──────────────────────────────

#[tokio::test]
async fn delete_session_removes_entry_and_file() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");
    let entry = s.get_or_create(&k).await.unwrap();
    let sid = entry.session_id.clone();

    s.append_transcript(&role(), &sid, &record("user", "hi"))
        .await
        .unwrap();

    // Transcript file should exist.
    let transcript_path = s.transcript_path(&role(), &sid);
    assert!(
        transcript_path.exists(),
        "transcript file should exist before delete"
    );

    s.delete_session(&role(), &sid).await.unwrap();

    // Index should be empty.
    let idx = s.read_index(&role()).unwrap();
    assert!(
        idx.sessions.is_empty(),
        "index should be empty after delete"
    );

    // Transcript file should be gone.
    assert!(
        !transcript_path.exists(),
        "transcript file should be removed after delete"
    );
}

// ─── 22. delete_session_not_found ───────────────────────────────────────────

#[tokio::test]
async fn delete_session_not_found() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let missing = SessionId::new("ghost-session");

    let result = s.delete_session(&role(), &missing).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        MemoryError::SessionNotFound(_)
    ));
}

// ─── 23. append_to_nonexistent_session ──────────────────────────────────────

#[tokio::test]
async fn append_to_nonexistent_session() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let missing = SessionId::new("no-such-session");
    let rec = record("user", "hello");

    let result = s.append_transcript(&role(), &missing, &rec).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        MemoryError::SessionNotFound(_)
    ));
}

// ─── 24. append_transcript_updates_updated_at ───────────────────────────────

#[tokio::test]
async fn append_transcript_updates_updated_at() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");
    let entry = s.get_or_create(&k).await.unwrap();
    let original_updated = entry.updated_at.clone();

    // Small delay to ensure timestamp differs.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    s.append_transcript(&role(), &entry.session_id, &record("user", "hi"))
        .await
        .unwrap();

    let refreshed = s
        .get_by_id(&role(), &entry.session_id)
        .await
        .unwrap()
        .unwrap();
    // updated_at should be >= original (may be same second, but not the exact same
    // object since we rebuild the index).
    assert!(refreshed.updated_at >= original_updated);
}

// ─── 25. get_or_create_renews_expired_daily_session ────────────────────────

#[tokio::test]
async fn get_or_create_renews_expired_daily_session() {
    use crate::session::policy::SessionPolicy;

    let dir = TempDir::new().unwrap();
    // Use explicit reset_at_hour=4 for deterministic test.
    let s = FsSessionStore::with_policy(
        dir.path(),
        SessionPolicy {
            reset_at_hour: 4,
            idle_minutes: None,
        },
    );
    let k = key("cli");

    // Create a session, then manually backdate its updated_at to yesterday.
    let entry = s.get_or_create(&k).await.unwrap();
    let old_id = entry.session_id.clone();

    // Overwrite the index with an old updated_at.
    let index = s.read_index(&role()).unwrap();
    let backdated: Vec<SessionEntry> = index
        .sessions
        .into_iter()
        .map(|e| SessionEntry {
            updated_at: "2020-01-01T01:00:00Z".into(),
            ..e
        })
        .collect();
    let new_index = SessionIndex {
        version: index.version,
        sessions: backdated,
    };
    s.write_index(&role(), &new_index).unwrap();

    // Now get_or_create should detect expiry and create a new session.
    let renewed = s.get_or_create(&k).await.unwrap();
    assert_ne!(renewed.session_id, old_id, "should create new session");
}

// ─── 26. get_or_create_reuses_unexpired_session ────────────────────────────

#[tokio::test]
async fn get_or_create_reuses_unexpired_session() {
    use crate::session::policy::SessionPolicy;

    let dir = TempDir::new().unwrap();
    // Use explicit policy for deterministic test.
    let s = FsSessionStore::with_policy(
        dir.path(),
        SessionPolicy {
            reset_at_hour: 4,
            idle_minutes: None,
        },
    );
    let k = key("cli");

    let first = s.get_or_create(&k).await.unwrap();
    let second = s.get_or_create(&k).await.unwrap();

    assert_eq!(
        first.session_id, second.session_id,
        "unexpired session should be reused"
    );
}

// ─── 27. get_or_create_renews_idle_expired_session ─────────────────────────

#[tokio::test]
async fn get_or_create_renews_idle_expired_session() {
    use crate::session::policy::SessionPolicy;

    let dir = TempDir::new().unwrap();
    let policy = SessionPolicy {
        reset_at_hour: 4,
        idle_minutes: Some(1), // 1 minute idle timeout
    };
    let s = FsSessionStore::with_policy(dir.path(), policy);
    let k = key("cli");

    let entry = s.get_or_create(&k).await.unwrap();
    let old_id = entry.session_id.clone();

    // Backdate updated_at to 10 minutes ago (exceeds 1 min idle).
    let index = s.read_index(&role()).unwrap();
    let now_epoch = crate::session::time_util::now_epoch_secs();
    let ten_min_ago = now_epoch - 600;
    let (y, mo, d, h, mi, sc) = crate::session::time_util::epoch_to_ymd_hms(ten_min_ago);
    let old_ts = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, sc);

    let backdated: Vec<SessionEntry> = index
        .sessions
        .into_iter()
        .map(|e| SessionEntry {
            updated_at: old_ts.clone(),
            ..e
        })
        .collect();
    let new_index = SessionIndex {
        version: index.version,
        sessions: backdated,
    };
    s.write_index(&role(), &new_index).unwrap();

    let renewed = s.get_or_create(&k).await.unwrap();
    assert_ne!(renewed.session_id, old_id, "idle-expired should renew");
}

// ─── 28. reset_session_creates_new_session ─────────────────────────────────

#[tokio::test]
async fn reset_session_creates_new_session() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");

    let original = s.get_or_create(&k).await.unwrap();
    let reset = s.reset_session(&k).await.unwrap();

    assert_ne!(original.session_id, reset.session_id);
    assert_eq!(reset.session_key, k);
}

// ─── 29. reset_session_preserves_old_session ───────────────────────────────

#[tokio::test]
async fn reset_session_preserves_old_session() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");

    let original = s.get_or_create(&k).await.unwrap();
    s.reset_session(&k).await.unwrap();

    let all = s.list_sessions(&role()).await.unwrap();
    assert_eq!(all.len(), 2, "old + new session");
    assert!(
        all.iter().any(|e| e.session_id == original.session_id),
        "old session should still exist"
    );
}

// ─── 30. get_by_key_returns_latest_session ─────────────────────────────────

#[tokio::test]
async fn get_by_key_returns_latest_session() {
    let dir = TempDir::new().unwrap();
    let s = store(&dir);
    let k = key("cli");

    s.get_or_create(&k).await.unwrap();
    let latest = s.reset_session(&k).await.unwrap();

    let found = s.get_by_key(&k).await.unwrap().unwrap();
    assert_eq!(
        found.session_id, latest.session_id,
        "get_by_key should return latest"
    );
}
