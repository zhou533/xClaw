//! Core trait and type definitions for the xclaw-tools system.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ToolError;

/// Defines the workspace boundary for tool execution.
#[derive(Debug, Clone)]
pub struct WorkspaceScope {
    pub workspace_root: PathBuf,
}

impl WorkspaceScope {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }
}

/// Security and resource context passed to every tool invocation.
///
/// `fs_allowlist` is private to prevent post-construction mutation of
/// the security boundary. Use [`with_extra_paths`](Self::with_extra_paths)
/// to add paths at construction time.
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub scope: WorkspaceScope,
    fs_allowlist: Vec<PathBuf>,
    pub net_allowlist: Vec<String>,
    pub timeout: Duration,
}

impl ToolContext {
    pub fn new(scope: WorkspaceScope, timeout: Duration) -> Self {
        let workspace_canonical = std::fs::canonicalize(&scope.workspace_root)
            .unwrap_or_else(|_| scope.workspace_root.clone());
        Self {
            fs_allowlist: vec![workspace_canonical],
            scope,
            net_allowlist: Vec::new(),
            timeout,
        }
    }

    /// Add extra allowed filesystem paths (canonicalized eagerly).
    pub fn with_extra_paths(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        for p in paths {
            let canonical = std::fs::canonicalize(&p).unwrap_or(p);
            self.fs_allowlist.push(canonical);
        }
        self
    }

    /// Read-only access to the filesystem allowlist.
    pub fn fs_allowlist(&self) -> &[PathBuf] {
        &self.fs_allowlist
    }
}

/// Output returned by a tool after execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// Schema description of a tool, used for LLM function-calling integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Core trait that all tools must implement.
///
/// Each tool is a stateless, atomic operation that executes within the
/// constraints of a [`ToolContext`].
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name identifying this tool (e.g., "file_read").
    fn name(&self) -> &str;

    /// Human-readable description of what this tool does.
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given context and parameters.
    async fn execute(
        &self,
        ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError>;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_scope_new() {
        let scope = WorkspaceScope::new("/home/user/project");
        assert_eq!(scope.workspace_root, PathBuf::from("/home/user/project"));
    }

    #[test]
    fn tool_context_defaults_allowlist_to_workspace_root() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = ToolContext::new(WorkspaceScope::new(tmp.path()), Duration::from_secs(30));
        assert_eq!(ctx.fs_allowlist().len(), 1);
        assert!(ctx.net_allowlist.is_empty());
        assert_eq!(ctx.timeout, Duration::from_secs(30));
    }

    #[test]
    fn tool_context_with_extra_paths() {
        let tmp = tempfile::TempDir::new().unwrap();
        let extra = tempfile::TempDir::new().unwrap();
        let ctx = ToolContext::new(WorkspaceScope::new(tmp.path()), Duration::from_secs(30))
            .with_extra_paths(vec![extra.path().to_path_buf()]);
        assert_eq!(ctx.fs_allowlist().len(), 2);
    }

    #[test]
    fn tool_output_success() {
        let out = ToolOutput::success("file contents here");
        assert_eq!(out.content, "file contents here");
        assert!(!out.is_error);
    }

    #[test]
    fn tool_output_error() {
        let out = ToolOutput::error("something went wrong");
        assert_eq!(out.content, "something went wrong");
        assert!(out.is_error);
    }

    #[test]
    fn tool_output_serializes() {
        let out = ToolOutput::success("data");
        let json = serde_json::to_string(&out).unwrap();
        let back: ToolOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, "data");
        assert!(!back.is_error);
    }

    #[test]
    fn tool_schema_serializes() {
        let schema = ToolSchema {
            name: "file_read".to_string(),
            description: "Read file contents".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        };
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json["name"], "file_read");
        assert_eq!(json["parameters"]["required"][0], "path");
    }

    /// Verify that Tool trait is dyn-compatible (object-safe).
    #[test]
    fn tool_trait_is_object_safe() {
        struct DummyTool;

        #[async_trait]
        impl Tool for DummyTool {
            fn name(&self) -> &str {
                "dummy"
            }
            fn description(&self) -> &str {
                "A dummy tool"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn execute(
                &self,
                _ctx: &ToolContext,
                _params: serde_json::Value,
            ) -> Result<ToolOutput, ToolError> {
                Ok(ToolOutput::success("ok"))
            }
        }

        // This line proves the trait is object-safe: we can create Box<dyn Tool>
        let tool: Box<dyn Tool> = Box::new(DummyTool);
        assert_eq!(tool.name(), "dummy");
    }
}
