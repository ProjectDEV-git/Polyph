//! Polyph's built-in ACP coding agent for OpenAI-compatible endpoints.

pub mod agent;
pub mod llm;
pub mod server;
pub mod textproto;
pub mod tools;

pub use server::run;
