use std::time::Duration;

use xclaw_tools::{ToolContext, ToolRegistry, WorkspaceScope, register_builtin_tools};

fn test_ctx(workspace: &std::path::Path) -> ToolContext {
    ToolContext::new(WorkspaceScope::new(workspace), Duration::from_secs(30))
}

#[tokio::test]
async fn datetime_tool_found_in_registry() {
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry);

    let tool = registry.get("get_current_datetime");
    assert!(tool.is_some(), "get_current_datetime should be registered");
}

#[tokio::test]
async fn datetime_tool_executes_through_registry() {
    let mut registry = ToolRegistry::new();
    register_builtin_tools(&mut registry);

    let tool = registry.get("get_current_datetime").unwrap();
    let tmp = tempfile::TempDir::new().unwrap();
    let ctx = test_ctx(tmp.path());

    let result = tool
        .execute(&ctx, serde_json::json!({}))
        .await
        .expect("execute should succeed");

    assert!(!result.is_error);
    assert!(!result.content.is_empty());

    // Verify the output is a valid datetime
    let parsed = chrono::DateTime::parse_from_str(&result.content, "%Y-%m-%dT%H:%M:%S%:z");
    assert!(
        parsed.is_ok(),
        "output '{}' should parse as ISO 8601",
        result.content
    );
}
