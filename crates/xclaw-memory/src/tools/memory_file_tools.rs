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
        "Read a role's memory file by kind. Supported files: AGENTS.md (collaboration rules), SOUL.md (AI persona), TOOLS.md (tool guidance), IDENTITY.md (self-identity), USER.md (user preferences), HEARTBEAT.md (action reference), BOOTSTRAP.md (workspace bootstrap), MEMORY.md (long-term knowledge). Returns the file content or a message if it does not exist."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role identifier in snake_case (default: 'default')" },
                "kind": {
                    "type": "string",
                    "enum": ["agents", "soul", "tools", "identity", "user", "heartbeat", "bootstrap", "long_term"],
                    "description": "Memory file kind: agents=collaboration rules, soul=AI persona, tools=tool guidance, identity=self-identity, user=user preferences, heartbeat=action reference, bootstrap=workspace bootstrap, long_term=distilled knowledge (MEMORY.md)"
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

// ─── MemoryFileDeleteTool ───────────────────────────────────────────────────

pub struct MemoryFileDeleteTool {
    base_dir: PathBuf,
}

impl MemoryFileDeleteTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryFileDeleteTool {
    fn name(&self) -> &str {
        "memory_file_delete"
    }

    fn description(&self) -> &str {
        "Delete a role's memory file by kind. Primarily used to remove BOOTSTRAP.md after workspace bootstrap is complete. Other memory file kinds can also be deleted."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role identifier in snake_case (default: 'default')" },
                "kind": {
                    "type": "string",
                    "enum": ["agents", "soul", "tools", "identity", "user", "heartbeat", "bootstrap", "long_term"],
                    "description": "Memory file kind: agents=collaboration rules, soul=AI persona, tools=tool guidance, identity=self-identity, user=user preferences, heartbeat=action reference, bootstrap=workspace bootstrap, long_term=distilled knowledge (MEMORY.md)"
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
        let deleted = loader
            .delete_file(&role, kind)
            .await
            .map_err(to_tool_error)?;

        if deleted {
            Ok(ToolOutput::success(format!(
                "{} deleted for role '{}'",
                kind.filename(),
                role
            )))
        } else {
            Ok(ToolOutput::success(format!(
                "{} does not exist for role '{}'",
                kind.filename(),
                role
            )))
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
        "Write (overwrite) a role's memory file by kind. The entire file content is replaced. Creates the file if it does not exist. Supported kinds: agents, soul, tools, identity, user, heartbeat, bootstrap, long_term."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role identifier in snake_case (default: 'default')" },
                "kind": {
                    "type": "string",
                    "enum": ["agents", "soul", "tools", "identity", "user", "heartbeat", "bootstrap", "long_term"],
                    "description": "Memory file kind: agents=collaboration rules, soul=AI persona, tools=tool guidance, identity=self-identity, user=user preferences, heartbeat=action reference, bootstrap=workspace bootstrap, long_term=distilled knowledge (MEMORY.md)"
                },
                "content": { "type": "string", "description": "Markdown content to write. Replaces the entire file." }
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
