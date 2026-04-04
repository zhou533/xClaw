//! Session subsystem — conversation history with file-based persistence.

pub(crate) mod expiry;
pub mod fs_store;
pub mod policy;
pub mod record_id;
pub mod store;
pub mod time_util;
pub mod types;

pub use fs_store::FsSessionStore;
pub use policy::SessionPolicy;
pub use record_id::{RecordId, generate_record_id};
pub use store::SessionStore;
pub use types::{
    ContentBlock, ContentBlockKind, ImageSource, SessionEntry, SessionIndex, SessionSummary,
    StopReason, TokenUsage, TranscriptRecord, TranscriptRole,
};
