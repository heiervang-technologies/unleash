//! Multi-agent management for unleash
//!
//! Manages different code agents (Claude Code, Codex, etc.) including:
//! - Agent definitions and configuration
//! - Version tracking and updates
//! - Installation management

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

/// Supported agent types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    /// The unleash wrapper itself (version management entry at top of the picker)
    Unleash,
    Claude,
    Codex,
    Antigravity,
    Gemini,
    OpenCode,
    Pi,
    Hermes,
    Custom(String),
}

impl AgentType {
    /// Built-in agent types in stable order (used for TUI cycling)
    pub fn builtin() -> &'static [AgentType] {
        &[
            AgentType::Claude,
            AgentType::Codex,
            AgentType::Antigravity,
            AgentType::OpenCode,
            AgentType::Pi,
            AgentType::Hermes,
            AgentType::Gemini,
        ]
    }

    /// All agent types: built-ins + custom agents from definitions
    pub fn all_with_custom(custom: &[AgentDefinition]) -> Vec<AgentType> {
        let mut types: Vec<AgentType> = Self::builtin().to_vec();
        for def in custom {
            if let AgentType::Custom(_) = &def.agent_type {
                types.push(def.agent_type.clone());
            }
        }
        types
    }

    /// All types for the version manager picker: Unleash first, then agents + custom.
    pub fn all_for_version_picker(custom: &[AgentDefinition]) -> Vec<AgentType> {
        let mut types = vec![AgentType::Unleash];
        types.extend(Self::all_with_custom(custom));
        types
    }

    pub fn display_name(&self) -> Cow<'static, str> {
        match self {
            AgentType::Unleash => Cow::Borrowed("Unleash"),
            AgentType::Claude => Cow::Borrowed("Claude Code"),
            AgentType::Codex => Cow::Borrowed("Codex"),
            AgentType::Antigravity => Cow::Borrowed("Antigravity CLI"),
            AgentType::Gemini => Cow::Borrowed("Gemini CLI"),
            AgentType::OpenCode => Cow::Borrowed("OpenCode"),
            AgentType::Pi => Cow::Borrowed("Pi"),
            AgentType::Hermes => Cow::Borrowed("Hermes Agent"),
            AgentType::Custom(name) => Cow::Owned(name.clone()),
        }
    }

    // Public API since 0.1.x; signature returns Option, not Result as
    // std::str::FromStr requires. Renaming would break callers.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" | "claude-code" => Some(AgentType::Claude),
            "codex" => Some(AgentType::Codex),
            "antigravity" | "antigravity-cli" | "agy" => Some(AgentType::Antigravity),
            "gemini" | "gemini-cli" => Some(AgentType::Gemini),
            "opencode" | "open-code" => Some(AgentType::OpenCode),
            "pi" | "pi-coding-agent" => Some(AgentType::Pi),
            "hermes" | "hermes-agent" => Some(AgentType::Hermes),
            _ => None,
        }
    }

    /// Cleanly map each agent type to its mascot file key name
    pub fn mascot_name(&self) -> &'static str {
        match self {
            AgentType::Unleash => "unleash",
            AgentType::Claude => "claude",
            AgentType::Codex => "codex",
            AgentType::Antigravity => "antigravity",
            AgentType::Gemini => "gemini",
            AgentType::OpenCode => "opencode",
            AgentType::Pi => "pi",
            AgentType::Hermes => "hermes",
            AgentType::Custom(_) => "claude",
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

/// Strategy for resuming a session
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResumeStrategy {
    /// Use a flag (e.g., --resume)
    Flag(String),
    /// Use a subcommand (e.g., resume)
    Subcommand(String),
}

impl ResumeStrategy {
    pub fn get_args(&self, session_id: Option<&str>) -> Vec<String> {
        let mut args: Vec<String> = match self {
            ResumeStrategy::Flag(s) | ResumeStrategy::Subcommand(s) => {
                s.split_whitespace().map(|x| x.to_string()).collect()
            }
        };
        if let Some(id) = session_id {
            args.push(id.to_string());
        }
        args
    }
}

/// Session management strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStrategy {
    /// Strategy for continuing last session
    pub continue_strategy: ResumeStrategy,
    /// Strategy for resuming specific session
    pub resume_strategy: ResumeStrategy,
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
    /// Flag name for git worktree mode, if supported
    #[serde(default)]
    pub worktree_flag: Option<String>,
    /// Flag name for "run an initial prompt then continue interactively",
    /// if the agent has a dedicated flag for that mode (e.g. agy's `-i` /
    /// `--prompt-interactive`). Used by the crossload auto-fallback path
    /// to drop the user into an interactive session pre-loaded with the
    /// rendered transcript, instead of using the one-shot `headless` flag
    /// which would print one response and exit.
    #[serde(default)]
    pub interactive_prompt_flag: Option<String>,
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
        self.session.continue_strategy.get_args(None)
    }

    /// Get args for resuming a specific session
    pub fn get_resume_args(&self, session_id: Option<&str>) -> Vec<String> {
        self.session.resume_strategy.get_args(session_id)
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
    /// Create an agent definition from a user-defined custom agent config.
    pub fn from_custom_config(config: &crate::config::CustomAgentConfig) -> Self {
        Self {
            agent_type: AgentType::Custom(config.name.clone()),
            name: config.name.clone(),
            binary: config.binary.clone(),
            description: config.description.clone(),
            polyfill: config.polyfill.clone(),
            github_repo: config.github_repo.clone(),
            npm_package: config.npm_package.clone(),
            enabled: config.enabled,
        }
    }

    /// Create an agent definition from an agent type.
    /// Panics for `Custom` and `Unleash` — use `from_custom_config()` for custom agents.
    pub fn from_type(agent_type: AgentType) -> Self {
        match agent_type {
            AgentType::Unleash => panic!(
                "AgentDefinition::from_type() called with Unleash. \
                 Unleash is not a launchable agent."
            ),
            AgentType::Claude => Self::claude(),
            AgentType::Codex => Self::codex(),
            AgentType::Antigravity => Self::antigravity(),
            AgentType::Gemini => Self::gemini(),
            AgentType::OpenCode => Self::opencode(),
            AgentType::Pi => Self::pi(),
            AgentType::Hermes => Self::hermes(),
            AgentType::Custom(ref name) => panic!(
                "AgentDefinition::from_type() called with Custom(\"{}\"). Use from_custom_config() instead.",
                name
            ),
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
                    continue_strategy: ResumeStrategy::Flag("--continue".to_string()),
                    resume_strategy: ResumeStrategy::Flag("--resume".to_string()),
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
                worktree_flag: Some("--worktree".to_string()),
                interactive_prompt_flag: None,
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
                    continue_strategy: ResumeStrategy::Subcommand("resume --last".to_string()),
                    resume_strategy: ResumeStrategy::Subcommand("resume".to_string()),
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
                sandbox: SandboxStrategy::ValueFlag(
                    "--sandbox".to_string(),
                    "workspace-write".to_string(),
                ),
                name_flag: None,
                add_dir_flag: Some("--add-dir".to_string()),
                approval_mode_flag: Some("-a".to_string()),
                worktree_flag: None,
                interactive_prompt_flag: None,
            },
            github_repo: Some("openai/codex".to_string()),
            npm_package: None,
            enabled: true,
        }
    }

    /// Create Antigravity CLI agent definition
    pub fn antigravity() -> Self {
        Self {
            agent_type: AgentType::Antigravity,
            name: "Antigravity CLI".to_string(),
            binary: "agy".to_string(),
            description: "Google's Antigravity CLI".to_string(),
            polyfill: AgentPolyfillConfig {
                headless: HeadlessStrategy::Flag("-p".to_string()),
                // agy uses `--continue` for "continue last conversation" and
                // `--conversation <id>` for "resume by ID" — verified at
                // `agy --help`. Previously the polyfill mapped both to
                // `--resume [latest|<id>]` which agy doesn't accept,
                // breaking `unleash agy -c` and `unleash agy -x <session>`
                // with `flags provided but not defined: -resume`.
                session: SessionStrategy {
                    continue_strategy: ResumeStrategy::Flag("--continue".to_string()),
                    resume_strategy: ResumeStrategy::Flag("--conversation".to_string()),
                },
                fork: ForkStrategy::Unsupported,
                yolo_flag: Some("--dangerously-skip-permissions".to_string()),
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
                approval_mode_flag: None,
                worktree_flag: Some("--worktree".to_string()),
                // agy supports `-i` / `--prompt-interactive`: load the prompt
                // as the first message and then drop into an interactive
                // session. The crossload auto-fallback uses this so the user
                // can keep typing after the prior context loads, instead of
                // getting a single response and exiting via `-p` / `--print`.
                interactive_prompt_flag: Some("-i".to_string()),
            },
            github_repo: None,
            // No npm package exists for antigravity — `@google/antigravity-cli`
            // is not published. Real install path is the AUR helper (see
            // VersionManager::install_antigravity_version_streaming and PR #259).
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
                    continue_strategy: ResumeStrategy::Flag("--resume latest".to_string()),
                    resume_strategy: ResumeStrategy::Flag("--resume".to_string()),
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
                worktree_flag: Some("--worktree".to_string()),
                interactive_prompt_flag: None,
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
                    continue_strategy: ResumeStrategy::Flag("--continue".to_string()),
                    resume_strategy: ResumeStrategy::Flag("-s".to_string()),
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
                worktree_flag: None,
                interactive_prompt_flag: None,
            },
            github_repo: Some("anomalyco/opencode".to_string()),
            npm_package: Some("opencode-ai".to_string()),
            enabled: true,
        }
    }

    /// Create Pi agent definition
    pub fn pi() -> Self {
        Self {
            agent_type: AgentType::Pi,
            name: "Pi".to_string(),
            binary: "pi".to_string(),
            description: "Coding agent CLI with read, bash, edit, write tools".to_string(),
            polyfill: AgentPolyfillConfig {
                headless: HeadlessStrategy::Flag("-p".to_string()),
                session: SessionStrategy {
                    continue_strategy: ResumeStrategy::Flag("--continue".to_string()),
                    resume_strategy: ResumeStrategy::Flag("--session".to_string()),
                },
                fork: ForkStrategy::Flag("--fork".to_string()),
                yolo_flag: None,
                model_flag: "--model".to_string(),
                effort_flag: Some("--thinking".to_string()),
                auto_flag: None,
                verbose_flag: None,
                output_format_flag: Some("--mode".to_string()),
                system_prompt_flag: Some("--system-prompt".to_string()),
                allowed_tools_flag: Some("--tools".to_string()),
                sandbox: SandboxStrategy::Unsupported,
                name_flag: None,
                add_dir_flag: None,
                approval_mode_flag: None,
                worktree_flag: None,
                interactive_prompt_flag: None,
            },
            github_repo: None,
            npm_package: Some("@mariozechner/pi-coding-agent".to_string()),
            enabled: true,
        }
    }

    /// Create Hermes Agent definition
    pub fn hermes() -> Self {
        Self {
            agent_type: AgentType::Hermes,
            name: "Hermes Agent".to_string(),
            binary: "hermes".to_string(),
            description: "NousResearch's autonomous AI agent with persistent memory".to_string(),
            polyfill: AgentPolyfillConfig {
                headless: HeadlessStrategy::Flag("-z".to_string()),
                session: SessionStrategy {
                    continue_strategy: ResumeStrategy::Flag("--continue".to_string()),
                    resume_strategy: ResumeStrategy::Flag("--resume".to_string()),
                },
                fork: ForkStrategy::Flag("--worktree".to_string()),
                yolo_flag: Some("--yolo".to_string()),
                model_flag: "-m".to_string(),
                effort_flag: None,
                auto_flag: None,
                verbose_flag: Some("--verbose".to_string()),
                output_format_flag: None,
                system_prompt_flag: None,
                allowed_tools_flag: None,
                sandbox: SandboxStrategy::Unsupported,
                name_flag: None,
                add_dir_flag: None,
                approval_mode_flag: None,
                worktree_flag: Some("--worktree".to_string()),
                interactive_prompt_flag: None,
            },
            github_repo: Some("NousResearch/hermes-agent".to_string()),
            npm_package: None,
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
        manager.register_agent(AgentDefinition::antigravity());
        manager.register_agent(AgentDefinition::opencode());
        manager.register_agent(AgentDefinition::pi());
        manager.register_agent(AgentDefinition::hermes());

        // Load cached versions
        manager.load_version_cache()?;

        Ok(manager)
    }

    /// Register an agent definition
    pub fn register_agent(&mut self, agent: AgentDefinition) {
        self.agents.insert(agent.agent_type.clone(), agent);
    }

    /// Get an agent definition
    pub fn get_agent(&self, agent_type: AgentType) -> Option<&AgentDefinition> {
        self.agents.get(&agent_type)
    }

    /// List all registered agents
    pub fn list_agents(&self) -> Vec<&AgentDefinition> {
        self.agents.values().collect()
    }

    fn parse_asar_version(content: &[u8]) -> Option<String> {
        let pattern1 = b"\"name\": \"antigravity\"";
        let pattern2 = b"\"name\":\"antigravity\"";
        let pos = content
            .windows(pattern1.len())
            .position(|w| w == pattern1)
            .or_else(|| content.windows(pattern2.len()).position(|w| w == pattern2))?;

        let search_slice = &content[pos..pos + std::cmp::min(1000, content.len() - pos)];
        let version_pattern = b"\"version\"";
        let v_pos = search_slice
            .windows(version_pattern.len())
            .position(|w| w == version_pattern)?;

        let val_slice = &search_slice[v_pos + version_pattern.len()..];

        let mut start_idx = None;
        let mut colon_found = false;
        for (i, &b) in val_slice.iter().enumerate() {
            if b == b':' {
                colon_found = true;
            } else if b == b'"' && colon_found {
                start_idx = Some(i + 1);
                break;
            }
        }

        let start = start_idx?;
        let end_slice = &val_slice[start..];
        let end = end_slice.iter().position(|&b| b == b'"')?;

        String::from_utf8(end_slice[..end].to_vec()).ok()
    }

    /// Get installed version for an agent
    pub fn get_installed_version(&mut self, agent_type: AgentType) -> io::Result<Option<String>> {
        let agent = self
            .agents
            .get(&agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Agent not found"))?;

        if agent_type == AgentType::Antigravity {
            // Check Electron app.asar paths for system/packaged installations
            let paths = [
                PathBuf::from("/opt/Antigravity/resources/app.asar"), // Arch Linux / pacman default
                PathBuf::from("/Applications/Antigravity.app/Contents/Resources/app.asar"), // macOS default
            ];
            let mut version = None;
            for path in &paths {
                if path.exists() {
                    if let Ok(content) = fs::read(path) {
                        if let Some(v) = Self::parse_asar_version(&content) {
                            version = Some(v);
                            break;
                        }
                    }
                }
            }

            // Fallback for binaries installed outside the agent's canonical path
            // (e.g. AUR-installed `agy`, custom `--prefix`, or distro packages).
            if version.is_none() {
                if let Ok(bin_path) = which::which(&agent.binary) {
                    if let Ok(output) = Command::new(&bin_path).arg("--version").output() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        // Clean up output to extract semver if present
                        let ver_str = stdout.trim();
                        if !ver_str.is_empty() {
                            version = Some(ver_str.to_string());
                        }
                    }
                }
            }

            // Update cache
            let entry = self.versions.entry(agent_type).or_default();
            entry.installed = version.clone();
            entry.binary_path = which::which(&agent.binary).ok();

            return Ok(version);
        }

        // Try to get version from binary
        let binary = agent.binary.clone();
        let output = Command::new(&binary).arg("--version").output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout_str = String::from_utf8_lossy(&out.stdout);
                let mut version = Self::parse_version(&stdout_str);

                // Some agents (e.g. pi) write --version to stderr.
                if version.is_none() {
                    let stderr_str = String::from_utf8_lossy(&out.stderr);
                    version = Self::parse_version(&stderr_str);
                }

                // Codex reports "0.0.0" from source builds — fall back to git tag
                if agent_type == AgentType::Codex && version.as_deref() == Some("0.0.0") {
                    version = Self::codex_version_from_git_tag();
                }

                // Hermes reports both a SemVer ("v0.13.0") and a CalVer date
                // ("2026.5.7") on the same line. Upstream tags releases by
                // CalVer, so the GH "latest" comparison only works against the
                // CalVer — extract it from the parenthesized suffix.
                if agent_type == AgentType::Hermes {
                    let stdout_str = String::from_utf8_lossy(&out.stdout);
                    let stderr_str = String::from_utf8_lossy(&out.stderr);
                    if let Some(v) = Self::parse_hermes_calver(&stdout_str)
                        .or_else(|| Self::parse_hermes_calver(&stderr_str))
                    {
                        version = Some(v);
                    }
                }

                // Update cache
                let entry = self.versions.entry(agent_type).or_default();
                entry.installed = version.clone();
                entry.binary_path = which::which(&binary).ok();

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

    /// Pull the CalVer date out of `hermes --version` output. The format is
    /// "Hermes Agent v<semver> (<calver>)" on the first line. We need the
    /// CalVer to match upstream's GitHub release tags.
    fn parse_hermes_calver(output: &str) -> Option<String> {
        let line = output.lines().next()?;
        let start = line.rfind('(')?;
        let end = line.rfind(')')?;
        if end <= start + 1 {
            return None;
        }
        let inner = line[start + 1..end].trim();
        if inner.chars().next()?.is_ascii_digit() {
            Some(inner.to_string())
        } else {
            None
        }
    }

    /// Get latest version from GitHub
    pub fn get_latest_version(&mut self, agent_type: AgentType) -> io::Result<Option<String>> {
        let agent = self
            .agents
            .get(&agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Agent not found"))?;

        let repo = match &agent.github_repo {
            Some(r) => r.clone(),
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
        let installed = self.get_installed_version(agent_type.clone())?;
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
            AgentType::Unleash => Err(io::Error::other(
                "Use `unleash update` to update unleash itself",
            )),
            AgentType::Claude => self.update_claude(),
            AgentType::Codex => self.update_codex(),
            AgentType::Antigravity => self.update_antigravity(),
            AgentType::Gemini => self.update_npm_agent("@google/gemini-cli", "Gemini CLI"),
            AgentType::OpenCode => self.update_opencode(),
            AgentType::Pi => self.update_npm_agent("@mariozechner/pi-coding-agent", "Pi"),
            AgentType::Hermes => self.update_hermes(),
            AgentType::Custom(_) => Err(io::Error::other(
                "Version management is not yet supported for custom agents",
            )),
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
        // Codex's Linux releases are statically-linked musl builds; the gnu
        // targets were dropped upstream around rust-v0.118. The musl binaries
        // run fine on glibc systems thanks to static linking.
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return Some("x86_64-unknown-linux-musl");
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            return Some("aarch64-unknown-linux-musl");
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

    /// Update Antigravity CLI via an AUR helper (yay/paru). Antigravity has
    /// no public npm or GitHub-releases channel, so this is the only way to
    /// upgrade it programmatically on Arch-family systems. On every other
    /// OS, returns an honest error pointing at the antigravity.google
    /// download page rather than the old "managed by pacman/yay" lie.
    fn update_antigravity(&self) -> io::Result<String> {
        use std::process::Command;

        let helper = ["yay", "paru"]
            .iter()
            .find(|h| Command::new(*h).arg("--version").output().is_ok());

        let Some(helper) = helper else {
            return Err(io::Error::other(
                "Antigravity CLI has no npm/GitHub release channel. \
                 Install via your distro's AUR helper (yay/paru — package \
                 `antigravity-cli`) or download from https://antigravity.google",
            ));
        };

        let output = Command::new(helper)
            .args(["-S", "--noconfirm", "--needed", "antigravity-cli"])
            .stdin(std::process::Stdio::null())
            .output()?;

        if output.status.success() {
            Ok("Antigravity CLI updated successfully".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(io::Error::other(format!(
                "{} -S antigravity-cli failed: {}",
                helper, stderr
            )))
        }
    }

    /// Update Hermes via the official curl bash installer.
    /// Hermes' installer always installs the latest version — there is no
    /// version pin argument. `--skip-setup` bypasses the interactive setup
    /// wizard, which the installer otherwise drives by reading from /dev/tty
    /// even when piped from curl.
    ///
    /// install.sh's update path does `git pull --ff-only`, which aborts when
    /// the local clone has diverged from origin/main (upstream rebases,
    /// stray local commits). We pre-reset to upstream so the ff-only pull
    /// always succeeds — see `VersionManager::reset_diverged_hermes_clone`
    /// for the rationale and `install_hermes_version_streaming` for the
    /// TUI-side caller.
    fn update_hermes(&self) -> io::Result<String> {
        if let Some(dir) = crate::version::VersionManager::hermes_install_dir() {
            let branch = std::env::var("HERMES_BRANCH").unwrap_or_else(|_| "main".to_string());
            crate::version::VersionManager::reset_diverged_hermes_clone(
                &dir,
                &branch,
                &mut |msg| eprintln!("{}", msg),
            );
        }

        let output = Command::new("bash")
            .args([
                "-c",
                "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash -s -- --skip-setup",
            ])
            .stdin(std::process::Stdio::null())
            .output()?;

        if output.status.success() {
            Ok("Hermes Agent updated successfully".to_string())
        } else {
            Err(io::Error::other(format!(
                "Failed to update Hermes Agent: {}",
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
        let agent_types: Vec<AgentType> = self.agents.keys().cloned().collect();
        let mut results = Vec::new();

        for agent_type in agent_types {
            let installed = self
                .get_installed_version(agent_type.clone())
                .ok()
                .flatten();
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
        for agent_type in AgentType::builtin() {
            let def = AgentDefinition::from_type(agent_type.clone());
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
    fn pi_npm_package_is_mariozechner() {
        let pi = AgentDefinition::pi();
        assert_eq!(
            pi.npm_package.as_deref(),
            Some("@mariozechner/pi-coding-agent")
        );
        assert_eq!(pi.binary, "pi");
        assert_eq!(pi.agent_type, AgentType::Pi);
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
    fn hermes_has_no_npm_package() {
        let hermes = AgentDefinition::hermes();
        assert!(hermes.npm_package.is_none());
        assert_eq!(hermes.binary, "hermes");
        assert_eq!(hermes.agent_type, AgentType::Hermes);
        assert_eq!(
            hermes.github_repo.as_deref(),
            Some("NousResearch/hermes-agent")
        );
    }

    #[test]
    fn antigravity_has_no_npm_package() {
        // `@google/antigravity-cli` is not published on npm. Setting it on
        // the definition causes false "npm required" warnings, wasted 404
        // queries in the version-check path, and pointless `npm uninstall`
        // attempts. The real install path is the AUR helper — see
        // VersionManager::install_antigravity_version_streaming.
        let agy = AgentDefinition::antigravity();
        assert!(agy.npm_package.is_none());
        assert_eq!(agy.binary, "agy");
        assert_eq!(agy.agent_type, AgentType::Antigravity);
    }

    #[test]
    fn antigravity_uses_continue_and_conversation_flags() {
        // agy doesn't accept `--resume`. Verified via `agy --help` (which
        // shows `--continue` for "most recent" and `--conversation <id>`
        // for "by ID"). The previous polyfill mapped both to `--resume`,
        // which broke `unleash agy -c` and `unleash agy -x <session>` with
        //   flags provided but not defined: -resume
        // User-reported regression.
        let agy = AgentDefinition::antigravity();
        match &agy.polyfill.session.continue_strategy {
            ResumeStrategy::Flag(s) => assert_eq!(
                s, "--continue",
                "agy continue must use --continue, not --resume"
            ),
            other => panic!("expected continue_strategy::Flag, got {other:?}"),
        }
        match &agy.polyfill.session.resume_strategy {
            ResumeStrategy::Flag(s) => assert_eq!(
                s, "--conversation",
                "agy resume-by-id must use --conversation, not --resume"
            ),
            other => panic!("expected resume_strategy::Flag, got {other:?}"),
        }
    }

    #[test]
    fn antigravity_has_interactive_prompt_flag() {
        // The crossload auto-fallback path (lib.rs) uses
        // `interactive_prompt_flag` to drop the user into an interactive
        // REPL pre-loaded with the rendered transcript, instead of `-p` /
        // `--print` which would emit one response and exit. agy exposes
        // this as `-i` / `--prompt-interactive`. Without this field set,
        // `unleash agy -x <session>` (no `-p`) silently degrades to a
        // one-shot run — which defeats the purpose of crossloading.
        let agy = AgentDefinition::antigravity();
        assert_eq!(
            agy.polyfill.interactive_prompt_flag.as_deref(),
            Some("-i"),
            "agy must expose its `-i` flag for the crossload auto-fallback path"
        );
    }

    #[test]
    fn non_agy_agents_have_no_interactive_prompt_flag() {
        // Currently agy is the only target that hits the crossload
        // auto-fallback (every other CLI has real session injection). Until
        // someone identifies an analogous flag elsewhere, leave them at
        // None so the fallback uses the existing one-shot path.
        for def in [
            AgentDefinition::claude(),
            AgentDefinition::codex(),
            AgentDefinition::gemini(),
            AgentDefinition::opencode(),
            AgentDefinition::pi(),
            AgentDefinition::hermes(),
        ] {
            assert!(
                def.polyfill.interactive_prompt_flag.is_none(),
                "{} should not set interactive_prompt_flag yet",
                def.name
            );
        }
    }

    #[test]
    fn hermes_is_in_builtin_after_pi() {
        let builtins = AgentType::builtin();
        let pi_idx = builtins
            .iter()
            .position(|t| *t == AgentType::Pi)
            .expect("Pi in builtins");
        let hermes_idx = builtins
            .iter()
            .position(|t| *t == AgentType::Hermes)
            .expect("Hermes in builtins");
        assert!(
            hermes_idx > pi_idx,
            "Hermes must come after Pi to preserve existing builtin-index assertions"
        );
    }

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

    #[test]
    fn parse_hermes_calver_extracts_date_from_parens() {
        assert_eq!(
            AgentManager::parse_hermes_calver("Hermes Agent v0.13.0 (2026.5.7)"),
            Some("2026.5.7".to_string())
        );
        assert_eq!(
            AgentManager::parse_hermes_calver(
                "Hermes Agent v0.14.1 (2026.6.12)\nProject: /home/x/.hermes\n"
            ),
            Some("2026.6.12".to_string())
        );
        // Missing parens
        assert_eq!(
            AgentManager::parse_hermes_calver("Hermes Agent v0.13.0"),
            None
        );
        // Non-numeric content in parens
        assert_eq!(
            AgentManager::parse_hermes_calver("Hermes Agent v0.13.0 (dev)"),
            None
        );
    }
}
