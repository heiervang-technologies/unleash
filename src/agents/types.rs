use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use super::AgentDefinition;

/// Supported agent types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    /// The unleash wrapper itself (version management entry at top of the picker)
    Unleash,
    Claude,
    Codex,
    Clanker,
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
            AgentType::Clanker,
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
            if let AgentType::Custom(name) = &def.agent_type {
                if Self::from_str(name).is_none() {
                    types.push(def.agent_type.clone());
                }
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
            AgentType::Clanker => Cow::Borrowed("Clanker Code"),
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
            "clanker" | "clanker-code" => Some(AgentType::Clanker),
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
            AgentType::Clanker => "clanker",
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
