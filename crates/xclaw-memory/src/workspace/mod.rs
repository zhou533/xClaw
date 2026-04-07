//! Memory files (AGENTS.md, SOUL.md, MEMORY.md, etc.).

pub mod loader;
pub mod templates;
pub mod types;

pub use loader::{FsMemoryFileLoader, MemoryFileLoader};
pub use templates::{bootstrap_template, seed_new_role_templates, supplement_missing_templates};
pub use types::{MemoryFileKind, MemorySnapshot};
