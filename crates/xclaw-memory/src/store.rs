//! MemoryStore implementation with layered storage strategy.
//!
//! - Hot data: in-memory (`DashMap`) for active session context
//! - Warm data: SQLite for recent conversation history
//! - Cold data: filesystem (JSON) for long-term memory and exports
