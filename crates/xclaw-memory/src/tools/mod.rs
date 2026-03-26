//! Memory tools for LLM function calling.
//!
//! These tools allow the LLM to manage roles, read/write memory files,
//! and append daily memory via the standard `ToolRegistry`.

pub mod memory_file_tools;
pub mod memory_tools;
pub mod role_tools;

use std::path::PathBuf;

use xclaw_core::types::RoleId;
use xclaw_tools::error::ToolError;
use xclaw_tools::registry::ToolRegistry;

use crate::error::MemoryError;

/// Parse optional `role` parameter (defaults to "default").
pub(crate) fn parse_role(params: &serde_json::Value) -> Result<RoleId, ToolError> {
    let name = params["role"].as_str().unwrap_or("default");
    RoleId::new(name).map_err(|e| ToolError::InvalidParams(e.to_string()))
}

/// Convert `MemoryError` to `ToolError` with semantic mapping.
pub(crate) fn to_tool_error(e: MemoryError) -> ToolError {
    match e {
        MemoryError::RoleNotFound(_)
        | MemoryError::InvalidRoleId(_)
        | MemoryError::RoleAlreadyExists(_)
        | MemoryError::InvalidDate(_) => ToolError::InvalidParams(e.to_string()),
        _ => ToolError::Internal(e.to_string()),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_tool_error_maps_role_not_found_to_invalid_params() {
        let err = to_tool_error(MemoryError::RoleNotFound("ghost".into()));
        assert!(matches!(err, ToolError::InvalidParams(_)));
        assert!(err.to_string().contains("ghost"));
    }

    #[test]
    fn to_tool_error_maps_role_already_exists_to_invalid_params() {
        let err = to_tool_error(MemoryError::RoleAlreadyExists("dup".into()));
        assert!(matches!(err, ToolError::InvalidParams(_)));
    }

    #[test]
    fn to_tool_error_maps_invalid_role_id_to_invalid_params() {
        let err = to_tool_error(MemoryError::InvalidRoleId("BAD".into()));
        assert!(matches!(err, ToolError::InvalidParams(_)));
    }

    #[test]
    fn to_tool_error_maps_invalid_date_to_invalid_params() {
        let err = to_tool_error(MemoryError::InvalidDate("bad".into()));
        assert!(matches!(err, ToolError::InvalidParams(_)));
    }

    #[test]
    fn to_tool_error_maps_io_to_internal() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let err = to_tool_error(MemoryError::Io(io_err));
        assert!(matches!(err, ToolError::Internal(_)));
    }

    #[test]
    fn to_tool_error_maps_yaml_parse_to_internal() {
        let err = to_tool_error(MemoryError::YamlParse("bad yaml".into()));
        assert!(matches!(err, ToolError::Internal(_)));
    }
}

pub use memory_file_tools::{MemoryFileReadTool, MemoryFileWriteTool};
pub use memory_tools::{MemoryDailyAppendTool, MemoryDailyReadTool};
pub use role_tools::{RoleCreateTool, RoleDeleteTool, RoleGetTool, RoleListTool};

/// Register all memory-related tools into a `ToolRegistry`.
pub fn register_memory_tools(registry: &mut ToolRegistry, base_dir: PathBuf) {
    // Role tools
    registry.register(RoleCreateTool::new(&base_dir));
    registry.register(RoleListTool::new(&base_dir));
    registry.register(RoleGetTool::new(&base_dir));
    registry.register(RoleDeleteTool::new(&base_dir));
    // Memory file tools
    registry.register(MemoryFileReadTool::new(&base_dir));
    registry.register(MemoryFileWriteTool::new(&base_dir));
    // Daily tools
    registry.register(MemoryDailyAppendTool::new(&base_dir));
    registry.register(MemoryDailyReadTool::new(&base_dir));
}
