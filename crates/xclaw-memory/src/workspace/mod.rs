//! Memory files (AGENTS.md, SOUL.md, MEMORY.md, etc.).

pub mod loader;
pub mod types;

pub use loader::{FsMemoryFileLoader, MemoryFileLoader};
pub use types::{MemoryFileKind, MemorySnapshot};
