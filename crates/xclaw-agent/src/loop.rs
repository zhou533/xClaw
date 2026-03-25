//! Agent loop: the core execution engine that drives conversations.
//!
//! Receives user input, loads session context, injects memory,
//! builds prompts, calls the LLM, dispatches tool/skill calls,
//! and returns the final response.
