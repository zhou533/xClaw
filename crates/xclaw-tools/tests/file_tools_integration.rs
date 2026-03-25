//! Integration tests for file tools via ToolRegistry.

use std::time::Duration;

use xclaw_tools::{ToolContext, ToolRegistry, WorkspaceScope, register_builtin_tools};

fn setup() -> (tempfile::TempDir, ToolRegistry, ToolContext) {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry);
    let ctx = ToolContext::new(WorkspaceScope::new(tmp.path()), Duration::from_secs(30));
    (tmp, registry, ctx)
}

#[tokio::test]
async fn write_then_read_via_registry() {
    let (tmp, registry, ctx) = setup();
    let file_path = tmp.path().join("integration.txt");
    let path_str = file_path.to_str().unwrap();

    // Write
    let write_tool = registry.get("file_write").unwrap();
    let write_result = write_tool
        .execute(
            &ctx,
            serde_json::json!({
                "path": path_str,
                "content": "integration test content"
            }),
        )
        .await
        .unwrap();
    assert!(!write_result.is_error);

    // Read
    let read_tool = registry.get("file_read").unwrap();
    let read_result = read_tool
        .execute(&ctx, serde_json::json!({ "path": path_str }))
        .await
        .unwrap();
    assert_eq!(read_result.content, "integration test content");
}

#[tokio::test]
async fn write_then_edit_then_read_via_registry() {
    let (tmp, registry, ctx) = setup();
    let file_path = tmp.path().join("edit_test.txt");
    let path_str = file_path.to_str().unwrap();

    // Write initial content
    let write_tool = registry.get("file_write").unwrap();
    write_tool
        .execute(
            &ctx,
            serde_json::json!({
                "path": path_str,
                "content": "fn old_name() {}\n"
            }),
        )
        .await
        .unwrap();

    // Edit
    let edit_tool = registry.get("file_edit").unwrap();
    let edit_result = edit_tool
        .execute(
            &ctx,
            serde_json::json!({
                "path": path_str,
                "edits": [{ "search": "old_name", "replace": "new_name" }]
            }),
        )
        .await
        .unwrap();
    assert!(edit_result.content.contains("1 edit(s)"));

    // Read back
    let read_tool = registry.get("file_read").unwrap();
    let read_result = read_tool
        .execute(&ctx, serde_json::json!({ "path": path_str }))
        .await
        .unwrap();
    assert!(read_result.content.contains("fn new_name()"));
}

#[test]
fn list_schemas_includes_all_file_tools() {
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry);

    let schemas = registry.list_schemas();
    let names: Vec<&str> = schemas.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"file_read"));
    assert!(names.contains(&"file_write"));
    assert!(names.contains(&"file_edit"));
}
