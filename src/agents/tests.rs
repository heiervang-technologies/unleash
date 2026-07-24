use super::install::{atomic_install_binary, pick_asset_name};
use super::manager::status_update_available;
use super::*;
use std::fs;

fn add_args(name: &str) -> AddCustomAgentArgs {
    AddCustomAgentArgs {
        name: name.into(),
        binary: format!("{}-bin", name),
        headless_flag: Some("-p".into()),
        headless_subcommand: None,
        description: None,
        continue_flag: None,
        resume_flag: None,
        model_flag: None,
        yolo_flag: None,
        github_repo: None,
        npm_package: None,
        dry_run: false,
        force: false,
    }
}

#[test]
fn build_custom_agent_config_uses_defaults_for_omitted_flags() {
    let cfg = build_custom_agent_config(&add_args("aider")).unwrap();
    assert_eq!(cfg.name, "aider");
    assert_eq!(cfg.binary, "aider-bin");
    assert_eq!(cfg.description, "Custom agent: aider");
    assert_eq!(cfg.polyfill.model_flag, "--model");
    assert!(cfg.enabled);
    assert!(matches!(cfg.polyfill.headless, HeadlessStrategy::Flag(ref s) if s == "-p"));
    assert!(matches!(cfg.polyfill.fork, ForkStrategy::Unsupported));
    match &cfg.polyfill.session.continue_strategy {
        ResumeStrategy::Flag(s) => assert_eq!(s, "--continue"),
        _ => panic!("expected continue flag"),
    }
}

#[test]
fn build_custom_agent_config_rejects_empty_name() {
    let mut a = add_args("aider");
    a.name = "  ".into();
    assert!(build_custom_agent_config(&a).is_err());
}

#[test]
fn build_custom_agent_config_rejects_empty_binary() {
    let mut a = add_args("aider");
    a.binary = "".into();
    assert!(build_custom_agent_config(&a).is_err());
}

#[test]
fn build_custom_agent_config_rejects_builtin_name_clash() {
    for builtin in [
        "claude", "codex", "gemini", "opencode", "pi", "hermes", "agy",
    ] {
        assert!(
            build_custom_agent_config(&add_args(builtin)).is_err(),
            "expected '{}' to clash with built-in",
            builtin
        );
    }
}

#[test]
fn build_custom_agent_config_requires_some_headless_strategy() {
    let mut a = add_args("aider");
    a.headless_flag = None;
    a.headless_subcommand = None;
    assert!(build_custom_agent_config(&a).is_err());
}

#[test]
fn build_custom_agent_config_subcommand_headless() {
    let mut a = add_args("aider");
    a.headless_flag = None;
    a.headless_subcommand = Some("exec".into());
    let cfg = build_custom_agent_config(&a).unwrap();
    assert!(matches!(cfg.polyfill.headless, HeadlessStrategy::Subcommand(ref s) if s == "exec"));
}

#[test]
fn add_custom_agent_with_writes_app_config_and_profile() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mgr =
        crate::config::ProfileManager::with_config_dir(tmp.path().to_path_buf()).expect("manager");

    add_custom_agent_with(&mgr, add_args("myagent")).expect("add");

    let cfg = mgr.load_app_config().expect("load");
    assert_eq!(cfg.custom_agents.len(), 1);
    assert_eq!(cfg.custom_agents[0].name, "myagent");

    let profile_path = tmp.path().join("profiles").join("myagent.toml");
    assert!(profile_path.exists(), "profile file should exist");
}

fn aider_def() -> AgentDefinition {
    AgentDefinition {
        agent_type: AgentType::Custom("aider".into()),
        name: "aider".into(),
        binary: "aider".into(),
        description: "AI pair programmer".into(),
        polyfill: AgentPolyfillConfig {
            headless: HeadlessStrategy::Flag("--message".into()),
            session: SessionStrategy {
                continue_strategy: ResumeStrategy::Flag("--restore-chat-history".into()),
                resume_strategy: ResumeStrategy::Flag("--restore-chat-history".into()),
            },
            fork: ForkStrategy::Unsupported,
            yolo_flag: Some("--yes-always".into()),
            model_flag: "--model".into(),
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
        github_repo: Some("paul-gauthier/aider".into()),
        npm_package: None,
        enabled: true,
    }
}

#[test]
fn manager_includes_custom_agents_in_listing() {
    let mgr = AgentManager::new_with_custom_for_tests(vec![aider_def()]).expect("manager");
    let agents: Vec<_> = mgr
        .list_agents()
        .into_iter()
        .map(|d| d.name.clone())
        .collect();
    assert!(
        agents.contains(&"aider".to_string()),
        "custom agent must surface in list_agents"
    );
}

#[test]
fn manager_resolves_custom_agent_by_type() {
    let mgr = AgentManager::new_with_custom_for_tests(vec![aider_def()]).expect("manager");
    let def = mgr.get_agent(AgentType::Custom("aider".into()));
    assert!(
        def.is_some(),
        "get_agent must return registered custom agent"
    );
    assert_eq!(
        def.unwrap().github_repo.as_deref(),
        Some("paul-gauthier/aider")
    );
}

#[test]
fn resolve_agent_type_handles_builtin_aliases() {
    let mgr = AgentManager::new_with_custom_for_tests(vec![]).expect("manager");
    assert_eq!(mgr.resolve_agent_type("claude"), Some(AgentType::Claude));
    assert_eq!(
        mgr.resolve_agent_type("claude-code"),
        Some(AgentType::Claude)
    );
    assert_eq!(mgr.resolve_agent_type("agy"), Some(AgentType::Antigravity));
}

#[test]
fn resolve_agent_type_finds_registered_custom_agent() {
    let mgr = AgentManager::new_with_custom_for_tests(vec![aider_def()]).expect("manager");
    assert_eq!(
        mgr.resolve_agent_type("aider"),
        Some(AgentType::Custom("aider".to_string())),
        "registered custom agent must resolve via its name"
    );
}

#[test]
fn resolve_agent_type_returns_none_for_unregistered_custom() {
    let mgr = AgentManager::new_with_custom_for_tests(vec![]).expect("manager");
    assert_eq!(
        mgr.resolve_agent_type("unknown-agent-xyz"),
        None,
        "must not invent Custom() for unregistered names"
    );
}

#[test]
fn update_custom_agent_errors_when_not_in_config() {
    // Per the #338 install/update implementation: update_custom reads from
    // AppConfig (the persisted [[custom_agents]] block), not just the
    // in-memory AgentManager registry. If the agent isn't there, the
    // error must point at `unleash agents add` as the fix.
    let tmp = tempfile::tempdir().expect("tempdir");
    let pm =
        crate::config::ProfileManager::with_config_dir(tmp.path().to_path_buf()).expect("manager");
    let am = AgentManager::new_with_custom_for_tests(vec![aider_def()]).expect("manager");
    let err = am
        .update_custom_with_manager("unregistered-xyz", &pm)
        .expect_err("must error when agent missing from AppConfig");
    let msg = err.to_string();
    assert!(
        msg.contains("unregistered-xyz"),
        "error should name the missing agent: {}",
        msg
    );
    assert!(
        msg.contains("agents add"),
        "error should point at the `unleash agents add` fix path: {}",
        msg
    );
}

#[test]
fn update_custom_agent_errors_when_github_repo_missing() {
    // Per #338: convention/template both resolve from a GitHub release.
    // An agent registered without a github_repo can't be auto-updated,
    // so we error with a hint pointing the user at the config field
    // they need to set rather than silently doing nothing.
    let tmp = tempfile::tempdir().expect("tempdir");
    let pm =
        crate::config::ProfileManager::with_config_dir(tmp.path().to_path_buf()).expect("manager");
    let mut existing = pm.load_app_config().expect("load");
    existing
        .custom_agents
        .push(crate::config::CustomAgentConfig {
            name: "noupdate".into(),
            binary: "noupdate".into(),
            description: "no repo".into(),
            polyfill: aider_def().polyfill,
            github_repo: None,
            npm_package: None,
            asset_template: None,
            enabled: true,
        });
    pm.save_app_config(&existing).expect("save");

    let am = AgentManager::new_with_custom_for_tests(vec![]).expect("manager");
    let err = am
        .update_custom_with_manager("noupdate", &pm)
        .expect_err("must error when github_repo missing");
    let msg = err.to_string();
    assert!(
        msg.contains("github_repo"),
        "error must name the missing field so user knows what to set: {}",
        msg
    );
    assert!(
        msg.contains("noupdate"),
        "error must name the agent: {}",
        msg
    );
}

#[test]
fn add_custom_agent_with_preserves_user_profile_customizations() {
    // Regression: re-adding an agent whose profile already exists must NOT
    // clobber user-customized fields (theme, env, defaults, agents
    // overrides, agent_cli_args, stop_prompt). Only name, description, and
    // agent_cli_path are owned by this subcommand.
    let tmp = tempfile::tempdir().expect("tempdir");
    let mgr =
        crate::config::ProfileManager::with_config_dir(tmp.path().to_path_buf()).expect("manager");

    // Pre-create a profile with hand-customized fields the user added in
    // the TUI editor.
    let mut existing = crate::config::Profile {
        name: "aider".into(),
        agent_cli_path: "/old/path/to/aider".into(),
        theme: "orange".into(),
        agent_cli_args: vec!["--my-custom-arg".into()],
        stop_prompt: Some("Custom stop prompt".into()),
        ..crate::config::Profile::default()
    };
    existing
        .env
        .insert("CUSTOM_KEY".into(), "custom_value".into());
    mgr.save_profile(&existing).expect("pre-save");

    // Re-add with new binary — should overwrite name/description/path only.
    let mut a = add_args("aider");
    a.description = Some("Pair programmer".into());
    a.force = true;
    add_custom_agent_with(&mgr, a).expect("re-add");

    let after = mgr.load_profile("aider").expect("load");
    assert_eq!(after.description, "Pair programmer", "description updated");
    assert_eq!(after.theme, "orange", "theme preserved");
    assert_eq!(
        after.agent_cli_args,
        vec!["--my-custom-arg".to_string()],
        "agent_cli_args preserved"
    );
    assert_eq!(
        after.stop_prompt.as_deref(),
        Some("Custom stop prompt"),
        "stop_prompt preserved"
    );
    assert_eq!(
        after.env.get("CUSTOM_KEY").map(String::as_str),
        Some("custom_value"),
        "env entries preserved"
    );
}

#[test]
fn add_custom_agent_with_propagates_description_to_profile() {
    // Regression: the profile is built with `..Profile::default()`, which
    // hardcodes Claude's name + description. Without an explicit override
    // the custom agent's profile shows up in TUI search as "Claude Code by
    // Anthropic" — wrong on its face and pollutes description-based filter
    // matching. Pin the override.
    let tmp = tempfile::tempdir().expect("tempdir");
    let mgr =
        crate::config::ProfileManager::with_config_dir(tmp.path().to_path_buf()).expect("manager");

    let mut a = add_args("aider");
    a.description = Some("Pair programmer".into());
    add_custom_agent_with(&mgr, a).expect("add");

    let profile = mgr.load_profile("aider").expect("load profile");
    assert_eq!(profile.description, "Pair programmer");
    assert_ne!(
        profile.description, "Claude Code by Anthropic",
        "must not inherit Profile::default description"
    );
}

#[test]
fn add_custom_agent_with_is_idempotent_on_reregister() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mgr =
        crate::config::ProfileManager::with_config_dir(tmp.path().to_path_buf()).expect("manager");

    add_custom_agent_with(&mgr, add_args("twice")).expect("first add");
    let mut a2 = add_args("twice");
    a2.binary = "different-bin".into();
    a2.force = true;
    add_custom_agent_with(&mgr, a2).expect("second add");

    let cfg = mgr.load_app_config().expect("load");
    assert_eq!(cfg.custom_agents.len(), 1, "should overwrite, not append");
    assert_eq!(cfg.custom_agents[0].binary, "different-bin");
}

#[test]
fn add_custom_agent_with_dry_run_does_not_touch_disk() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mgr =
        crate::config::ProfileManager::with_config_dir(tmp.path().to_path_buf()).expect("manager");

    let mut a = add_args("nope");
    a.dry_run = true;
    add_custom_agent_with(&mgr, a).expect("dry run");

    let cfg = mgr.load_app_config().expect("load");
    assert!(cfg.custom_agents.is_empty());
    let profile_path = tmp.path().join("profiles").join("nope.toml");
    assert!(!profile_path.exists());
}

#[test]
fn add_custom_agent_with_preserves_existing_config_fields_on_readd() {
    // Regression: re-running `unleash agents add` without specifying every
    // optional CLI flag must NOT clobber hand-edited [[custom_agents]]
    // fields (effort_flag, sandbox, fork, model_flag override, github_repo,
    // yolo_flag, enabled). Only fields the user explicitly passes on the
    // CLI — plus the always-required binary + headless — should change.
    // Mirrors #349 at the config-block level rather than profile level.
    let tmp = tempfile::tempdir().expect("tempdir");
    let mgr =
        crate::config::ProfileManager::with_config_dir(tmp.path().to_path_buf()).expect("manager");

    let mut existing = mgr.load_app_config().expect("load");
    existing
        .custom_agents
        .push(crate::config::CustomAgentConfig {
            name: "aider".into(),
            binary: "aider-old".into(),
            description: "Hand-tuned description".into(),
            polyfill: AgentPolyfillConfig {
                headless: HeadlessStrategy::Flag("--old-prompt".into()),
                session: SessionStrategy {
                    continue_strategy: ResumeStrategy::Flag("--restore-chat".into()),
                    resume_strategy: ResumeStrategy::Flag("--restore-chat".into()),
                },
                fork: ForkStrategy::Unsupported,
                yolo_flag: Some("--yes".into()),
                model_flag: "--mdl".into(),
                effort_flag: Some("--effort".into()),
                auto_flag: None,
                verbose_flag: Some("--verbose".into()),
                output_format_flag: None,
                system_prompt_flag: None,
                allowed_tools_flag: None,
                sandbox: SandboxStrategy::BoolFlag("--sandbox".into()),
                name_flag: None,
                add_dir_flag: None,
                approval_mode_flag: None,
                worktree_flag: None,
                interactive_prompt_flag: None,
            },
            github_repo: Some("paul-gauthier/aider".into()),
            npm_package: None,
            asset_template: None,
            enabled: false,
        });
    mgr.save_app_config(&existing).expect("pre-save");

    // Re-add specifying only binary + headless-flag (and the description) —
    // every other optional flag is omitted.
    let mut a = add_args("aider");
    a.binary = "aider-new".into();
    a.headless_flag = Some("--new-prompt".into());
    a.description = Some("Updated description".into());
    a.force = true;
    add_custom_agent_with(&mgr, a).expect("re-add");

    let after = mgr.load_app_config().expect("load");
    assert_eq!(after.custom_agents.len(), 1, "no duplicate entry");
    let entry = &after.custom_agents[0];

    // Required fields took the CLI values.
    assert_eq!(entry.binary, "aider-new");
    assert!(matches!(
        entry.polyfill.headless,
        HeadlessStrategy::Flag(ref f) if f == "--new-prompt"
    ));
    // Explicit CLI overrides applied.
    assert_eq!(entry.description, "Updated description");
    // Omitted CLI flags must NOT have wiped existing values.
    assert_eq!(entry.polyfill.effort_flag.as_deref(), Some("--effort"));
    assert_eq!(entry.polyfill.verbose_flag.as_deref(), Some("--verbose"));
    assert_eq!(entry.polyfill.yolo_flag.as_deref(), Some("--yes"));
    assert_eq!(entry.polyfill.model_flag, "--mdl");
    assert!(matches!(
        entry.polyfill.session.continue_strategy,
        ResumeStrategy::Flag(ref f) if f == "--restore-chat"
    ));
    assert!(matches!(
        entry.polyfill.session.resume_strategy,
        ResumeStrategy::Flag(ref f) if f == "--restore-chat"
    ));
    assert!(matches!(
        entry.polyfill.sandbox,
        SandboxStrategy::BoolFlag(ref f) if f == "--sandbox"
    ));
    assert_eq!(entry.github_repo.as_deref(), Some("paul-gauthier/aider"));
    assert!(!entry.enabled, "enabled state preserved across re-add");
}

#[test]
fn build_custom_agent_config_honors_overrides() {
    let mut a = add_args("aider");
    a.description = Some("Pair programmer".into());
    a.continue_flag = Some("-c".into());
    a.resume_flag = Some("-r".into());
    a.model_flag = Some("-m".into());
    a.yolo_flag = Some("--yes".into());
    a.github_repo = Some("paul-gauthier/aider".into());
    a.npm_package = Some("aider-chat".into());
    let cfg = build_custom_agent_config(&a).unwrap();
    assert_eq!(cfg.description, "Pair programmer");
    assert_eq!(cfg.polyfill.model_flag, "-m");
    assert_eq!(cfg.polyfill.yolo_flag.as_deref(), Some("--yes"));
    assert_eq!(cfg.github_repo.as_deref(), Some("paul-gauthier/aider"));
    assert_eq!(cfg.npm_package.as_deref(), Some("aider-chat"));
    match &cfg.polyfill.session.continue_strategy {
        ResumeStrategy::Flag(s) => assert_eq!(s, "-c"),
        _ => panic!("wrong continue strategy"),
    }
    match &cfg.polyfill.session.resume_strategy {
        ResumeStrategy::Flag(s) => assert_eq!(s, "-r"),
        _ => panic!("wrong resume strategy"),
    }
}

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
fn clanker_is_first_class_and_ranked_directly_after_codex() {
    let builtins = AgentType::builtin();
    assert_eq!(builtins[1], AgentType::Codex);
    assert_eq!(builtins[2], AgentType::Clanker);
    assert!(
        builtins
            .iter()
            .position(|agent| *agent == AgentType::Clanker)
            < builtins.iter().position(|agent| *agent == AgentType::Pi)
    );
    assert!(
        builtins
            .iter()
            .position(|agent| *agent == AgentType::Clanker)
            < builtins
                .iter()
                .position(|agent| *agent == AgentType::Hermes)
    );
    assert_eq!(AgentType::from_str("clanker"), Some(AgentType::Clanker));
    assert_eq!(
        AgentType::from_str("clanker-code"),
        Some(AgentType::Clanker)
    );
}

#[test]
fn clanker_definition_uses_fork_binary_repository_and_name_flag() {
    let clanker = AgentDefinition::clanker();
    assert_eq!(clanker.agent_type, AgentType::Clanker);
    assert_eq!(clanker.binary, "clanker");
    assert_eq!(
        clanker.github_repo.as_deref(),
        Some(crate::clanker::REPOSITORY)
    );
    assert!(clanker.npm_package.is_none());
    assert_eq!(clanker.polyfill.name_flag.as_deref(), Some("--name"));
    assert!(matches!(
        clanker.polyfill.headless,
        HeadlessStrategy::Subcommand(ref command) if command == "exec"
    ));
}

#[test]
fn clanker_status_compares_revisions_instead_of_product_semver() {
    let latest = "40e7d1c0d9b0621d756eb14a5aa7735466aca0a9";
    let prior = "1111111111111111111111111111111111111111";

    assert!(!status_update_available(
        &AgentType::Clanker,
        Some("0.1.0+codex.0.143.0"),
        Some(latest),
        Some(latest),
    ));
    assert!(status_update_available(
        &AgentType::Clanker,
        Some("999.0.0"),
        Some(latest),
        Some(prior),
    ));
    assert!(status_update_available(
        &AgentType::Clanker,
        Some("999.0.0"),
        Some(latest),
        None,
    ));
}

#[test]
fn legacy_custom_clanker_is_suppressed_from_builtin_picker_order() {
    let mut legacy = AgentDefinition::clanker();
    legacy.agent_type = AgentType::Custom("clanker".to_string());
    let types = AgentType::all_with_custom(&[legacy]);
    assert_eq!(
        types
            .iter()
            .filter(|agent| **agent == AgentType::Clanker)
            .count(),
        1
    );
    assert!(!types.contains(&AgentType::Custom("clanker".to_string())));
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

// ── pick_asset_name: Shape B (convention) + Shape A (template) ────────
// These tests pin the #338 install/update path's resolver behavior.
// The function is pure (no I/O), so the full asset-picking decision tree
// is exercisable without network or fixture-release files.

fn assets(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn pick_asset_convention_arch_os_canonical() {
    // Default Shape B: <name>-<arch>-<os> matches the canonical aliases.
    let avail = assets(&["aider-x86_64-linux", "checksums.txt"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "linux", "0.50.0", "v0.50.0", &avail),
        Some("aider-x86_64-linux".to_string())
    );
}

#[test]
fn pick_asset_convention_arch_alias_amd64() {
    // amd64 is the most common x86_64 alias in GitHub releases.
    let avail = assets(&["aider-amd64-linux"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "linux", "0.50.0", "v0.50.0", &avail),
        Some("aider-amd64-linux".to_string())
    );
}

#[test]
fn pick_asset_convention_arch_alias_arm64() {
    let avail = assets(&["aider-arm64-linux"]);
    assert_eq!(
        pick_asset_name("aider", None, "aarch64", "linux", "0.50.0", "v0.50.0", &avail),
        Some("aider-arm64-linux".to_string())
    );
}

#[test]
fn pick_asset_convention_os_alias_darwin() {
    let avail = assets(&["aider-x86_64-darwin"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "macos", "0.50.0", "v0.50.0", &avail),
        Some("aider-x86_64-darwin".to_string())
    );
}

#[test]
fn pick_asset_convention_os_arch_order_reversed() {
    // Some projects publish <name>-<os>-<arch> instead of -<arch>-<os>.
    let avail = assets(&["aider-linux-x86_64"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "linux", "0.50.0", "v0.50.0", &avail),
        Some("aider-linux-x86_64".to_string())
    );
}

#[test]
fn pick_asset_convention_underscore_separator() {
    // Some projects use underscores instead of hyphens.
    let avail = assets(&["aider_amd64_linux"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "linux", "0.50.0", "v0.50.0", &avail),
        Some("aider_amd64_linux".to_string())
    );
}

#[test]
fn pick_asset_convention_tarball_extension() {
    // .tar.gz extension covers archive-distributed binaries.
    let avail = assets(&["aider-x86_64-linux.tar.gz"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "linux", "0.50.0", "v0.50.0", &avail),
        Some("aider-x86_64-linux.tar.gz".to_string())
    );
}

#[test]
fn pick_asset_convention_zip_extension() {
    let avail = assets(&["aider-x86_64-linux.zip"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "linux", "0.50.0", "v0.50.0", &avail),
        Some("aider-x86_64-linux.zip".to_string())
    );
}

#[test]
fn pick_asset_convention_bare_name_fallback() {
    // Universal binary: just the agent name with no arch/os qualifier.
    let avail = assets(&["aider", "README.md"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "linux", "0.50.0", "v0.50.0", &avail),
        Some("aider".to_string())
    );
}

#[test]
fn pick_asset_convention_no_match_returns_none() {
    // No asset matches the convention — caller will surface the asset
    // list to the user with an asset_template hint.
    let avail = assets(&["foo.dmg", "bar.exe", "weird-name-1.0.0.pkg"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "linux", "0.50.0", "v0.50.0", &avail),
        None
    );
}

#[test]
fn pick_asset_convention_prefers_no_extension_over_archive() {
    // When both `aider-x86_64-linux` and `aider-x86_64-linux.tar.gz` exist,
    // pick the bare binary first — saves an extraction step and matches
    // what `which aider` would find on a real system.
    let avail = assets(&["aider-x86_64-linux", "aider-x86_64-linux.tar.gz"]);
    assert_eq!(
        pick_asset_name("aider", None, "x86_64", "linux", "0.50.0", "v0.50.0", &avail),
        Some("aider-x86_64-linux".to_string())
    );
}

#[test]
fn pick_asset_template_substitutes_placeholders() {
    // Shape A: explicit template overrides convention.
    let avail = assets(&["my-cli-0.50.0-x86_64-linux.tar.gz"]);
    assert_eq!(
        pick_asset_name(
            "my-cli",
            Some("{name}-{version}-{arch}-{os}.tar.gz"),
            "x86_64",
            "linux",
            "0.50.0",
            "v0.50.0",
            &avail
        ),
        Some("my-cli-0.50.0-x86_64-linux.tar.gz".to_string())
    );
}

#[test]
fn pick_asset_template_supports_tag_with_v_prefix() {
    // {tag} preserves the literal tag (with `v`), {version} strips it.
    let avail = assets(&["my-cli-v0.50.0-x86_64-linux.tar.gz"]);
    assert_eq!(
        pick_asset_name(
            "my-cli",
            Some("{name}-{tag}-{arch}-{os}.tar.gz"),
            "x86_64",
            "linux",
            "0.50.0",
            "v0.50.0",
            &avail
        ),
        Some("my-cli-v0.50.0-x86_64-linux.tar.gz".to_string())
    );
}

#[test]
fn pick_asset_template_no_match_returns_none() {
    // If the user-specified template doesn't match any asset, the caller
    // surfaces the asset list with a "set asset_template" hint. We must
    // NOT silently fall back to the convention — that would mask user
    // intent and surprise them.
    let avail = assets(&[
        "my-cli-x86_64-linux", // would match Shape B convention
        "my-cli-0.50.0.zip",
    ]);
    assert_eq!(
        pick_asset_name(
            "my-cli",
            Some("{name}-{version}-{arch}-{os}.tar.gz"),
            "x86_64",
            "linux",
            "0.50.0",
            "v0.50.0",
            &avail
        ),
        None,
        "explicit template miss must NOT fall back to convention"
    );
}

#[test]
fn pick_asset_unsupported_arch_returns_none() {
    // arch we don't have alias coverage for (e.g. riscv64) — the caller
    // should still get None and surface a clear error rather than picking
    // a nonsense asset.
    let avail = assets(&["aider-riscv64-linux"]);
    assert_eq!(
        pick_asset_name("aider", None, "riscv64", "linux", "0.50.0", "v0.50.0", &avail),
        None
    );
}

// ── atomic_install_binary ────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn atomic_install_replaces_content_and_marks_executable() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("subdir").join("agent-bin");
    // A stale, non-executable file already sits at the install path.
    fs::create_dir_all(dst.parent().unwrap()).unwrap();
    fs::write(&dst, b"OLD BINARY").unwrap();
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o644)).unwrap();

    let src = tmp.path().join("freshly-downloaded");
    fs::write(&src, b"NEW BINARY CONTENT").unwrap();

    atomic_install_binary(&src, &dst).expect("install");

    assert_eq!(fs::read(&dst).unwrap(), b"NEW BINARY CONTENT");
    let mode = fs::metadata(&dst).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "installed binary must be executable");

    // No `.agent-bin.unleash-install.*` temp turd left behind.
    let leftovers: Vec<_> = fs::read_dir(dst.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains("unleash-install"))
        .collect();
    assert!(leftovers.is_empty(), "temp file leaked: {leftovers:?}");
}

#[cfg(unix)]
#[test]
fn atomic_install_swaps_inode_instead_of_writing_in_place() {
    // Regression for #353 "remaining binary install/copy paths atomic":
    // the previous `fs::copy(src, dst)` opened `dst` and truncated it *in
    // place*, mutating the inode. Any other reference to that inode — a
    // running process's executable image, or a hard link — would observe
    // the half-written / replaced bytes. The atomic helper renames a fresh
    // inode into place, so existing references keep seeing the old binary.
    //
    // A hard link is a deterministic, root-safe stand-in for "the currently
    // running binary". With the old direct-copy behavior this test FAILS:
    // the link reads NEW because it shares the truncated-in-place inode.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("agent-bin");
    fs::write(&dst, b"OLD BINARY").unwrap();

    let other_ref = tmp.path().join("agent-bin.inuse");
    fs::hard_link(&dst, &other_ref).expect("hard link");

    let src = tmp.path().join("freshly-downloaded");
    fs::write(&src, b"NEW BINARY").unwrap();

    atomic_install_binary(&src, &dst).expect("install");

    assert_eq!(fs::read(&dst).unwrap(), b"NEW BINARY", "dst updated");
    assert_eq!(
        fs::read(&other_ref).unwrap(),
        b"OLD BINARY",
        "prior reference must still see the old inode (atomic swap, not in-place write)"
    );
}

#[cfg(unix)]
#[test]
fn atomic_install_sweeps_stale_temp_from_hard_killed_run() {
    // A prior install SIGKILL'd between copy and rename leaves a
    // `.agent-bin.unleash-install.<pid>` turd. The next install must
    // self-heal by sweeping it, not accumulate leaked temps.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("agent-bin");
    let stale = tmp.path().join(".agent-bin.unleash-install.999999");
    fs::write(&stale, b"leaked partial").unwrap();

    let src = tmp.path().join("freshly-downloaded");
    fs::write(&src, b"NEW BINARY").unwrap();
    atomic_install_binary(&src, &dst).expect("install");

    assert!(!stale.exists(), "stale temp must be swept");
    assert_eq!(fs::read(&dst).unwrap(), b"NEW BINARY");
}

#[cfg(all(unix, target_os = "linux"))]
#[test]
fn atomic_install_preserves_live_concurrent_temp() {
    // The sweep must NOT delete a concurrent install's in-flight temp: a
    // temp suffixed with a live pid (our own) is left alone, while one
    // suffixed with a dead pid is swept.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("agent-bin");
    // pid 1 (init) is always live; avoids colliding with the helper's own
    // staging temp, which uses our pid.
    let live = tmp.path().join(".agent-bin.unleash-install.1");
    let dead = tmp.path().join(".agent-bin.unleash-install.999999");
    fs::write(&live, b"racer in-flight").unwrap();
    fs::write(&dead, b"stale leak").unwrap();

    let src = tmp.path().join("freshly-downloaded");
    fs::write(&src, b"NEW BINARY").unwrap();
    atomic_install_binary(&src, &dst).expect("install");

    assert!(live.exists(), "live concurrent temp must be preserved");
    assert!(!dead.exists(), "dead-pid stale temp must be swept");
}

#[cfg(unix)]
#[test]
fn atomic_install_leaves_no_partial_on_missing_source() {
    // A failed install (source vanished) must not leave a partial temp file
    // or a truncated destination behind.
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst = tmp.path().join("agent-bin");
    fs::write(&dst, b"GOOD EXISTING").unwrap();

    let missing = tmp.path().join("does-not-exist");
    let r = atomic_install_binary(&missing, &dst);
    assert!(r.is_err(), "install from missing source must error");

    // Existing good binary untouched, no temp turd.
    assert_eq!(fs::read(&dst).unwrap(), b"GOOD EXISTING");
    let leftovers: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains("unleash-install"))
        .collect();
    assert!(leftovers.is_empty(), "temp file leaked: {leftovers:?}");
}
