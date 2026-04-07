//! Integration tests for MemorySystem facade and memory tools.

use std::time::Duration;

use xclaw_core::types::RoleId;
use xclaw_memory::facade::FsMemorySystem;
use xclaw_memory::role::daily::DailyMemory;
use xclaw_memory::tools::register_memory_tools;
use xclaw_memory::workspace::loader::{FsMemoryFileLoader, MemoryFileLoader};
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

/// Extract the content_hash from the YAML front matter returned by memory_file_read.
fn extract_hash(output: &str) -> &str {
    for line in output.lines() {
        if let Some(hash) = line.strip_prefix("content_hash: ") {
            return hash.trim();
        }
    }
    panic!("no content_hash found in output:\n{output}");
}

// ─── Facade Tests ────────────────────────────────────────────────────────────

#[tokio::test]
async fn ensure_default_creates_role_directory() {
    let (tmp, mem) = setup();
    mem.ensure_default_role().await.unwrap();
    assert!(tmp.path().join("roles.yaml").exists());
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
    // Soul was explicitly saved above — must be present.
    assert!(snap.files[&MemoryFileKind::Soul].is_some());
    // Agents is seeded from the bootstrap template on role creation — must also be present.
    assert!(snap.files[&MemoryFileKind::Agents].is_some());
    // Heartbeat has no template and was never written — must be absent.
    assert!(snap.files[&MemoryFileKind::Heartbeat].is_none());
}

// ─── Tools Registration Tests ────────────────────────────────────────────────

#[tokio::test]
async fn register_memory_tools_adds_10_tools() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    assert_eq!(registry.len(), 10);

    // Verify all tool names
    let expected = [
        "role_create",
        "role_list",
        "role_get",
        "role_delete",
        "memory_file_read",
        "memory_file_append",
        "memory_file_edit",
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
    assert!(tmp.path().join("roles.yaml").exists());
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
async fn tool_memory_file_append_and_read() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);

    // Append to new file via memory_file_append
    let append_tool = registry.get("memory_file_append").unwrap();
    let result = append_tool
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "soul",
                "content": "# Persona\nFriendly and helpful",
                "content_hash": "__new__"
            }),
        )
        .await
        .unwrap();
    assert!(!result.is_error);

    // Read via memory_file_read — must return line numbers and hash
    let read_tool = registry.get("memory_file_read").unwrap();
    let result = read_tool
        .execute(&ctx, serde_json::json!({ "kind": "soul" }))
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("content_hash:"));
    assert!(result.content.contains("Friendly and helpful"));
    assert!(result.content.contains("1 |"));
}

#[tokio::test]
async fn tool_memory_file_append_and_read_long_term() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);

    // Create long-term memory
    let append_tool = registry.get("memory_file_append").unwrap();
    let result = append_tool
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "long_term",
                "content": "# Knowledge\n- User prefers Rust",
                "content_hash": "__new__"
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
    assert!(result.content.contains("content_hash:"));
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
async fn tool_memory_file_append_missing_content_returns_error() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("memory_file_append").unwrap();
    let err = tool
        .execute(
            &ctx,
            serde_json::json!({ "kind": "soul", "content_hash": "__new__" }),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("content is required"));
}

#[tokio::test]
async fn tool_memory_file_append_missing_hash_returns_error() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());

    let ctx = make_ctx(&tmp);
    let tool = registry.get("memory_file_append").unwrap();
    let err = tool
        .execute(
            &ctx,
            serde_json::json!({ "kind": "soul", "content": "hello" }),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("content_hash is required"));
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

// ─── Edit Tool Integration Tests ────────────────────────────────────────────

#[tokio::test]
async fn tool_memory_file_edit_replace() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());
    let ctx = make_ctx(&tmp);

    // Create file
    registry
        .get("memory_file_append")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "soul",
                "content": "line one\nline two\nline three",
                "content_hash": "__new__"
            }),
        )
        .await
        .unwrap();

    // Read to get hash
    let read_out = registry
        .get("memory_file_read")
        .unwrap()
        .execute(&ctx, serde_json::json!({ "kind": "soul" }))
        .await
        .unwrap();
    let hash = extract_hash(&read_out.content).to_owned();

    // Edit line 2 with replace
    let edit_result = registry
        .get("memory_file_edit")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "soul",
                "content_hash": hash,
                "line_start": 2,
                "operation": "replace",
                "content": "REPLACED LINE"
            }),
        )
        .await
        .unwrap();
    assert!(!edit_result.is_error);

    // Verify content
    let final_out = registry
        .get("memory_file_read")
        .unwrap()
        .execute(&ctx, serde_json::json!({ "kind": "soul" }))
        .await
        .unwrap();
    assert!(final_out.content.contains("REPLACED LINE"));
    assert!(final_out.content.contains("line one"));
    assert!(final_out.content.contains("line three"));
}

#[tokio::test]
async fn tool_memory_file_edit_insert_after() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());
    let ctx = make_ctx(&tmp);

    registry
        .get("memory_file_append")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "user",
                "content": "alpha\nbeta",
                "content_hash": "__new__"
            }),
        )
        .await
        .unwrap();

    let read_out = registry
        .get("memory_file_read")
        .unwrap()
        .execute(&ctx, serde_json::json!({ "kind": "user" }))
        .await
        .unwrap();
    let hash = extract_hash(&read_out.content).to_owned();

    registry
        .get("memory_file_edit")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "user",
                "content_hash": hash,
                "line_start": 1,
                "operation": "insert_after",
                "content": "INSERTED"
            }),
        )
        .await
        .unwrap();

    let final_out = registry
        .get("memory_file_read")
        .unwrap()
        .execute(&ctx, serde_json::json!({ "kind": "user" }))
        .await
        .unwrap();
    assert!(final_out.content.contains("alpha"));
    assert!(final_out.content.contains("INSERTED"));
    assert!(final_out.content.contains("beta"));
}

#[tokio::test]
async fn tool_memory_file_edit_insert_before() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());
    let ctx = make_ctx(&tmp);

    registry
        .get("memory_file_append")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "tools",
                "content": "alpha\nbeta",
                "content_hash": "__new__"
            }),
        )
        .await
        .unwrap();

    let read_out = registry
        .get("memory_file_read")
        .unwrap()
        .execute(&ctx, serde_json::json!({ "kind": "tools" }))
        .await
        .unwrap();
    let hash = extract_hash(&read_out.content).to_owned();

    registry
        .get("memory_file_edit")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "tools",
                "content_hash": hash,
                "line_start": 2,
                "operation": "insert_before",
                "content": "BEFORE_BETA"
            }),
        )
        .await
        .unwrap();

    let final_out = registry
        .get("memory_file_read")
        .unwrap()
        .execute(&ctx, serde_json::json!({ "kind": "tools" }))
        .await
        .unwrap();
    assert!(final_out.content.contains("alpha"));
    assert!(final_out.content.contains("BEFORE_BETA"));
    assert!(final_out.content.contains("beta"));
}

#[tokio::test]
async fn tool_memory_file_edit_stale_hash_rejected() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());
    let ctx = make_ctx(&tmp);

    // Create file
    registry
        .get("memory_file_append")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "identity",
                "content": "original",
                "content_hash": "__new__"
            }),
        )
        .await
        .unwrap();

    // Read to capture hash
    let read_out = registry
        .get("memory_file_read")
        .unwrap()
        .execute(&ctx, serde_json::json!({ "kind": "identity" }))
        .await
        .unwrap();
    let stale_hash = extract_hash(&read_out.content).to_owned();

    // Modify file directly via loader (simulate concurrent change)
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = RoleId::default();
    loader
        .save_file(&role, MemoryFileKind::Identity, "changed externally")
        .await
        .unwrap();

    // Edit with stale hash — must be rejected
    let err = registry
        .get("memory_file_edit")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "identity",
                "content_hash": stale_hash,
                "line_start": 1,
                "operation": "replace",
                "content": "attempt"
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("content hash mismatch"),
        "expected stale error, got: {err}"
    );
}

#[tokio::test]
async fn tool_memory_file_edit_line_out_of_range() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());
    let ctx = make_ctx(&tmp);

    registry
        .get("memory_file_append")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "agents",
                "content": "one\ntwo",
                "content_hash": "__new__"
            }),
        )
        .await
        .unwrap();

    let read_out = registry
        .get("memory_file_read")
        .unwrap()
        .execute(&ctx, serde_json::json!({ "kind": "agents" }))
        .await
        .unwrap();
    let hash = extract_hash(&read_out.content).to_owned();

    let err = registry
        .get("memory_file_edit")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "agents",
                "content_hash": hash,
                "line_start": 999,
                "operation": "replace",
                "content": "nope"
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("line out of range") || err.to_string().contains("out of range"),
        "expected range error, got: {err}"
    );
}

#[tokio::test]
async fn tool_memory_file_append_stale_hash_rejected() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());
    let ctx = make_ctx(&tmp);

    // Create file
    registry
        .get("memory_file_append")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "heartbeat",
                "content": "original",
                "content_hash": "__new__"
            }),
        )
        .await
        .unwrap();

    // Capture hash
    let read_out = registry
        .get("memory_file_read")
        .unwrap()
        .execute(&ctx, serde_json::json!({ "kind": "heartbeat" }))
        .await
        .unwrap();
    let stale_hash = extract_hash(&read_out.content).to_owned();

    // Modify externally
    let loader = FsMemoryFileLoader::new(tmp.path());
    let role = RoleId::default();
    loader
        .save_file(&role, MemoryFileKind::Heartbeat, "changed externally")
        .await
        .unwrap();

    // Append with stale hash — must be rejected
    let err = registry
        .get("memory_file_append")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "heartbeat",
                "content": "new stuff",
                "content_hash": stale_hash
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("content hash mismatch"),
        "expected stale error, got: {err}"
    );
}

#[tokio::test]
async fn tool_memory_file_append_new_file_with_wrong_hash_rejected() {
    let (tmp, _mem) = setup();
    let mut registry = ToolRegistry::new();
    register_memory_tools(&mut registry, tmp.path().to_path_buf());
    let ctx = make_ctx(&tmp);

    // File does not exist — pass wrong hash (not "__new__")
    let err = registry
        .get("memory_file_append")
        .unwrap()
        .execute(
            &ctx,
            serde_json::json!({
                "kind": "bootstrap",
                "content": "content",
                "content_hash": "wronghash0000000"
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("content hash mismatch"),
        "expected stale error, got: {err}"
    );
}
