//! Memory files (AGENTS.md, SOUL.md, MEMORY.md, etc.).

pub mod loader;
pub mod templates;
pub mod types;

pub use loader::{FsMemoryFileLoader, MemoryFileLoader};
pub use templates::{bootstrap_template, ensure_bootstrap_templates};
pub use types::{MemoryFileKind, MemorySnapshot};
