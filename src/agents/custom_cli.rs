use std::io;

use super::{
    AgentPolyfillConfig, AgentType, ForkStrategy, HeadlessStrategy, ResumeStrategy,
    SandboxStrategy, SessionStrategy,
};

/// Fields collected from the `unleash agents add` CLI subcommand.
pub struct AddCustomAgentArgs {
    pub name: String,
    pub binary: String,
    pub headless_flag: Option<String>,
    pub headless_subcommand: Option<String>,
    pub description: Option<String>,
    pub continue_flag: Option<String>,
    pub resume_flag: Option<String>,
    pub model_flag: Option<String>,
    pub yolo_flag: Option<String>,
    pub github_repo: Option<String>,
    pub npm_package: Option<String>,
    pub dry_run: bool,
    pub force: bool,
}

/// Build a `CustomAgentConfig` from CLI args, mirroring the TUI wizard's
/// `CustomAgentDraft::into_config` defaults so both code paths produce
/// equivalent TOML for equivalent input.
pub fn build_custom_agent_config(
    args: &AddCustomAgentArgs,
) -> Result<crate::config::CustomAgentConfig, String> {
    if args.name.trim().is_empty() {
        return Err("Custom agent name is required".into());
    }
    if args.binary.trim().is_empty() {
        return Err("Custom agent binary is required".into());
    }
    if AgentType::from_str(args.name.trim()).is_some() {
        return Err(format!(
            "'{}' clashes with a built-in agent name",
            args.name.trim()
        ));
    }

    let headless = match (
        args.headless_flag
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        args.headless_subcommand
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    ) {
        (Some(f), None) => HeadlessStrategy::Flag(f.to_string()),
        (None, Some(s)) => HeadlessStrategy::Subcommand(s.to_string()),
        (Some(_), Some(_)) => unreachable!("clap conflicts_with prevents this"),
        (None, None) => {
            return Err("Either --headless-flag or --headless-subcommand is required".into())
        }
    };

    Ok(crate::config::CustomAgentConfig {
        name: args.name.trim().to_string(),
        binary: args.binary.trim().to_string(),
        description: args
            .description
            .clone()
            .unwrap_or_else(|| format!("Custom agent: {}", args.name.trim())),
        polyfill: AgentPolyfillConfig {
            headless,
            session: SessionStrategy {
                continue_strategy: ResumeStrategy::Flag(
                    args.continue_flag
                        .clone()
                        .unwrap_or_else(|| "--continue".into()),
                ),
                resume_strategy: ResumeStrategy::Flag(
                    args.resume_flag
                        .clone()
                        .unwrap_or_else(|| "--resume".into()),
                ),
            },
            fork: ForkStrategy::Unsupported,
            yolo_flag: args.yolo_flag.clone(),
            model_flag: args.model_flag.clone().unwrap_or_else(|| "--model".into()),
            effort_flag: None,
            auto_flag: None,
            verbose_flag: None,
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
        github_repo: args.github_repo.clone(),
        npm_package: args.npm_package.clone(),
        asset_template: None,
        enabled: true,
    })
}

/// Handler for `unleash agents add`. Builds the config, validates, then either
/// prints the rendered TOML (`--dry-run`) or commits both the app-config entry
/// and a matching profile file. Re-adds with the same name overwrite in place
/// (warns unless `--force` is set).
pub fn add_custom_agent_cli(args: AddCustomAgentArgs) -> io::Result<()> {
    let mgr = crate::config::ProfileManager::new()?;
    add_custom_agent_with(&mgr, args)
}

/// Testable inner of `add_custom_agent_cli` — takes an explicit ProfileManager
/// (typically constructed via `ProfileManager::with_config_dir(tempdir())` in
/// tests) so the disk-touching path is exercisable without env-var fiddling.
pub fn add_custom_agent_with(
    mgr: &crate::config::ProfileManager,
    args: AddCustomAgentArgs,
) -> io::Result<()> {
    let fresh_agent = build_custom_agent_config(&args)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let mut app_config = mgr.load_app_config()?;
    let existing_idx = app_config
        .custom_agents
        .iter()
        .position(|c| c.name == fresh_agent.name);
    let merged_with_existing = existing_idx.is_some();
    let agent = if let Some(idx) = existing_idx {
        merge_args_into_existing(&app_config.custom_agents[idx], &args, &fresh_agent)
    } else {
        fresh_agent
    };

    if args.dry_run {
        let rendered = toml::to_string_pretty(&agent)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        println!("# Would write to ~/.config/unleash/config.toml under [[custom_agents]]");
        if merged_with_existing {
            println!("# (merging with existing entry — preserving fields not specified)");
        }
        println!("{}", rendered);
        println!(
            "# Would write profile to ~/.config/unleash/profiles/{}.toml",
            agent.name
        );
        return Ok(());
    }

    if let Some(idx) = existing_idx {
        if !args.force {
            eprintln!(
                "warn: custom agent '{}' already registered — merging with existing entry (pass --force to silence)",
                agent.name
            );
        }
        app_config.custom_agents[idx] = agent.clone();
    } else {
        app_config.custom_agents.push(agent.clone());
    }
    mgr.save_app_config(&app_config)?;

    let resolved_binary = which::which(&agent.binary)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| agent.binary.clone());
    // Preserve user customizations (theme, env, defaults, agents overrides,
    // agent_cli_args, stop_prompt) when re-adding an agent whose profile
    // already exists. Only overwrite the fields this subcommand actually
    // owns: name, description, agent_cli_path. The fresh-install path
    // (load_profile returns Err) still falls back to Profile::default.
    let mut profile = mgr
        .load_profile(&agent.name)
        .unwrap_or_else(|_| crate::config::Profile::default());
    profile.name = agent.name.clone();
    profile.description = agent.description.clone();
    profile.agent_cli_path = resolved_binary;
    mgr.save_profile(&profile)?;

    println!(
        "✓ Registered custom agent '{}' — run `unleash {}` to use it.",
        agent.name, agent.name
    );
    Ok(())
}

/// Overlay CLI args onto an existing custom-agent entry. Required fields
/// (`binary`, `headless`) come from the CLI invocation; optional fields are
/// overwritten only when the user explicitly passed the corresponding flag.
/// Fields with no CLI surface (e.g. `effort_flag`, `sandbox`, `fork`,
/// `enabled`) are preserved verbatim from the existing config. Mirrors the
/// profile-level preservation introduced in #349.
fn merge_args_into_existing(
    existing: &crate::config::CustomAgentConfig,
    args: &AddCustomAgentArgs,
    fresh: &crate::config::CustomAgentConfig,
) -> crate::config::CustomAgentConfig {
    let mut merged = existing.clone();
    merged.binary = fresh.binary.clone();
    merged.polyfill.headless = fresh.polyfill.headless.clone();
    if let Some(d) = args.description.clone() {
        merged.description = d;
    }
    if let Some(f) = args.continue_flag.clone() {
        merged.polyfill.session.continue_strategy = ResumeStrategy::Flag(f);
    }
    if let Some(f) = args.resume_flag.clone() {
        merged.polyfill.session.resume_strategy = ResumeStrategy::Flag(f);
    }
    if let Some(f) = args.model_flag.clone() {
        merged.polyfill.model_flag = f;
    }
    if let Some(y) = args.yolo_flag.clone() {
        merged.polyfill.yolo_flag = Some(y);
    }
    if let Some(r) = args.github_repo.clone() {
        merged.github_repo = Some(r);
    }
    if let Some(p) = args.npm_package.clone() {
        merged.npm_package = Some(p);
    }
    merged
}
