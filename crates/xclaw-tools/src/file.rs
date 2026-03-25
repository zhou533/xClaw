//! File read/write/edit tools.
//!
//! Provides three atomic tools:
//! - `file_read` — Read file contents with optional line offset/limit
//! - `file_write` — Create or overwrite files
//! - `file_edit` — Patch files via search-and-replace blocks

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::ToolRegistry;
use crate::security;
use crate::traits::{Tool, ToolContext, ToolOutput};

/// Maximum file size for reading (10 MB).
const MAX_READ_BYTES: u64 = 10 * 1024 * 1024;

// ─── FileReadTool ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FileReadParams {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read file contents. Supports optional line offset and limit."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Start reading from this line number (0-based)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(
        &self,
        ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let p: FileReadParams =
            serde_json::from_value(params).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let path = std::path::PathBuf::from(&p.path);
        let canonical = security::validate_path(&path, ctx)?;

        // Check file size before reading
        let metadata = tokio::fs::metadata(&canonical).await?;
        if metadata.len() > MAX_READ_BYTES {
            return Err(ToolError::InvalidParams(format!(
                "file is {} bytes, exceeds {} byte limit; use offset/limit to read in chunks",
                metadata.len(),
                MAX_READ_BYTES
            )));
        }

        let content = tokio::time::timeout(ctx.timeout, tokio::fs::read_to_string(&canonical))
            .await
            .map_err(|_| ToolError::Timeout)??;

        let result = if p.offset.is_some() || p.limit.is_some() {
            let offset = p.offset.unwrap_or(0);
            let limit = p.limit.unwrap_or(usize::MAX);
            content
                .lines()
                .skip(offset)
                .take(limit)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            content
        };

        Ok(ToolOutput::success(result))
    }
}

// ─── FileWriteTool ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FileWriteParams {
    path: String,
    content: String,
}

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write or create a file. Automatically creates parent directories."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(
        &self,
        ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let p: FileWriteParams =
            serde_json::from_value(params).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let path = std::path::PathBuf::from(&p.path);
        let target = security::validate_path_for_write(&path, ctx)?;

        if let Some(parent) = target.parent() {
            tokio::time::timeout(ctx.timeout, tokio::fs::create_dir_all(parent))
                .await
                .map_err(|_| ToolError::Timeout)??;
        }

        tokio::time::timeout(ctx.timeout, tokio::fs::write(&target, &p.content))
            .await
            .map_err(|_| ToolError::Timeout)??;

        Ok(ToolOutput::success(format!(
            "Successfully wrote {} bytes to {}",
            p.content.len(),
            path.display()
        )))
    }
}

// ─── FileEditTool ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FileEditParams {
    path: String,
    edits: Vec<EditBlock>,
}

#[derive(Deserialize)]
struct EditBlock {
    search: String,
    replace: String,
}

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        "Patch a file using search-and-replace blocks. Each edit replaces the first occurrence of the search text."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "edits": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "search": {
                                "type": "string",
                                "description": "Text to search for in the file"
                            },
                            "replace": {
                                "type": "string",
                                "description": "Text to replace the search match with"
                            }
                        },
                        "required": ["search", "replace"]
                    },
                    "description": "List of search-and-replace edit blocks"
                }
            },
            "required": ["path", "edits"]
        })
    }

    async fn execute(
        &self,
        ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let p: FileEditParams =
            serde_json::from_value(params).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let path = std::path::PathBuf::from(&p.path);
        let canonical = security::validate_path(&path, ctx)?;

        let original = tokio::time::timeout(ctx.timeout, tokio::fs::read_to_string(&canonical))
            .await
            .map_err(|_| ToolError::Timeout)??;

        let edited = apply_edits(&original, &p.edits)?;

        tokio::time::timeout(ctx.timeout, tokio::fs::write(&canonical, &edited))
            .await
            .map_err(|_| ToolError::Timeout)??;
        let applied = p.edits.len();

        Ok(ToolOutput::success(format!(
            "Applied {applied} edit(s) to {}",
            path.display()
        )))
    }
}

// ─── Edit helpers ────────────────────────────────────────────────────────────

/// Pre-validate all edits exist, then apply them sequentially.
/// Returns the new file content without mutating the original.
fn apply_edits(original: &str, edits: &[EditBlock]) -> Result<String, ToolError> {
    // Pre-validate: every search string must be present
    for edit in edits {
        if !original.contains(&edit.search) {
            return Err(ToolError::EditNotFound);
        }
    }

    // Apply sequentially — each edit replaces first occurrence
    let mut result = original.to_string();
    for edit in edits {
        if let Some(pos) = result.find(&edit.search) {
            let mut next = String::with_capacity(result.len());
            next.push_str(&result[..pos]);
            next.push_str(&edit.replace);
            next.push_str(&result[pos + edit.search.len()..]);
            result = next;
        }
    }
    Ok(result)
}

// ─── Registration ────────────────────────────────────────────────────────────

/// Register all file tools with the given registry.
pub fn register_file_tools(registry: &mut ToolRegistry) {
    registry.register(FileReadTool);
    registry.register(FileWriteTool);
    registry.register(FileEditTool);
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::WorkspaceScope;
    use std::time::Duration;
    use tempfile::TempDir;

    fn test_ctx(workspace: &std::path::Path) -> ToolContext {
        ToolContext::new(WorkspaceScope::new(workspace), Duration::from_secs(30))
    }

    // ── FileReadTool ──

    #[tokio::test]
    async fn file_read_reads_full_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.txt");
        std::fs::write(&file, "line1\nline2\nline3").unwrap();

        let tool = FileReadTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({ "path": file.to_str().unwrap() });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content, "line1\nline2\nline3");
    }

    #[tokio::test]
    async fn file_read_with_offset_and_limit() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("lines.txt");
        std::fs::write(&file, "a\nb\nc\nd\ne").unwrap();

        let tool = FileReadTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({
            "path": file.to_str().unwrap(),
            "offset": 1,
            "limit": 2
        });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert_eq!(result.content, "b\nc");
    }

    #[tokio::test]
    async fn file_read_rejects_path_outside_workspace() {
        let workspace = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let file = outside.path().join("secret.txt");
        std::fs::write(&file, "secret").unwrap();

        let tool = FileReadTool;
        let ctx = test_ctx(workspace.path());
        let params = serde_json::json!({ "path": file.to_str().unwrap() });

        let result = tool.execute(&ctx, params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn file_read_invalid_params() {
        let tmp = TempDir::new().unwrap();
        let tool = FileReadTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({ "wrong_field": 123 });

        let result = tool.execute(&ctx, params).await;
        assert!(matches!(result, Err(ToolError::InvalidParams(_))));
    }

    // ── FileWriteTool ──

    #[tokio::test]
    async fn file_write_creates_new_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("new.txt");

        let tool = FileWriteTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({
            "path": file.to_str().unwrap(),
            "content": "hello world"
        });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("11 bytes"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn file_write_creates_nested_directories() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("a/b/c/deep.txt");

        let tool = FileWriteTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({
            "path": file.to_str().unwrap(),
            "content": "deep content"
        });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "deep content");
    }

    #[tokio::test]
    async fn file_write_overwrites_existing() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("existing.txt");
        std::fs::write(&file, "old").unwrap();

        let tool = FileWriteTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({
            "path": file.to_str().unwrap(),
            "content": "new"
        });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new");
    }

    #[tokio::test]
    async fn file_write_rejects_outside_workspace() {
        let workspace = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let file = outside.path().join("evil.txt");

        let tool = FileWriteTool;
        let ctx = test_ctx(workspace.path());
        let params = serde_json::json!({
            "path": file.to_str().unwrap(),
            "content": "evil"
        });

        let result = tool.execute(&ctx, params).await;
        assert!(result.is_err());
    }

    // ── FileEditTool ──

    #[tokio::test]
    async fn file_edit_replaces_text() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("edit_me.txt");
        std::fs::write(&file, "Hello World! Hello World!").unwrap();

        let tool = FileEditTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({
            "path": file.to_str().unwrap(),
            "edits": [
                { "search": "World", "replace": "Rust" }
            ]
        });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("1 edit(s)"));
        // Only first occurrence replaced
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "Hello Rust! Hello World!"
        );
    }

    #[tokio::test]
    async fn file_edit_multiple_edits() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("multi.txt");
        std::fs::write(&file, "fn foo() {}\nfn bar() {}").unwrap();

        let tool = FileEditTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({
            "path": file.to_str().unwrap(),
            "edits": [
                { "search": "fn foo()", "replace": "fn foo_renamed()" },
                { "search": "fn bar()", "replace": "fn bar_renamed()" }
            ]
        });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(result.content.contains("2 edit(s)"));
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("fn foo_renamed()"));
        assert!(content.contains("fn bar_renamed()"));
    }

    #[tokio::test]
    async fn file_edit_returns_error_when_search_not_found() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("no_match.txt");
        std::fs::write(&file, "Hello World").unwrap();

        let tool = FileEditTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({
            "path": file.to_str().unwrap(),
            "edits": [
                { "search": "NONEXISTENT", "replace": "something" }
            ]
        });

        let result = tool.execute(&ctx, params).await;
        assert!(matches!(result, Err(ToolError::EditNotFound)));
    }

    #[tokio::test]
    async fn file_edit_rejects_outside_workspace() {
        let workspace = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let file = outside.path().join("no.txt");
        std::fs::write(&file, "content").unwrap();

        let tool = FileEditTool;
        let ctx = test_ctx(workspace.path());
        let params = serde_json::json!({
            "path": file.to_str().unwrap(),
            "edits": [{ "search": "content", "replace": "hacked" }]
        });

        let result = tool.execute(&ctx, params).await;
        assert!(result.is_err());
    }

    // ── Registration ──

    #[test]
    fn register_file_tools_adds_three_tools() {
        let mut reg = ToolRegistry::new();
        register_file_tools(&mut reg);

        assert_eq!(reg.len(), 3);
        assert!(reg.get("file_read").is_some());
        assert!(reg.get("file_write").is_some());
        assert!(reg.get("file_edit").is_some());
    }
}
