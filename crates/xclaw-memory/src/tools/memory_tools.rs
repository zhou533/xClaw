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
        "Append a Markdown entry to today's daily memory file. Each day has a separate file under the role's memory directory. The entry is appended to the end of the file."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role identifier in snake_case (default: 'default')" },
                "entry": { "type": "string", "description": "Markdown text to append to today's daily memory" }
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
        "Read the full content of a daily memory file for a given date (YYYY-MM-DD). Returns the entire day's entries as Markdown text, or an empty string if no entries exist for that date."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role identifier in snake_case (default: 'default')" },
                "date": { "type": "string", "description": "Date in YYYY-MM-DD format (e.g. 2026-04-03)" }
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
