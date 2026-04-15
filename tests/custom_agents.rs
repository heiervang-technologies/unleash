use unleash::agents::{AgentDefinition, AgentType};
use unleash::config::AppConfig;
use unleash::polyfill::{self, PolyfillFlags};

/// Full config loading — deserialize TOML with [[custom_agents]] including polyfill,
/// verify AgentDefinition::from_custom_config() produces correct binary, agent_type,
/// model_flag, yolo_flag.
#[test]
fn test_custom_agent_full_config_loading() {
    let toml_str = r#"
current_profile = "claude"

[[custom_agents]]
name = "aider"
binary = "aider"
description = "AI pair programming in your terminal"
github_repo = "paul-gauthier/aider"

[custom_agents.polyfill]
headless = { flag = "--message" }
session = { continue_arg = "--restore-chat-history", resume_arg = "--restore-chat-history" }
fork = "unsupported"
model_flag = "--model"
yolo_flag = "--yes"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.custom_agents.len(), 1);

    let def = AgentDefinition::from_custom_config(&config.custom_agents[0]);
    assert_eq!(def.binary, "aider");
    assert_eq!(def.agent_type, AgentType::Custom("aider".to_string()));
    assert_eq!(def.polyfill.model_flag, "--model");
    assert_eq!(def.polyfill.yolo_flag, Some("--yes".to_string()));
    assert_eq!(def.description, "AI pair programming in your terminal");
    assert_eq!(
        def.github_repo,
        Some("paul-gauthier/aider".to_string())
    );
    assert!(def.enabled);
}

/// Polyfill resolution — build an AgentDefinition from a custom config, then
/// call polyfill::resolve() with model and yolo flags and verify the resolved
/// args contain the expected flag names and values.
#[test]
fn test_polyfill_resolution_for_custom_agent() {
    let toml_str = r#"
[[custom_agents]]
name = "aider"
binary = "aider"

[custom_agents.polyfill]
headless = { flag = "--message" }
session = { continue_arg = "--c", resume_arg = "--r" }
fork = "unsupported"
model_flag = "--model"
yolo_flag = "--yes"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let def = AgentDefinition::from_custom_config(&config.custom_agents[0]);

    let flags = PolyfillFlags {
        model: Some("gpt-4".to_string()),
        yolo: true,
        ..PolyfillFlags::default()
    };

    let resolved = polyfill::resolve(&def.polyfill, &flags, &[]);

    // Model flag and value must be present
    assert!(
        resolved.args.contains(&"--model".to_string()),
        "resolved args should contain --model, got: {:?}",
        resolved.args
    );
    assert!(
        resolved.args.contains(&"gpt-4".to_string()),
        "resolved args should contain gpt-4, got: {:?}",
        resolved.args
    );

    // Yolo flag must be present
    assert!(
        resolved.args.contains(&"--yes".to_string()),
        "resolved args should contain --yes (yolo flag), got: {:?}",
        resolved.args
    );
}

/// Multiple custom agents with enabled flag — verify 2 agents parsed, first
/// enabled, second disabled.
#[test]
fn test_multiple_custom_agents_with_enabled_flag() {
    let toml_str = r#"
[[custom_agents]]
name = "aider"
binary = "aider"
[custom_agents.polyfill]
headless = { flag = "--message" }
session = { continue_arg = "--c", resume_arg = "--r" }
fork = "unsupported"
model_flag = "--model"

[[custom_agents]]
name = "cursor"
binary = "cursor-cli"
enabled = false
[custom_agents.polyfill]
headless = { flag = "-p" }
session = { continue_arg = "--continue", resume_arg = "--resume" }
fork = "unsupported"
model_flag = "--model"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.custom_agents.len(), 2);

    assert_eq!(config.custom_agents[0].name, "aider");
    assert!(config.custom_agents[0].enabled);

    assert_eq!(config.custom_agents[1].name, "cursor");
    assert!(!config.custom_agents[1].enabled);
}

/// Empty custom agents is default — AppConfig with no custom_agents key
/// deserializes with an empty vec.
#[test]
fn test_empty_custom_agents_is_default() {
    let toml_str = r#"current_profile = "claude""#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert!(config.custom_agents.is_empty());
}

/// Custom AgentType equality — verify Custom variants compare by name.
#[test]
fn test_custom_agent_type_equality() {
    assert_eq!(
        AgentType::Custom("aider".into()),
        AgentType::Custom("aider".into()),
        "Custom agents with the same name should be equal"
    );
    assert_ne!(
        AgentType::Custom("aider".into()),
        AgentType::Custom("cursor".into()),
        "Custom agents with different names should not be equal"
    );
    // Custom should not equal any built-in
    assert_ne!(AgentType::Custom("claude".into()), AgentType::Claude);
}
