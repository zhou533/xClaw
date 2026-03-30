//! Session subsystem — conversation history with file-based persistence.

pub(crate) mod expiry;
pub mod fs_store;
pub mod policy;
pub mod store;
pub(crate) mod time_util;
pub mod types;

pub use fs_store::FsSessionStore;
pub use policy::SessionPolicy;
pub use store::SessionStore;
pub use types::{SessionEntry, SessionIndex, SessionSummary, TranscriptRecord};
