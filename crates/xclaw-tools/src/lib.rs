pub mod error;
pub mod security;
pub mod traits;

pub mod browser;
pub mod file;
pub mod file_io;
pub mod registry;
pub mod shell;
pub mod web_fetch;

// Re-exports for convenience
pub use error::ToolError;
pub use registry::ToolRegistry;
pub use traits::{Tool, ToolContext, ToolOutput, ToolSchema, WorkspaceScope};

/// Register all built-in tools with the given registry.
///
/// Currently registers:
/// - `file_read`, `file_write`, `file_edit` (from `file` module)
///
/// Future: shell, browser, http, memory, cron tools.
pub fn register_builtin_tools(registry: &mut ToolRegistry) {
    file::register_file_tools(registry);
}
