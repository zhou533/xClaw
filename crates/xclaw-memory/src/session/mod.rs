//! Session subsystem — conversation history with file-based persistence.

pub mod fs_store;
pub mod store;
pub mod types;

pub use fs_store::FsSessionStore;
pub use store::SessionStore;
pub use types::{SessionEntry, SessionIndex, SessionSummary, TranscriptRecord};
