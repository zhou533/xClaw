//! Integration tests for MemorySystem facade and memory tools.

use std::time::Duration;

use xclaw_core::types::RoleId;
use xclaw_memory::facade::FsMemorySystem;
use xclaw_memory::role::daily::DailyMemory;
use xclaw_memory::tools::register_memory_tools;
use xclaw_memory::workspace::loader::MemoryFileLoader;
use xclaw_memory::workspace::types::MemoryFileKind;
use xclaw_tools::registry::ToolRegistry;
use xclaw_tools::traits::{ToolContext, WorkspaceScope};

fn setup() -> (tempfile::TempDir, FsMemorySystem) {
    let tmp = tempfile::TempDir::new().unwrap();
    let mem = FsMemorySystem::fs(tmp.path());
    (tmp, mem)
}

fn make_ctx(tmp: &tempfile::TempDir) -> ToolContext {
    ToolContext::new(WorkspaceScope::new(tmp.path()), Duration::from_secs(30))
}

// ─── Facade Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn ensure_default_creates_role_directory() {
    let (tmp, mem) = setup();
    mem.ensure_default_role().await.unwrap();
    assert!(tmp.path().join("roles/default/role.yaml").exists());
    assert!(tmp.path().join("roles/default/memory").is_dir());
}

#[tokio::test]
async fn facade_full_workflow() {
    let (_tmp, mem) = setup();
    let role = RoleId::default();

    mem.ensure_default_role().await.unwrap();

    // Long-term memory via files
    mem.files
        .save_file(
            &role,
            MemoryFileKind::LongTerm,
            "# Key Facts\n- User prefers Rust",
        )
        .await
        .unwrap();
    let lt = mem
        .files
        .load_file(&role, MemoryFileKind::LongTerm)
        .await
        .unwrap();
    assert!(lt.as_deref().unwrap().contains("Rust"));

    // Daily memory
    mem.daily.append(&role, "Session started").await.unwrap();
    mem.daily
        .append(&role, "Discussed architecture")
        .await
        .unwrap();

    // Workspace files via files
    mem.files
        .save_file(
            &role,
            MemoryFileKind::Soul,
            "# Persona\nHelpful and precise",
        )
        .await
        .unwrap();
    let snap = mem.files.load_snapshot(&role).await.unwrap();
    assert!(snap.files[&MemoryFileKind::Soul].is_some());
    assert!(snap.files[&MemoryFileKind::Agents].is_none());
}

// ─── Tools Registration Tests ────────────────────────────────────────────────

#[tokio::test]
async fn register_memory_tools_adds_9_tools() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    assert_eq!(registry.len(), 9);

    // Verify all tool names
    let expected = [
        "role_create",
        "role_list",
        "role_get",
        "role_delete",
        "memory_file_read",
        "memory_file_write",
        "memory_file_delete",
        "memory_daily_append",
        "memory_daily_read",
    ];
    for name in &expected {
        assert!(registry.get(name).is_some(), "missing tool: {name}");
    }
}

#[tokio::test]
async fn tool_role_create_creates_directory() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("role_create").unwrap();
    let result = tool
        .execute(
            &ctx,
            serde_json::json!({
                "name": "coder",
                "description": ["Rust coding assistant"],
                "system_prompt": "You are a Rust expert",
                "tools": ["file_read", "file_write"]
            }),
        )
        .await
        .unwrap();

    assert!(!result.is_error);
    assert!(tmp.path().join("roles/coder/role.yaml").exists());
    assert!(tmp.path().join("roles/coder/memory").is_dir());
}

#[tokio::test]
async fn tool_memory_daily_append_writes_file() {
    let (tmp, mem) = setup();
    mem.ensure_default_role().await.unwrap();

    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("memory_daily_append").unwrap();
    let result = tool
        .execute(&ctx, serde_json::json!({ "entry": "User likes dark mode" }))
        .await
        .unwrap();

    assert!(!result.is_error);

    // Verify via trait
    let role = RoleId::default();
    let date = xclaw_memory::role::daily::today();
    let content = mem.daily.load_day(&role, &date).await.unwrap();
    assert!(content.contains("dark mode"));
}

#[tokio::test]
async fn tool_memory_file_write_and_read() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);

    // Write via memory_file_write
    let write_tool = registry.get("memory_file_write").unwrap();
    let result = write_tool
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "soul",
                "content": "# Persona\nFriendly and helpful"
            }),
        )
        .await
        .unwrap();
    assert!(!result.is_error);

    // Read via memory_file_read
    let read_tool = registry.get("memory_file_read").unwrap();
    let result = read_tool
        .execute(&ctx, serde_json::json!({ "kind": "soul" }))
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("Friendly and helpful"));
}

#[tokio::test]
async fn tool_memory_file_write_and_read_long_term() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);

    // Write long-term memory
    let write_tool = registry.get("memory_file_write").unwrap();
    let result = write_tool
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "long_term",
                "content": "# Knowledge\n- User prefers Rust"
            }),
        )
        .await
        .unwrap();
    assert!(!result.is_error);

    // Read long-term memory
    let read_tool = registry.get("memory_file_read").unwrap();
    let result = read_tool
        .execute(&ctx, serde_json::json!({ "kind": "long_term" }))
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("User prefers Rust"));
}

#[tokio::test]
async fn tool_role_list_after_create() {
    let (tmp, mem) = setup();
    mem.ensure_default_role().await.unwrap();

    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);

    // Create a role
    let create = registry.get("role_create").unwrap();
    create
        .execute(&ctx, serde_json::json!({ "name": "secretary" }))
        .await
        .unwrap();

    // List roles
    let list = registry.get("role_list").unwrap();
    let result = list.execute(&ctx, serde_json::json!({})).await.unwrap();
    assert!(result.content.contains("default"));
    assert!(result.content.contains("secretary"));
}

// ─── Tool Error Path Tests ──────────────────────────────────────────────────

#[tokio::test]
async fn tool_memory_file_read_missing_kind_returns_error() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("memory_file_read").unwrap();
    let err = tool.execute(&ctx, serde_json::json!({})).await.unwrap_err();
    assert!(err.to_string().contains("kind is required"));
}

#[tokio::test]
async fn tool_memory_file_read_invalid_kind_returns_error() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("memory_file_read").unwrap();
    let err = tool
        .execute(&ctx, serde_json::json!({ "kind": "nonexistent" }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("invalid memory file kind"));
}

#[tokio::test]
async fn tool_memory_file_write_missing_content_returns_error() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("memory_file_write").unwrap();
    let err = tool
        .execute(&ctx, serde_json::json!({ "kind": "soul" }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("content is required"));
}

#[tokio::test]
async fn tool_memory_daily_append_missing_entry_returns_error() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("memory_daily_append").unwrap();
    let err = tool.execute(&ctx, serde_json::json!({})).await.unwrap_err();
    assert!(err.to_string().contains("entry is required"));
}

#[tokio::test]
async fn tool_memory_daily_read_missing_date_returns_error() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("memory_daily_read").unwrap();
    let err = tool.execute(&ctx, serde_json::json!({})).await.unwrap_err();
    assert!(err.to_string().contains("date is required"));
}

#[tokio::test]
async fn tool_role_get_nonexistent_returns_error() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("role_get").unwrap();
    let err = tool
        .execute(&ctx, serde_json::json!({ "name": "ghost" }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("role not found"));
}

#[tokio::test]
async fn tool_role_create_invalid_name_returns_error() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("role_create").unwrap();
    let err = tool
        .execute(&ctx, serde_json::json!({ "name": "INVALID" }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("invalid"));
}
