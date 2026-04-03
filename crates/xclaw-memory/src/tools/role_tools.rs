//! Role management tools for LLM function calling.

use std::path::PathBuf;

use async_trait::async_trait;
use xclaw_core::types::RoleId;
use xclaw_tools::error::ToolError;
use xclaw_tools::traits::{Tool, ToolContext, ToolOutput};

use super::to_tool_error;
use crate::role::config::RoleConfig;
use crate::role::manager::{FsRoleManager, RoleManager};

// ─── RoleCreateTool ──────────────────────────────────────────────────────────

pub struct RoleCreateTool {
    base_dir: PathBuf,
}

impl RoleCreateTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for RoleCreateTool {
    fn name(&self) -> &str {
        "role_create"
    }

    fn description(&self) -> &str {
        "Create a new role with its configuration and initialize its memory directory with bootstrap templates. Role name must be snake_case."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Role identifier in snake_case" },
                "description": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Role description lines"
                },
                "system_prompt": { "type": "string", "description": "System prompt for this role" },
                "tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tool whitelist for this role"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("name is required".into()))?;

        // Pre-validate role name before building config
        let _ = RoleId::new(name).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let config = RoleConfig {
            name: name.to_string(),
            description: params["description"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            system_prompt: params["system_prompt"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            tools: params["tools"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            meta: Default::default(),
            memory_dir: format!("roles/{name}"),
        };

        let mgr = FsRoleManager::new(&self.base_dir);
        mgr.create_role(config).await.map_err(to_tool_error)?;

        Ok(ToolOutput::success(format!(
            "Role '{name}' created successfully"
        )))
    }
}

// ─── RoleListTool ────────────────────────────────────────────────────────────

pub struct RoleListTool {
    base_dir: PathBuf,
}

impl RoleListTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for RoleListTool {
    fn name(&self) -> &str {
        "role_list"
    }

    fn description(&self) -> &str {
        "List all available roles. Returns a JSON array of role names."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        _params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let mgr = FsRoleManager::new(&self.base_dir);
        let roles = mgr.list_roles().await.map_err(to_tool_error)?;

        let names: Vec<&str> = roles.iter().map(|r| r.name.as_str()).collect();
        let json =
            serde_json::to_string_pretty(&names).map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(ToolOutput::success(json))
    }
}

// ─── RoleGetTool ─────────────────────────────────────────────────────────────

pub struct RoleGetTool {
    base_dir: PathBuf,
}

impl RoleGetTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for RoleGetTool {
    fn name(&self) -> &str {
        "role_get"
    }

    fn description(&self) -> &str {
        "Get the full configuration of a specific role. Returns YAML with name, description, system_prompt, tools, and memory_dir fields."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Role identifier in snake_case" }
            },
            "required": ["name"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("name is required".into()))?;

        let role_id = RoleId::new(name).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let mgr = FsRoleManager::new(&self.base_dir);
        let config = mgr.get_role(&role_id).await.map_err(to_tool_error)?;

        let yaml = config.to_yaml().map_err(to_tool_error)?;
        Ok(ToolOutput::success(yaml))
    }
}

// ─── RoleDeleteTool ──────────────────────────────────────────────────────────

pub struct RoleDeleteTool {
    base_dir: PathBuf,
}

impl RoleDeleteTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for RoleDeleteTool {
    fn name(&self) -> &str {
        "role_delete"
    }

    fn description(&self) -> &str {
        "Delete a role and all its memory files. The 'default' role cannot be deleted."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Role identifier in snake_case" }
            },
            "required": ["name"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("name is required".into()))?;

        let role_id = RoleId::new(name).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let mgr = FsRoleManager::new(&self.base_dir);
        mgr.delete_role(&role_id).await.map_err(to_tool_error)?;

        Ok(ToolOutput::success(format!("Role '{name}' deleted")))
    }
}
