//! Version management for code agents (Claude Code, Codex, Gemini CLI, OpenCode, etc.)
//!
//! Handles detecting installed version, listing available versions,
//! and switching between versions for multiple agents.

mod antigravity;
pub mod cache;
mod claude;
pub mod cli;
mod codex;
mod compare;
mod gemini;
mod hermes;
mod manager;
mod opencode;
mod pi;
#[cfg(test)]
mod tests;
pub mod types;
mod unleash_self;

pub use cache::*;
pub use cli::{
    install_latest_streaming, install_version, list_versions, show_current, show_current_json,
};
pub(crate) use compare::{version_compare, version_less_than};
pub use manager::*;
pub use types::*;
