//! Unified memory file read/write tools for LLM function calling.

use std::path::PathBuf;

use async_trait::async_trait;
use xclaw_tools::error::ToolError;
use xclaw_tools::traits::{Tool, ToolContext, ToolOutput};

use super::{parse_role, to_tool_error};
use crate::workspace::loader::{FsMemoryFileLoader, MemoryFileLoader};
use crate::workspace::types::MemoryFileKind;

fn parse_kind(params: &serde_json::Value) -> Result<MemoryFileKind, ToolError> {
    let kind_str = params["kind"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidParams("kind is required".into()))?;
    MemoryFileKind::from_str_name(kind_str).ok_or_else(|| {
        ToolError::InvalidParams(format!(
            "invalid memory file kind: '{kind_str}' \
             (valid: agents, soul, tools, identity, user, heartbeat, bootstrap, long_term)"
        ))
    })
}

// ─── MemoryFileReadTool ─────────────────────────────────────────────────────

pub struct MemoryFileReadTool {
    base_dir: PathBuf,
}

impl MemoryFileReadTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryFileReadTool {
    fn name(&self) -> &str {
        "memory_file_read"
    }

    fn description(&self) -> &str {
        "Read a memory file (MEMORY.md, SOUL.md, AGENTS.md, etc.) for a role"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role name (default: 'default')" },
                "kind": {
                    "type": "string",
                    "enum": ["agents", "soul", "tools", "identity", "user", "heartbeat", "bootstrap", "long_term"],
                    "description": "Memory file kind"
                }
            },
            "required": ["kind"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let role = parse_role(&params)?;
        let kind = parse_kind(&params)?;

        let loader = FsMemoryFileLoader::new(&self.base_dir);
        let content = loader.load_file(&role, kind).await.map_err(to_tool_error)?;

        match content {
            Some(c) => Ok(ToolOutput::success(c)),
            None => Ok(ToolOutput::success(format!(
                "{} does not exist for role '{}'",
                kind.filename(),
                role
            ))),
        }
    }
}

// ─── MemoryFileWriteTool ────────────────────────────────────────────────────

pub struct MemoryFileWriteTool {
    base_dir: PathBuf,
}

impl MemoryFileWriteTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryFileWriteTool {
    fn name(&self) -> &str {
        "memory_file_write"
    }

    fn description(&self) -> &str {
        "Write a memory file (MEMORY.md, SOUL.md, AGENTS.md, etc.) for a role"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role name (default: 'default')" },
                "kind": {
                    "type": "string",
                    "enum": ["agents", "soul", "tools", "identity", "user", "heartbeat", "bootstrap", "long_term"],
                    "description": "Memory file kind"
                },
                "content": { "type": "string", "description": "File content to write" }
            },
            "required": ["kind", "content"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let role = parse_role(&params)?;
        let kind = parse_kind(&params)?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("content is required".into()))?;

        let loader = FsMemoryFileLoader::new(&self.base_dir);
        loader
            .save_file(&role, kind, content)
            .await
            .map_err(to_tool_error)?;

        Ok(ToolOutput::success(format!(
            "{} saved for role '{}'",
            kind.filename(),
            role
        )))
    }
}
