//! Tool and skill dispatch coordination.
//!
//! When the LLM requests a tool or skill invocation, this module
//! routes the call to the appropriate handler and feeds the result
//! back into the agent loop.
