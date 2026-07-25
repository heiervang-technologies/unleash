//! Multi-agent management for unleash
//!
//! Manages different code agents (Claude Code, Codex, etc.) including:
//! - Agent definitions and configuration
//! - Version tracking and updates
//! - Installation management

pub mod custom_cli;
mod definition;
pub(crate) mod install;
mod manager;
#[cfg(test)]
mod tests;
mod types;

pub use custom_cli::{
    add_custom_agent_cli, add_custom_agent_with, build_custom_agent_config, AddCustomAgentArgs,
};
pub use definition::*;
pub(crate) use install::atomic_install_binary;
pub use manager::*;
pub use types::*;
