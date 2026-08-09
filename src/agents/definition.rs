use serde::{Deserialize, Serialize};

use super::{
    AgentPolyfillConfig, AgentType, ForkStrategy, HeadlessStrategy, ResumeStrategy,
    SandboxStrategy, SessionStrategy,
};

/// npm package providing the `pi` binary.
pub const PI_NPM_PACKAGE: &str = "@earendil-works/pi-coding-agent";

/// Former name of [`PI_NPM_PACKAGE`], deprecated upstream in favour of the
/// `@earendil-works` scope. Kept so the updater can *remove* it rather than
/// leave two packages fighting over the same `pi` bin — both declare it, so a
/// plain install of the new package on top of the old one can leave the stale
/// binary in place while we report the new version.
pub const PI_NPM_PACKAGE_DEPRECATED: &str = "@mariozechner/pi-coding-agent";

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
            AgentType::Clanker => Self::clanker(),
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

    /// Create the Clanker Code definition.
    ///
    /// Clanker is a Codex-compatible fork, but it is independently installed
    /// and updated from the fork's release branch. Keep its launch grammar in
    /// sync with Codex while preserving Clanker's explicit character selector.
    pub fn clanker() -> Self {
        let mut definition = Self::codex();
        definition.agent_type = AgentType::Clanker;
        definition.name = "Clanker Code".to_string();
        definition.binary = "clanker".to_string();
        definition.description = "Heiervang Technologies' character-first Codex fork".to_string();
        definition.polyfill.name_flag = Some("--name".to_string());
        definition.github_repo = Some(crate::clanker::REPOSITORY.to_string());
        definition
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
            npm_package: Some(PI_NPM_PACKAGE.to_string()),
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
