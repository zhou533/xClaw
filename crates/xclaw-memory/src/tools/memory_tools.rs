//! Daily memory tools for LLM function calling.

use std::path::PathBuf;

use async_trait::async_trait;
use xclaw_tools::error::ToolError;
use xclaw_tools::traits::{Tool, ToolContext, ToolOutput};

use super::{parse_role, to_tool_error};
use crate::role::daily::{DailyMemory, FsDailyMemory, today};

// ─── MemoryDailyAppendTool ──────────────────────────────────────────────────

pub struct MemoryDailyAppendTool {
    base_dir: PathBuf,
}

impl MemoryDailyAppendTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryDailyAppendTool {
    fn name(&self) -> &str {
        "memory_daily_append"
    }

    fn description(&self) -> &str {
        "Append an entry to today's daily memory"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role name (default: 'default')" },
                "entry": { "type": "string", "description": "Memory entry to append" }
            },
            "required": ["entry"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let role = parse_role(&params)?;
        let entry = params["entry"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("entry is required".into()))?;

        let dm = FsDailyMemory::new(&self.base_dir);
        dm.append(&role, entry).await.map_err(to_tool_error)?;

        Ok(ToolOutput::success(format!(
            "Entry appended to daily memory ({})",
            today()
        )))
    }
}

// ─── MemoryDailyReadTool ────────────────────────────────────────────────────

pub struct MemoryDailyReadTool {
    base_dir: PathBuf,
}

impl MemoryDailyReadTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryDailyReadTool {
    fn name(&self) -> &str {
        "memory_daily_read"
    }

    fn description(&self) -> &str {
        "Read daily memory for a specific date"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role name (default: 'default')" },
                "date": { "type": "string", "description": "Date in YYYY-MM-DD format" }
            },
            "required": ["date"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let role = parse_role(&params)?;
        let date = params["date"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("date is required".into()))?;

        let dm = FsDailyMemory::new(&self.base_dir);
        let content = dm.load_day(&role, date).await.map_err(to_tool_error)?;

        Ok(ToolOutput::success(content))
    }
}
