//! Version management for code agents (Claude Code, Codex, Gemini CLI, OpenCode, etc.)
//!
//! Handles detecting installed version, listing available versions,
//! and switching between versions for multiple agents.

pub mod types;
pub mod cache;
mod compare;
mod manager;
mod claude;
mod unleash_self;
mod codex;
mod gemini;
mod pi;
mod hermes;
mod opencode;
mod antigravity;
pub mod cli;
#[cfg(test)]
mod tests;

pub use types::*;
pub use cache::*;
pub use manager::*;
pub(crate) use compare::{version_compare, version_less_than};
pub use cli::{list_versions, install_version, show_current, show_current_json, install_latest_streaming};
