//! Integration tests for MemoryFileLoader (including LongTerm) and DailyMemory.

use xclaw_core::types::RoleId;
use xclaw_memory::error::MemoryError;
use xclaw_memory::role::daily::{DailyMemory, FsDailyMemory, today};
use xclaw_memory::workspace::loader::{FsMemoryFileLoader, MemoryFileLoader};
use xclaw_memory::workspace::types::MemoryFileKind;

fn default_role() -> RoleId {
    RoleId::default()
}

// ─── LongTermMemory (via MemoryFileLoader) ──────────────────────────────────

#[tokio::test]
async fn long_term_save_and_load() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = default_role();

    loader
        .save_file(
            &role,
            MemoryFileKind::LongTerm,
            "# Knowledge\n- Rust is great",
        )
        .await
        .unwrap();
    let content = loader
        .load_file(&role, MemoryFileKind::LongTerm)
        .await
        .unwrap();
    assert_eq!(content.as_deref(), Some("# Knowledge\n- Rust is great"));
}

#[tokio::test]
async fn long_term_load_missing_returns_none() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let content = loader
        .load_file(&default_role(), MemoryFileKind::LongTerm)
        .await
        .unwrap();
    assert!(content.is_none());
}

#[tokio::test]
async fn long_term_save_overwrites() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = default_role();

    loader
        .save_file(&role, MemoryFileKind::LongTerm, "old")
        .await
        .unwrap();
    loader
        .save_file(&role, MemoryFileKind::LongTerm, "new")
        .await
        .unwrap();
    let content = loader
        .load_file(&role, MemoryFileKind::LongTerm)
        .await
        .unwrap();
    assert_eq!(content.as_deref(), Some("new"));
}

// ─── DailyMemory ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn daily_append_multiple_entries() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dm = FsDailyMemory::new(tmp.path());
    let role = default_role();

    dm.append(&role, "entry A").await.unwrap();
    dm.append(&role, "entry B").await.unwrap();
    dm.append(&role, "entry C").await.unwrap();

    let content = dm.load_day(&role, &today()).await.unwrap();
    assert!(content.contains("entry A"));
    assert!(content.contains("entry B"));
    assert!(content.contains("entry C"));
}

#[tokio::test]
async fn daily_load_missing_returns_empty() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dm = FsDailyMemory::new(tmp.path());
    let content = dm.load_day(&default_role(), "2020-01-01").await.unwrap();
    assert!(content.is_empty());
}

#[tokio::test]
async fn daily_invalid_date_fails() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dm = FsDailyMemory::new(tmp.path());
    let err = dm.load_day(&default_role(), "bad-date").await.unwrap_err();
    assert!(matches!(err, MemoryError::InvalidDate(_)));
}

#[tokio::test]
async fn daily_list_days_sorted() {
    let tmp = tempfile::TempDir::new().unwrap();
    let role = default_role();

    // Create date files manually
    let mem_dir = tmp.path().join("roles/default/memory");
    tokio::fs::create_dir_all(&mem_dir).await.unwrap();
    tokio::fs::write(mem_dir.join("2026-03-25.md"), "a")
        .await
        .unwrap();
    tokio::fs::write(mem_dir.join("2026-01-01.md"), "b")
        .await
        .unwrap();
    tokio::fs::write(mem_dir.join("2026-12-31.md"), "c")
        .await
        .unwrap();

    let dm = FsDailyMemory::new(tmp.path());
    let days = dm.list_days(&role).await.unwrap();
    assert_eq!(days, vec!["2026-01-01", "2026-03-25", "2026-12-31"]);
}

// ─── delete_file ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn workspace_delete_existing_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = default_role();

    loader
        .save_file(&role, MemoryFileKind::Soul, "persona")
        .await
        .unwrap();
    let deleted = loader
        .delete_file(&role, MemoryFileKind::Soul)
        .await
        .unwrap();
    assert!(deleted);

    let loaded = loader.load_file(&role, MemoryFileKind::Soul).await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn workspace_delete_nonexistent_returns_false() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let deleted = loader
        .delete_file(&default_role(), MemoryFileKind::Tools)
        .await
        .unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn workspace_delete_then_snapshot_excludes_deleted() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = default_role();

    loader
        .save_file(&role, MemoryFileKind::Soul, "persona")
        .await
        .unwrap();
    loader
        .save_file(&role, MemoryFileKind::User, "prefs")
        .await
        .unwrap();
    loader
        .save_file(&role, MemoryFileKind::Agents, "agents")
        .await
        .unwrap();

    // Delete only Soul
    loader
        .delete_file(&role, MemoryFileKind::Soul)
        .await
        .unwrap();

    let snap = loader.load_snapshot(&role).await.unwrap();
    assert!(snap.files[&MemoryFileKind::Soul].is_none());
    assert_eq!(snap.files[&MemoryFileKind::User].as_deref(), Some("prefs"));
    assert_eq!(
        snap.files[&MemoryFileKind::Agents].as_deref(),
        Some("agents")
    );
}

// ─── append_file ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn append_to_nonexistent_creates_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = default_role();

    loader
        .append_file(&role, MemoryFileKind::Soul, "# New Section")
        .await
        .unwrap();

    let content = loader.load_file(&role, MemoryFileKind::Soul).await.unwrap();
    assert_eq!(content.as_deref(), Some("# New Section"));
}

#[tokio::test]
async fn append_to_existing_preserves_original() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = default_role();

    loader
        .save_file(&role, MemoryFileKind::User, "# Original")
        .await
        .unwrap();
    loader
        .append_file(&role, MemoryFileKind::User, "## Appended")
        .await
        .unwrap();

    let content = loader.load_file(&role, MemoryFileKind::User).await.unwrap();
    assert_eq!(content.as_deref(), Some("# Original\n\n## Appended"));
}

#[tokio::test]
async fn append_multiple_accumulates() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = default_role();

    loader
        .save_file(&role, MemoryFileKind::Agents, "entry1")
        .await
        .unwrap();
    loader
        .append_file(&role, MemoryFileKind::Agents, "entry2")
        .await
        .unwrap();
    loader
        .append_file(&role, MemoryFileKind::Agents, "entry3")
        .await
        .unwrap();

    let content = loader
        .load_file(&role, MemoryFileKind::Agents)
        .await
        .unwrap();
    assert_eq!(content.as_deref(), Some("entry1\n\nentry2\n\nentry3"));
}

// ─── MemoryFileLoader (workspace files) ─────────────────────────────────────

#[tokio::test]
async fn workspace_save_and_load_all_kinds() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = default_role();

    for &kind in MemoryFileKind::all() {
        let content = format!("Content for {}", kind.filename());
        loader.save_file(&role, kind, &content).await.unwrap();
        let loaded = loader.load_file(&role, kind).await.unwrap();
        assert_eq!(loaded.as_deref(), Some(content.as_str()));
    }
}

#[tokio::test]
async fn workspace_missing_returns_none() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let result = loader
        .load_file(&default_role(), MemoryFileKind::Soul)
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn workspace_snapshot_partial() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = default_role();

    // Only write Soul and User
    loader
        .save_file(&role, MemoryFileKind::Soul, "persona")
        .await
        .unwrap();
    loader
        .save_file(&role, MemoryFileKind::User, "prefs")
        .await
        .unwrap();

    let snap = loader.load_snapshot(&role).await.unwrap();
    assert_eq!(snap.files.len(), 8);
    assert_eq!(
        snap.files[&MemoryFileKind::Soul].as_deref(),
        Some("persona")
    );
    assert_eq!(snap.files[&MemoryFileKind::User].as_deref(), Some("prefs"));
    assert!(snap.files[&MemoryFileKind::Agents].is_none());
    assert!(snap.files[&MemoryFileKind::Tools].is_none());
    assert!(snap.files[&MemoryFileKind::Identity].is_none());
    assert!(snap.files[&MemoryFileKind::Heartbeat].is_none());
    assert!(snap.files[&MemoryFileKind::Bootstrap].is_none());
    assert!(snap.files[&MemoryFileKind::LongTerm].is_none());
}
