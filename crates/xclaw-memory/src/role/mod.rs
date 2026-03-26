//! Role management: config, lifecycle, and daily memory.

pub mod config;
pub mod daily;
pub mod manager;

pub use config::{RoleConfig, RoleMeta};
pub use daily::{DailyMemory, FsDailyMemory};
pub use manager::{FsRoleManager, RoleManager};
