//! Multi-agent management for unleash
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
    Gemini,
    OpenCode,
}

impl AgentType {
    /// All agent types in stable order (used for TUI cycling)
    pub fn all() -> &'static [AgentType] {
        &[
            AgentType::Claude,
            AgentType::Codex,
            AgentType::Gemini,
            AgentType::OpenCode,
        ]
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AgentType::Claude => "Claude Code",
            AgentType::Codex => "Codex",
            AgentType::Gemini => "Gemini CLI",
            AgentType::OpenCode => "OpenCode",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" | "claude-code" => Some(AgentType::Claude),
            "codex" => Some(AgentType::Codex),
            "gemini" | "gemini-cli" => Some(AgentType::Gemini),
            "opencode" | "open-code" => Some(AgentType::OpenCode),
            _ => None,
        }
    }
}

/// Headless mode strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeadlessStrategy {
    /// Use a flag (e.g., -p)
    Flag(String),
    /// Use a subcommand (e.g., exec)
    Subcommand(String),
}

/// Fork strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForkStrategy {
    /// Use a flag (e.g., --fork)
    Flag(String),
    /// Use a subcommand (e.g., fork)
    Subcommand(String),
    /// Not supported by this agent
    Unsupported,
}

/// Sandbox mode strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxStrategy {
    /// Boolean flag (e.g., Gemini: --sandbox)
    BoolFlag(String),
    /// Flag with a fixed value (e.g., Codex: --sandbox workspace-write)
    ValueFlag(String, String),
    /// Not supported by this agent
    Unsupported,
}

/// Session management strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStrategy {
    /// Argument for continuing last session (e.g., "-c", "resume --last")
    pub continue_arg: String,
    /// Argument for resuming specific session (e.g., "-r", "resume")
    pub resume_arg: String,
}

/// Polyfill configuration for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPolyfillConfig {
    /// Strategy for headless mode
    pub headless: HeadlessStrategy,
    /// Strategy for session management
    pub session: SessionStrategy,
    /// Strategy for session forking
    pub fork: ForkStrategy,
    /// Flag name for YOLO mode (permission bypass), if any
    pub yolo_flag: Option<String>,
    /// Flag name for model selection
    pub model_flag: String,
    /// Flag name for reasoning effort, if supported
    #[serde(default)]
    pub effort_flag: Option<String>,
    /// Flag name for auto/full-auto mode, if supported as a CLI flag
    #[serde(default)]
    pub auto_flag: Option<String>,
    /// Flag name for verbose/debug output, if supported
    #[serde(default)]
    pub verbose_flag: Option<String>,
    /// Flag name for output format selection, if supported
    #[serde(default)]
    pub output_format_flag: Option<String>,
    /// Flag name for system prompt injection, if supported
    #[serde(default)]
    pub system_prompt_flag: Option<String>,
    /// Flag name for allowed tools filter, if supported
    #[serde(default)]
    pub allowed_tools_flag: Option<String>,
    /// Strategy for sandbox mode
    #[serde(default = "default_sandbox_unsupported")]
    pub sandbox: SandboxStrategy,
    /// Flag name for session naming, if supported
    #[serde(default)]
    pub name_flag: Option<String>,
    /// Flag name for adding extra directories, if supported
    #[serde(default)]
    pub add_dir_flag: Option<String>,
    /// Flag name for approval/permission mode, if supported
    #[serde(default)]
    pub approval_mode_flag: Option<String>,
}

fn default_sandbox_unsupported() -> SandboxStrategy {
    SandboxStrategy::Unsupported
}

impl AgentPolyfillConfig {
    /// Get the yolo flag for this agent
    pub fn get_yolo_flag(&self) -> Option<String> {
        self.yolo_flag.clone()
    }

    /// Get the model flag for this agent
    pub fn get_model_flag(&self) -> String {
        self.model_flag.clone()
    }

    /// Get the effort flag for this agent, if supported
    pub fn get_effort_flag(&self) -> Option<String> {
        self.effort_flag.clone()
    }

    /// Get args for continuing the latest session
    pub fn get_continue_args(&self) -> Vec<String> {
        self.session
            .continue_arg
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    }

    /// Get args for resuming a specific session
    pub fn get_resume_args(&self, session_id: Option<&str>) -> Vec<String> {
        let mut args: Vec<String> = self
            .session
            .resume_arg
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        if let Some(id) = session_id {
            args.push(id.to_string());
        }
        args
    }

    /// Get headless strategy and associated args/subcommand
    pub fn get_headless_invocation(&self, prompt: &str) -> (Vec<String>, Vec<String>) {
        match &self.headless {
            HeadlessStrategy::Flag(f) => (vec![f.clone(), prompt.to_string()], vec![]),
            HeadlessStrategy::Subcommand(s) => (vec![prompt.to_string()], vec![s.clone()]),
        }
    }

    /// Get fork strategy and associated args/subcommand
    pub fn get_fork_invocation(&self) -> (Vec<String>, Vec<String>, bool) {
        match &self.fork {
            ForkStrategy::Flag(f) => (vec![f.clone()], vec![], true),
            ForkStrategy::Subcommand(s) => (vec![], vec![s.clone()], true),
            ForkStrategy::Unsupported => (vec![], vec![], false),
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
    /// Polyfill configuration
    pub polyfill: AgentPolyfillConfig,
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
    /// Create an agent definition from an agent type
    pub fn from_type(agent_type: AgentType) -> Self {
        match agent_type {
            AgentType::Claude => Self::claude(),
            AgentType::Codex => Self::codex(),
            AgentType::Gemini => Self::gemini(),
            AgentType::OpenCode => Self::opencode(),
        }
    }

    /// Create Claude Code agent definition
    pub fn claude() -> Self {
        Self {
            agent_type: AgentType::Claude,
            name: "Claude Code".to_string(),
            binary: "claude".to_string(),
            description: "Anthropic's Claude Code CLI".to_string(),
            polyfill: AgentPolyfillConfig {
                headless: HeadlessStrategy::Flag("-p".to_string()),
                session: SessionStrategy {
                    continue_arg: "--continue".to_string(),
                    resume_arg: "--resume".to_string(),
                },
                fork: ForkStrategy::Flag("--fork-session".to_string()),
                yolo_flag: Some("--dangerously-skip-permissions".to_string()),
                model_flag: "--model".to_string(),
                effort_flag: Some("--effort".to_string()),
                auto_flag: None,
                verbose_flag: Some("--verbose".to_string()),
                output_format_flag: Some("--output-format".to_string()),
                system_prompt_flag: Some("--system-prompt".to_string()),
                allowed_tools_flag: Some("--allowedTools".to_string()),
                sandbox: SandboxStrategy::Unsupported,
                name_flag: Some("--name".to_string()),
                add_dir_flag: Some("--add-dir".to_string()),
                approval_mode_flag: Some("--permission-mode".to_string()),
            },
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
            polyfill: AgentPolyfillConfig {
                headless: HeadlessStrategy::Subcommand("exec".to_string()),
                session: SessionStrategy {
                    continue_arg: "resume --last".to_string(),
                    resume_arg: "resume".to_string(),
                },
                fork: ForkStrategy::Subcommand("fork".to_string()),
                yolo_flag: Some("--dangerously-bypass-approvals-and-sandbox".to_string()),
                model_flag: "-m".to_string(),
                effort_flag: None,
                auto_flag: Some("--full-auto".to_string()),
                verbose_flag: None,
                output_format_flag: None,
                system_prompt_flag: None,
                allowed_tools_flag: None,
                sandbox: SandboxStrategy::ValueFlag("--sandbox".to_string(), "workspace-write".to_string()),
                name_flag: None,
                add_dir_flag: Some("--add-dir".to_string()),
                approval_mode_flag: Some("-a".to_string()),
            },
            github_repo: Some("openai/codex".to_string()),
            npm_package: None,
            enabled: true,
        }
    }

    /// Create Gemini CLI agent definition
    pub fn gemini() -> Self {
        Self {
            agent_type: AgentType::Gemini,
            name: "Gemini CLI".to_string(),
            binary: "gemini".to_string(),
            description: "Google's Gemini CLI".to_string(),
            polyfill: AgentPolyfillConfig {
                headless: HeadlessStrategy::Flag("-p".to_string()),
                session: SessionStrategy {
                    continue_arg: "--resume latest".to_string(),
                    resume_arg: "--resume".to_string(),
                },
                fork: ForkStrategy::Unsupported,
                yolo_flag: Some("--yolo".to_string()),
                model_flag: "-m".to_string(),
                effort_flag: None,
                auto_flag: None,
                verbose_flag: Some("--debug".to_string()),
                output_format_flag: Some("-o".to_string()),
                system_prompt_flag: None,
                allowed_tools_flag: Some("--allowed-tools".to_string()),
                sandbox: SandboxStrategy::BoolFlag("--sandbox".to_string()),
                name_flag: None,
                add_dir_flag: Some("--include-directories".to_string()),
                approval_mode_flag: Some("--approval-mode".to_string()),
            },
            github_repo: Some("google-gemini/gemini-cli".to_string()),
            npm_package: Some("@google/gemini-cli".to_string()),
            enabled: true,
        }
    }

    /// Create OpenCode agent definition
    pub fn opencode() -> Self {
        Self {
            agent_type: AgentType::OpenCode,
            name: "OpenCode".to_string(),
            binary: "opencode".to_string(),
            description: "AI coding agent for the terminal".to_string(),
            polyfill: AgentPolyfillConfig {
                headless: HeadlessStrategy::Subcommand("run".to_string()),
                session: SessionStrategy {
                    continue_arg: "--continue".to_string(),
                    resume_arg: "--session".to_string(),
                },
                fork: ForkStrategy::Flag("--fork".to_string()),
                yolo_flag: None,
                model_flag: "-m".to_string(),
                effort_flag: None,
                auto_flag: None,
                verbose_flag: Some("--print-logs".to_string()),
                output_format_flag: None,
                system_prompt_flag: None,
                allowed_tools_flag: None,
                sandbox: SandboxStrategy::Unsupported,
                name_flag: None,
                add_dir_flag: None,
                approval_mode_flag: None,
            },
            github_repo: Some("anomalyco/opencode".to_string()),
            npm_package: Some("opencode-ai".to_string()),
            enabled: true,
        }
    }
}

/// Version information for an agent
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
            .join("unleash");

        fs::create_dir_all(&config_dir)?;

        let mut manager = Self {
            agents: HashMap::new(),
            versions: HashMap::new(),
            config_dir,
        };

        // Register default agents
        manager.register_agent(AgentDefinition::claude());
        manager.register_agent(AgentDefinition::codex());
        manager.register_agent(AgentDefinition::gemini());
        manager.register_agent(AgentDefinition::opencode());

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
                let mut version = Self::parse_version(&version_str);

                // Codex reports "0.0.0" from source builds — fall back to git tag
                if agent_type == AgentType::Codex && version.as_deref() == Some("0.0.0") {
                    version = Self::codex_version_from_git_tag();
                }

                // Update cache
                let entry = self.versions.entry(agent_type).or_default();
                entry.installed = version.clone();
                entry.binary_path = which::which(&agent.binary).ok();

                Ok(version)
            }
            _ => Ok(None),
        }
    }

    /// Get codex version from git tag in the cached source repo.
    /// Codex uses workspace version "0.0.0" so --version is useless;
    /// the real version comes from git tags like "rust-v0.116.0".
    fn codex_version_from_git_tag() -> Option<String> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("unleash/codex-source");

        if !cache_dir.join(".git").exists() {
            return None;
        }

        let output = Command::new("git")
            .args(["describe", "--tags", "--abbrev=0"])
            .current_dir(&cache_dir)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Tags are like "rust-v0.116.0" — strip "rust-v" prefix
        Some(
            tag.trim_start_matches("rust-v")
                .trim_start_matches('v')
                .to_string(),
        )
    }

    /// Get a GitHub token for API auth (needed for private repos).
    fn github_token() -> Option<String> {
        if let Ok(token) = std::env::var("GH_TOKEN") {
            return Some(token);
        }
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            return Some(token);
        }
        Command::new("gh")
            .args(["auth", "token"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
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

        let mut cmd = Command::new("curl");
        cmd.args(["-s", "-H", "Accept: application/vnd.github.v3+json"]);
        // Add auth for private repos
        if let Some(token) = Self::github_token() {
            cmd.arg("-H").arg(format!("Authorization: token {}", token));
        }
        let output = cmd.arg(&url).output()?;

        if !output.status.success() {
            return Ok(None);
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let tag = json.get("tag_name").and_then(|t| t.as_str()).map(|s| {
            // Handle tags like "rust-v0.116.0" (Codex) and "v1.2.3" (others)
            s.trim_start_matches("rust-v")
                .trim_start_matches('v')
                .to_string()
        });

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
            (Some(i), Some(l)) => Ok(crate::version::version_less_than(&i, &l)),
            _ => Ok(false),
        }
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
            AgentType::Gemini => self.update_npm_agent("@google/gemini-cli", "Gemini CLI"),
            AgentType::OpenCode => self.update_opencode(),
        }
    }

    /// Update Claude Code via npm
    fn update_claude(&self) -> io::Result<String> {
        let output = crate::version::VersionManager::npm_global_command()
            .args(["install", "-g", "@anthropic-ai/claude-code@latest"])
            .output()?;

        if output.status.success() {
            Ok("Claude Code updated successfully".to_string())
        } else {
            Err(io::Error::other(format!(
                "Failed to update Claude Code: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    /// Update Codex — prefer prebuilt binary, fall back to source build
    fn update_codex(&self) -> io::Result<String> {
        let install_path = dirs::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?
            .join(".local/bin/codex");
        fs::create_dir_all(install_path.parent().unwrap())?;

        // Try prebuilt binary first
        match Self::install_codex_binary(&install_path) {
            Ok(msg) => return Ok(msg),
            Err(e) => {
                eprintln!(
                    "Prebuilt binary install failed ({}), falling back to source build...",
                    e
                );
            }
        }

        // Fallback: build from source (requires cargo)
        if which::which("cargo").is_err() {
            return Err(io::Error::other(
                "No prebuilt Codex binary for this platform and cargo is not installed. \
                 Install Rust (rustup.rs) or download Codex manually from https://github.com/openai/codex/releases"
            ));
        }

        Self::build_codex_from_source(&install_path)
    }

    /// Download and install prebuilt Codex binary from GitHub releases
    fn install_codex_binary(install_path: &std::path::Path) -> io::Result<String> {
        // Detect platform triple
        let triple = Self::detect_platform_triple()
            .ok_or_else(|| io::Error::other("Unsupported platform for prebuilt binary"))?;

        let asset_name = format!("codex-{}.tar.gz", triple);

        // Get latest release tag
        let tag_output = Command::new("curl")
            .args([
                "-s",
                "-H",
                "Accept: application/vnd.github.v3+json",
                "https://api.github.com/repos/openai/codex/releases/latest",
            ])
            .output()?;

        let json: serde_json::Value = serde_json::from_slice(&tag_output.stdout)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let tag = json
            .get("tag_name")
            .and_then(|t| t.as_str())
            .ok_or_else(|| io::Error::other("Could not determine latest Codex release tag"))?;

        let version = tag.trim_start_matches("rust-v").trim_start_matches('v');

        // Check if asset exists in this release
        let has_asset = json
            .get("assets")
            .and_then(|a| a.as_array())
            .map(|assets| {
                assets
                    .iter()
                    .any(|a| a.get("name").and_then(|n| n.as_str()) == Some(&asset_name))
            })
            .unwrap_or(false);

        if !has_asset {
            return Err(io::Error::other(format!(
                "No prebuilt binary '{}' found in release {}",
                asset_name, tag
            )));
        }

        let download_url = format!(
            "https://github.com/openai/codex/releases/download/{}/{}",
            tag, asset_name
        );

        // Download to temp file
        let tmp_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("unleash/codex-download");
        fs::create_dir_all(&tmp_dir)?;
        let tmp_archive = tmp_dir.join(&asset_name);

        let dl_output = Command::new("curl")
            .args(["-fsSL", "-o", &tmp_archive.to_string_lossy(), &download_url])
            .output()?;

        if !dl_output.status.success() {
            return Err(io::Error::other(format!(
                "Download failed: {}",
                String::from_utf8_lossy(&dl_output.stderr)
            )));
        }

        // Extract — codex binary is at the root of the tar.gz
        let extract_output = Command::new("tar")
            .args([
                "xzf",
                &tmp_archive.to_string_lossy(),
                "-C",
                &tmp_dir.to_string_lossy(),
            ])
            .output()?;

        if !extract_output.status.success() {
            return Err(io::Error::other(format!(
                "Extraction failed: {}",
                String::from_utf8_lossy(&extract_output.stderr)
            )));
        }

        // Find the codex binary — named codex-<triple> inside the archive
        let extracted_binary = tmp_dir.join(format!("codex-{}", triple));
        let extracted_fallback = tmp_dir.join("codex");
        let binary_path = if extracted_binary.exists() {
            &extracted_binary
        } else if extracted_fallback.exists() {
            &extracted_fallback
        } else {
            return Err(io::Error::other(format!(
                "Extracted archive does not contain 'codex-{}' or 'codex' binary",
                triple
            )));
        };

        // Install
        fs::copy(binary_path, install_path)?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(install_path, fs::Permissions::from_mode(0o755))?;
        }

        // Cleanup
        let _ = fs::remove_dir_all(&tmp_dir);

        Ok(format!("Codex {} installed from prebuilt binary", version))
    }

    /// Detect the platform triple for prebuilt binary downloads
    fn detect_platform_triple() -> Option<&'static str> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return Some("x86_64-unknown-linux-gnu");
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            return Some("aarch64-unknown-linux-gnu");
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            return Some("aarch64-apple-darwin");
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            return Some("x86_64-apple-darwin");
        }
        #[allow(unreachable_code)]
        None
    }

    /// Build Codex from source (fallback when no prebuilt binary available)
    fn build_codex_from_source(install_path: &std::path::Path) -> io::Result<String> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("unleash/codex-source");

        let mut progress = Vec::new();

        // Clone or update the repo in cache
        if cache_dir.join(".git").exists() {
            progress.push(format!("Updating codex source at {}", cache_dir.display()));
            let output = Command::new("git")
                .args(["pull", "--ff-only"])
                .current_dir(&cache_dir)
                .output()?;

            if !output.status.success() {
                fs::remove_dir_all(&cache_dir)?;
                progress.push("Pull failed, re-cloning...".to_string());
            }
        }

        if !cache_dir.join(".git").exists() {
            progress.push("Cloning openai/codex from GitHub...".to_string());
            fs::create_dir_all(cache_dir.parent().unwrap())?;
            let output = Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "https://github.com/openai/codex.git",
                    &cache_dir.to_string_lossy(),
                ])
                .output()?;

            if !output.status.success() {
                return Err(io::Error::other(format!(
                    "Failed to clone codex: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }

            // Fetch tags so `git describe --tags` works on shallow clones
            let _ = Command::new("git")
                .args(["fetch", "--tags", "--depth=1"])
                .current_dir(&cache_dir)
                .output();
        }

        let codex_rs_dir = cache_dir.join("codex-rs");
        if !codex_rs_dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Codex codex-rs directory not found in cloned repo",
            ));
        }

        progress.push("Building codex from source (this may take a while)...".to_string());
        let output = Command::new("cargo")
            .args(["build", "--release", "-p", "codex-cli"])
            .current_dir(&codex_rs_dir)
            .output()?;

        if output.status.success() {
            let binary_path = codex_rs_dir.join("target/release/codex");
            fs::copy(&binary_path, install_path)?;

            progress.push(format!(
                "Codex built and installed to {}",
                install_path.display()
            ));
            Ok(progress.join("\n"))
        } else {
            Err(io::Error::other(format!(
                "Failed to build Codex: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    /// Update OpenCode using its built-in upgrade command
    fn update_opencode(&self) -> io::Result<String> {
        if which::which("opencode").is_ok() {
            let output = Command::new("opencode")
                .args(["upgrade", "latest"])
                .output()?;

            if output.status.success() {
                Ok("OpenCode updated successfully".to_string())
            } else {
                Err(io::Error::other(format!(
                    "Failed to update OpenCode: {}",
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        } else {
            self.update_npm_agent("opencode-ai", "OpenCode")
        }
    }

    /// Update an npm-based agent to latest version
    fn update_npm_agent(&self, package: &str, name: &str) -> io::Result<String> {
        let output = crate::version::VersionManager::npm_global_command()
            .args(["install", "-g", &format!("{}@latest", package)])
            .output()?;

        if output.status.success() {
            Ok(format!("{} updated successfully", name))
        } else {
            Err(io::Error::other(format!(
                "Failed to update {}: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            )))
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
            let latest = self
                .versions
                .get(&agent_type)
                .and_then(|v| v.latest.clone());
            let update_available = match (&installed, &latest) {
                (Some(i), Some(l)) => crate::version::version_less_than(i, l),
                _ => false,
            };
            results.push((agent_type, installed, latest, update_available));
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_npm_package_is_google() {
        let gemini = AgentDefinition::gemini();
        assert_eq!(
            gemini.npm_package.as_deref(),
            Some("@google/gemini-cli"),
            "Gemini npm_package must reference @google, not @anthropic-ai"
        );
    }

    #[test]
    fn no_non_anthropic_agent_uses_anthropic_npm_scope() {
        for agent_type in AgentType::all() {
            let def = AgentDefinition::from_type(*agent_type);
            if *agent_type != AgentType::Claude {
                if let Some(ref pkg) = def.npm_package {
                    assert!(
                        !pkg.starts_with("@anthropic-ai/"),
                        "Non-Anthropic agent {:?} incorrectly uses @anthropic-ai scope: {}",
                        agent_type,
                        pkg
                    );
                }
            }
        }
    }

    #[test]
    fn claude_npm_package_is_anthropic() {
        let claude = AgentDefinition::claude();
        assert_eq!(
            claude.npm_package.as_deref(),
            Some("@anthropic-ai/claude-code")
        );
    }

    // Version comparison tests moved to src/version.rs (canonical implementation)

    #[test]
    fn parse_version_various_formats() {
        assert_eq!(
            AgentManager::parse_version("claude 2.1.22"),
            Some("2.1.22".to_string())
        );
        assert_eq!(
            AgentManager::parse_version("codex 0.1.0"),
            Some("0.1.0".to_string())
        );
        assert_eq!(
            AgentManager::parse_version("v1.2.3"),
            Some("1.2.3".to_string())
        );
    }
}
