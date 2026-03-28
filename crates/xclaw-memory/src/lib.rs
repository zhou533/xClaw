pub mod error;
pub mod facade;
pub mod role;
pub mod search;
pub mod session;
pub mod tools;
pub mod traits;
pub mod workspace;

// Re-exports
pub use error::MemoryError;
pub use facade::{FsMemorySystem, MemorySystem};
pub use role::{DailyMemory, FsDailyMemory, FsRoleManager, RoleConfig, RoleManager, RoleMeta};
pub use search::{MemorySearcher, SearchResult};
pub use session::{
    FsSessionStore, SessionEntry, SessionIndex, SessionStore, SessionSummary, TranscriptRecord,
};
pub use tools::register_memory_tools;
pub use traits::{MemoryEntry, MemoryStore};
pub use workspace::{FsMemoryFileLoader, MemoryFileKind, MemoryFileLoader, MemorySnapshot};
