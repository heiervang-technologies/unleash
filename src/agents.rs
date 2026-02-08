//! Multi-agent management for Claude Unleashed
//!
//! Manages different code agents (Claude Code, Codex, etc.) including:
//! - Agent definitions and configuration
//! - Version tracking and updates
//! - Installation management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

/// Supported agent types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Claude,
    Codex,
}

impl AgentType {
    /// All agent types in stable order (used for TUI cycling)
    pub fn all() -> &'static [AgentType] {
        &[AgentType::Claude, AgentType::Codex]
    }


    pub fn display_name(&self) -> &'static str {
        match self {
            AgentType::Claude => "Claude Code",
            AgentType::Codex => "Codex",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" | "claude-code" => Some(AgentType::Claude),
            "codex" => Some(AgentType::Codex),
            _ => None,
        }
    }
}

/// Agent definition with installation and version info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// Agent type
    pub agent_type: AgentType,
    /// Display name
    pub name: String,
    /// Binary name to execute
    pub binary: String,
    /// Description
    pub description: String,
    /// GitHub repository (owner/repo)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_repo: Option<String>,
    /// NPM package name (for npm-based agents)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npm_package: Option<String>,
    /// Whether this agent is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl AgentDefinition {
    /// Create Claude Code agent definition
    pub fn claude() -> Self {
        Self {
            agent_type: AgentType::Claude,
            name: "Claude Code".to_string(),
            binary: "claude".to_string(),
            description: "Anthropic's Claude Code CLI".to_string(),
            github_repo: Some("anthropics/claude-code".to_string()),
            npm_package: Some("@anthropic-ai/claude-code".to_string()),
            enabled: true,
        }
    }

    /// Create Codex agent definition
    pub fn codex() -> Self {
        Self {
            agent_type: AgentType::Codex,
            name: "Codex".to_string(),
            binary: "codex".to_string(),
            description: "OpenAI Codex CLI".to_string(),
            github_repo: Some("openai/codex".to_string()),
            npm_package: None,
            enabled: true,
        }
    }

}

/// Version information for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct AgentVersion {
    /// Current installed version
    pub installed: Option<String>,
    /// Latest available version
    pub latest: Option<String>,
    /// Binary path
    pub binary_path: Option<PathBuf>,
    /// Last checked timestamp
    pub last_checked: Option<u64>,
}


/// Agent manager for handling multiple code agents
pub struct AgentManager {
    /// Agent definitions
    agents: HashMap<AgentType, AgentDefinition>,
    /// Version cache
    versions: HashMap<AgentType, AgentVersion>,
    /// Config directory
    config_dir: PathBuf,
}

impl AgentManager {
    /// Create a new AgentManager
    pub fn new() -> io::Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Config directory not found"))?
            .join("agent-unleashed");

        fs::create_dir_all(&config_dir)?;

        let mut manager = Self {
            agents: HashMap::new(),
            versions: HashMap::new(),
            config_dir,
        };

        // Register default agents
        manager.register_agent(AgentDefinition::claude());
        manager.register_agent(AgentDefinition::codex());

        // Load cached versions
        manager.load_version_cache()?;

        Ok(manager)
    }

    /// Register an agent definition
    pub fn register_agent(&mut self, agent: AgentDefinition) {
        self.agents.insert(agent.agent_type, agent);
    }

    /// Get an agent definition
    pub fn get_agent(&self, agent_type: AgentType) -> Option<&AgentDefinition> {
        self.agents.get(&agent_type)
    }

    /// List all registered agents
    pub fn list_agents(&self) -> Vec<&AgentDefinition> {
        self.agents.values().collect()
    }

    /// Get installed version for an agent
    pub fn get_installed_version(&mut self, agent_type: AgentType) -> io::Result<Option<String>> {
        let agent = self
            .agents
            .get(&agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Agent not found"))?;

        // Try to get version from binary
        let output = Command::new(&agent.binary).arg("--version").output();

        match output {
            Ok(out) if out.status.success() => {
                let version_str = String::from_utf8_lossy(&out.stdout);
                let version = Self::parse_version(&version_str);

                // Update cache
                let entry = self.versions.entry(agent_type).or_default();
                entry.installed = version.clone();
                entry.binary_path = which::which(&agent.binary).ok();

                Ok(version)
            }
            _ => Ok(None),
        }
    }

    /// Parse version string from command output
    fn parse_version(output: &str) -> Option<String> {
        // Handle various version formats:
        // "claude 2.1.22" -> "2.1.22"
        // "codex 0.1.0" -> "0.1.0"
        // "v1.2.3" -> "1.2.3"
        let line = output.lines().next()?;
        let parts: Vec<&str> = line.split_whitespace().collect();

        for part in parts {
            let cleaned = part.trim_start_matches('v').trim_end_matches(')');
            if cleaned
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
            {
                return Some(cleaned.to_string());
            }
        }

        None
    }

    /// Get latest version from GitHub
    pub fn get_latest_version(&mut self, agent_type: AgentType) -> io::Result<Option<String>> {
        let agent = self
            .agents
            .get(&agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Agent not found"))?;

        let repo = match &agent.github_repo {
            Some(r) => r,
            None => return Ok(None),
        };

        // Use GitHub API to get latest release
        let url = format!("https://api.github.com/repos/{}/releases/latest", repo);

        let output = Command::new("curl")
            .args(["-s", "-H", "Accept: application/vnd.github.v3+json", &url])
            .output()?;

        if !output.status.success() {
            return Ok(None);
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let tag = json
            .get("tag_name")
            .and_then(|t| t.as_str())
            .map(|s| s.trim_start_matches('v').to_string());

        // Update cache
        if let Some(ref version) = tag {
            let entry = self.versions.entry(agent_type).or_default();
            entry.latest = Some(version.clone());
            entry.last_checked = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );
        }

        Ok(tag)
    }

    /// Check if an update is available
    pub fn check_update(&mut self, agent_type: AgentType) -> io::Result<bool> {
        let installed = self.get_installed_version(agent_type)?;
        let latest = self.get_latest_version(agent_type)?;

        match (installed, latest) {
            (Some(i), Some(l)) => Ok(Self::version_compare(&i, &l) < 0),
            _ => Ok(false),
        }
    }

    /// Compare version strings (returns -1, 0, or 1)
    fn version_compare(a: &str, b: &str) -> i32 {
        let parse = |s: &str| -> Vec<u32> {
            s.split('.')
                .filter_map(|p| p.parse().ok())
                .collect()
        };

        let va = parse(a);
        let vb = parse(b);

        for i in 0..va.len().max(vb.len()) {
            let pa = va.get(i).copied().unwrap_or(0);
            let pb = vb.get(i).copied().unwrap_or(0);
            if pa < pb {
                return -1;
            }
            if pa > pb {
                return 1;
            }
        }
        0
    }

    /// Update an agent to latest version
    pub fn update_agent(&mut self, agent_type: AgentType) -> io::Result<String> {
        // Validate agent exists
        self.agents
            .get(&agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Agent not found"))?;

        match agent_type {
            AgentType::Claude => self.update_claude(),
            AgentType::Codex => self.update_codex(),
        }
    }

    /// Update Claude Code via npm
    fn update_claude(&self) -> io::Result<String> {
        let output = Command::new("npm")
            .args(["install", "-g", "@anthropic-ai/claude-code@latest"])
            .output()?;

        if output.status.success() {
            Ok("Claude Code updated successfully".to_string())
        } else {
            Err(io::Error::other(
                format!(
                    "Failed to update Claude Code: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ))
        }
    }

    /// Update Codex - build from source
    fn update_codex(&self) -> io::Result<String> {
        // Find the codex submodule - check multiple possible locations
        let possible_paths = [
            // Relative to current dir (if running from repo)
            std::env::current_dir()
                .ok()
                .map(|p| p.join("codex-unleashed/codex")),
            // Relative to executable location
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .map(|p| p.join("../codex-unleashed/codex")),
            // Known locations in home dir (new and legacy paths)
            dirs::home_dir().map(|p| p.join("ht/agent-unleashed/codex-unleashed/codex")),
            dirs::home_dir().map(|p| p.join("agent-unleashed/codex-unleashed/codex")),
            // Legacy paths
            dirs::home_dir().map(|p| p.join("ht/claude-unleashed/codex-unleashed/codex")),
            dirs::home_dir().map(|p| p.join("claude-unleashed/codex-unleashed/codex")),
        ];

        let codex_dir = possible_paths
            .into_iter()
            .flatten()
            .find(|p| p.exists())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Codex submodule not found. Clone agent-unleashed repo with submodules.",
                )
            })?;

        let repo_dir = codex_dir
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find repo root"))?;

        // The Rust code is in codex-rs subdirectory
        let codex_rs_dir = codex_dir.join("codex-rs");
        if !codex_rs_dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Codex codex-rs directory not found",
            ));
        }

        println!("Found codex at: {}", codex_dir.display());

        // Update submodule
        println!("Updating submodule...");
        let output = Command::new("git")
            .args(["submodule", "update", "--remote", "codex-unleashed/codex"])
            .current_dir(repo_dir)
            .output()?;

        if !output.status.success() {
            return Err(io::Error::other(
                format!(
                    "Failed to update codex submodule: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ));
        }

        // Build codex CLI (package is codex-cli, binary is codex)
        println!("Building codex (this may take a while)...");
        let output = Command::new("cargo")
            .args(["build", "--release", "-p", "codex-cli"])
            .current_dir(&codex_rs_dir)
            .output()?;

        if output.status.success() {
            // Install the binary
            let binary_path = codex_rs_dir.join("target/release/codex");
            let install_path = dirs::home_dir()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?
                .join(".local/bin/codex");

            fs::create_dir_all(install_path.parent().unwrap())?;
            fs::copy(&binary_path, &install_path)?;

            Ok(format!(
                "Codex updated and installed to {}",
                install_path.display()
            ))
        } else {
            Err(io::Error::other(
                format!(
                    "Failed to build Codex: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ))
        }
    }

    /// Get version cache file path
    fn version_cache_path(&self) -> PathBuf {
        self.config_dir.join("agent-versions.json")
    }

    /// Load version cache from disk
    fn load_version_cache(&mut self) -> io::Result<()> {
        let path = self.version_cache_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            self.versions = serde_json::from_str(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        }
        Ok(())
    }

    /// Save version cache to disk
    pub fn save_version_cache(&self) -> io::Result<()> {
        let path = self.version_cache_path();
        let content = serde_json::to_string_pretty(&self.versions)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        fs::write(path, content)
    }

    /// Get status summary for all agents
    pub fn status_summary(&mut self) -> Vec<(AgentType, Option<String>, Option<String>, bool)> {
        let agent_types: Vec<AgentType> = self.agents.keys().copied().collect();
        let mut results = Vec::new();

        for agent_type in agent_types {
            let installed = self.get_installed_version(agent_type).ok().flatten();
            let latest = self.versions.get(&agent_type).and_then(|v| v.latest.clone());
            let update_available = match (&installed, &latest) {
                (Some(i), Some(l)) => Self::version_compare(i, l) < 0,
                _ => false,
            };
            results.push((agent_type, installed, latest, update_available));
        }

        results
    }
}
