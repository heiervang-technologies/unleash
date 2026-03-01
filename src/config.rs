//! Profile configuration management
//!
//! Profiles are stored in ~/.config/agent-unleashed/profiles/
//! Each profile is a TOML file with agent settings and environment variables.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

/// A profile containing agent settings and environment variables for a session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    /// Display name for the profile
    pub name: String,
    /// Optional description
    #[serde(default)]
    pub description: String,
    /// Path to agent CLI executable (default: "aug" for full unleashed features)
    #[serde(default = "default_agent_cli_path", alias = "claude_path")]
    pub agent_cli_path: String,
    /// Additional arguments to pass to the agent CLI
    #[serde(default)]
    pub claude_args: Vec<String>,
    /// Custom stop-hook prompt for auto-mode (None = use default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_prompt: Option<String>,
    /// Color theme name (e.g., "orange", "blue", "green", or "#RRGGBB")
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Environment variables to set when launching the agent
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            description: "Default profile".to_string(),
            agent_cli_path: default_agent_cli_path(),
            claude_args: Vec::new(),
            stop_prompt: None,
            theme: default_theme(),
            env: default_env(),
        }
    }
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            agent_cli_path: default_agent_cli_path(),
            claude_args: Vec::new(),
            stop_prompt: None,
            theme: default_theme(),
            env: default_env(),
        }
    }

    /// Create a profile with common Anthropic env vars
    #[allow(dead_code)]
    pub fn with_api_key(name: &str, api_key: &str) -> Self {
        let mut env = HashMap::new();
        env.insert("ANTHROPIC_API_KEY".to_string(), api_key.to_string());
        Self {
            name: name.to_string(),
            description: format!("API key profile: {}", name),
            agent_cli_path: default_agent_cli_path(),
            claude_args: Vec::new(),
            stop_prompt: None,
            theme: default_theme(),
            env,
        }
    }

    /// Set an environment variable
    #[allow(dead_code)]
    pub fn set_env(&mut self, key: &str, value: &str) {
        self.env.insert(key.to_string(), value.to_string());
    }

    /// Get an environment variable
    #[allow(dead_code)]
    pub fn get_env(&self, key: &str) -> Option<&String> {
        self.env.get(key)
    }

    /// Remove an environment variable
    #[allow(dead_code)]
    pub fn remove_env(&mut self, key: &str) -> Option<String> {
        self.env.remove(key)
    }
}

/// Global app configuration (just tracks which profile is active)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// The currently selected profile name
    #[serde(default = "default_profile_name")]
    pub current_profile: String,
    /// Whether TUI animations are enabled
    #[serde(default = "default_true")]
    pub animations: bool,
    /// Whether the first-time setup wizard has been completed
    #[serde(default)]
    pub setup_completed: bool,
}

fn default_true() -> bool {
    true
}

fn default_profile_name() -> String {
    "default".to_string()
}

fn default_theme() -> String {
    "orange".to_string()
}

fn default_agent_cli_path() -> String {
    // Default to aug (au go) for full unleashed features:
    // - Auto-mode via Stop hook enforcement
    // - Restart/resurrection support
    // - Plugin loading
    // - Extended timeouts
    "aug".to_string()
}

fn default_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("AU_HYPRLAND_FOCUS".to_string(), "1".to_string());
    env
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            current_profile: default_profile_name(),
            animations: true,
            setup_completed: false,
        }
    }
}

/// Legacy app config format for migration detection
/// Used to read old config.toml files that had settings fields
#[derive(Deserialize)]
struct LegacyAppConfig {
    current_profile: Option<String>,
    #[serde(alias = "agent_cli_path")]
    claude_path: Option<String>,
    claude_args: Option<Vec<String>>,
    stop_prompt: Option<String>,
    theme: Option<String>,
}

impl LegacyAppConfig {
    /// Check if this config has any legacy fields that need migration
    fn needs_migration(&self) -> bool {
        self.claude_path.is_some()
            || self.claude_args.is_some()
            || self.stop_prompt.is_some()
            || self.theme.is_some()
    }
}

/// Manages profile storage and retrieval
pub struct ProfileManager {
    config_dir: PathBuf,
    profiles_dir: PathBuf,
}

impl ProfileManager {
    /// Create a new ProfileManager with the default config directory
    pub fn new() -> io::Result<Self> {
        let config_dir = Self::default_config_dir()?;
        Self::with_config_dir(config_dir)
    }

    /// Create a ProfileManager with a custom config directory (for testing)
    pub fn with_config_dir(config_dir: PathBuf) -> io::Result<Self> {
        let profiles_dir = config_dir.join("profiles");

        // Ensure directories exist
        fs::create_dir_all(&profiles_dir)?;

        let manager = Self {
            config_dir,
            profiles_dir,
        };

        // Migrate legacy config.toml settings into profiles
        manager.migrate_if_needed()?;

        // Create default profile if none exist
        if manager.list_profiles()?.is_empty() {
            manager.save_profile(&Profile::default())?;
        }

        Ok(manager)
    }

    /// Get the default config directory (~/.config/agent-unleashed)
    pub fn default_config_dir() -> io::Result<PathBuf> {
        let config_base = dirs::config_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find config directory"))?;

        Ok(config_base.join("agent-unleashed"))
    }

    /// Get path to a profile file
    fn profile_path(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(format!("{}.toml", name))
    }

    /// Get path to the app config file
    fn app_config_path(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// Migrate legacy config.toml format to new format.
    /// Old format had claude_path, claude_args, stop_prompt, theme in config.toml.
    /// New format moves those into the profile TOML files.
    fn migrate_if_needed(&self) -> io::Result<()> {
        let config_path = self.app_config_path();
        if !config_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&config_path)?;
        let legacy: LegacyAppConfig = toml::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        if !legacy.needs_migration() {
            return Ok(());
        }

        // Determine target profile name
        let profile_name = legacy.current_profile.clone()
            .unwrap_or_else(default_profile_name);

        // Load or create the target profile
        let mut profile = self.load_profile(&profile_name)
            .unwrap_or_else(|_| Profile::new(&profile_name));

        // Copy legacy settings into profile
        if let Some(path) = legacy.claude_path {
            profile.agent_cli_path = path;
        }
        if let Some(args) = legacy.claude_args {
            profile.claude_args = args;
        }
        if let Some(prompt) = legacy.stop_prompt {
            profile.stop_prompt = Some(prompt);
        }
        if let Some(theme) = legacy.theme {
            profile.theme = theme;
        }

        // Save the enriched profile
        self.save_profile(&profile)?;

        // Rewrite config.toml with only current_profile
        let new_config = AppConfig {
            current_profile: profile_name,
            ..Default::default()
        };
        self.save_app_config(&new_config)?;

        Ok(())
    }

    /// Load a profile by name
    pub fn load_profile(&self, name: &str) -> io::Result<Profile> {
        let path = self.profile_path(name);
        let content = fs::read_to_string(&path)?;
        toml::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }

    /// Save a profile
    pub fn save_profile(&self, profile: &Profile) -> io::Result<()> {
        let path = self.profile_path(&profile.name);
        let content = toml::to_string_pretty(profile)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        fs::write(&path, content)
    }

    /// Delete a profile
    pub fn delete_profile(&self, name: &str) -> io::Result<()> {
        let path = self.profile_path(name);
        if path.exists() {
            fs::remove_file(path)
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "Profile not found"))
        }
    }

    /// List all available profiles
    pub fn list_profiles(&self) -> io::Result<Vec<String>> {
        let mut profiles = Vec::new();

        if self.profiles_dir.exists() {
            for entry in fs::read_dir(&self.profiles_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "toml") {
                    if let Some(name) = path.file_stem() {
                        profiles.push(name.to_string_lossy().to_string());
                    }
                }
            }
        }

        profiles.sort();
        Ok(profiles)
    }

    /// Load all profiles
    pub fn load_all_profiles(&self) -> io::Result<Vec<Profile>> {
        let names = self.list_profiles()?;
        let mut profiles = Vec::new();

        for name in names {
            match self.load_profile(&name) {
                Ok(profile) => profiles.push(profile),
                Err(e) => eprintln!("Warning: Failed to load profile '{}': {}", name, e),
            }
        }

        Ok(profiles)
    }

    /// Load the app config
    pub fn load_app_config(&self) -> io::Result<AppConfig> {
        let path = self.app_config_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            toml::from_str(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
        } else {
            Ok(AppConfig::default())
        }
    }

    /// Save the app config
    pub fn save_app_config(&self, config: &AppConfig) -> io::Result<()> {
        let path = self.app_config_path();
        let content = toml::to_string_pretty(config)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        fs::write(&path, content)
    }

    /// Get the config directory path
    pub fn config_dir(&self) -> &PathBuf {
        &self.config_dir
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new().expect("Failed to create ProfileManager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_manager() -> (ProfileManager, TempDir) {
        let temp = TempDir::new().unwrap();
        let manager = ProfileManager::with_config_dir(temp.path().to_path_buf()).unwrap();
        (manager, temp)
    }

    #[test]
    fn test_default_profile_created() {
        let (manager, _temp) = test_manager();
        let profiles = manager.list_profiles().unwrap();
        assert!(profiles.contains(&"default".to_string()));
    }

    #[test]
    fn test_save_and_load_profile() {
        let (manager, _temp) = test_manager();

        let mut profile = Profile::new("test");
        profile.description = "Test profile".to_string();
        profile.agent_cli_path = "custom-cli".to_string();
        profile.theme = "blue".to_string();
        profile.set_env("ANTHROPIC_API_KEY", "sk-test-123");
        profile.set_env("ANTHROPIC_BASE_URL", "https://custom.api.com");

        manager.save_profile(&profile).unwrap();

        let loaded = manager.load_profile("test").unwrap();
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.description, "Test profile");
        assert_eq!(loaded.agent_cli_path, "custom-cli");
        assert_eq!(loaded.theme, "blue");
        assert_eq!(loaded.get_env("ANTHROPIC_API_KEY"), Some(&"sk-test-123".to_string()));
        assert_eq!(loaded.get_env("ANTHROPIC_BASE_URL"), Some(&"https://custom.api.com".to_string()));
    }

    #[test]
    fn test_delete_profile() {
        let (manager, _temp) = test_manager();

        let profile = Profile::new("to_delete");
        manager.save_profile(&profile).unwrap();

        assert!(manager.list_profiles().unwrap().contains(&"to_delete".to_string()));

        manager.delete_profile("to_delete").unwrap();

        assert!(!manager.list_profiles().unwrap().contains(&"to_delete".to_string()));
    }

    #[test]
    fn test_list_profiles() {
        let (manager, _temp) = test_manager();

        manager.save_profile(&Profile::new("alpha")).unwrap();
        manager.save_profile(&Profile::new("beta")).unwrap();
        manager.save_profile(&Profile::new("gamma")).unwrap();

        let profiles = manager.list_profiles().unwrap();
        assert!(profiles.contains(&"alpha".to_string()));
        assert!(profiles.contains(&"beta".to_string()));
        assert!(profiles.contains(&"gamma".to_string()));
        assert!(profiles.contains(&"default".to_string()));
    }

    #[test]
    fn test_app_config_simplified() {
        let (manager, _temp) = test_manager();

        let config = AppConfig {
            current_profile: "custom".to_string(),
            ..Default::default()
        };

        manager.save_app_config(&config).unwrap();

        let loaded = manager.load_app_config().unwrap();
        assert_eq!(loaded.current_profile, "custom");
    }

    #[test]
    fn test_profile_with_api_key() {
        let profile = Profile::with_api_key("work", "sk-work-key");
        assert_eq!(profile.name, "work");
        assert_eq!(profile.agent_cli_path, "aug");
        assert_eq!(profile.theme, "orange");
        assert_eq!(profile.get_env("ANTHROPIC_API_KEY"), Some(&"sk-work-key".to_string()));
    }

    #[test]
    fn test_profile_env_operations() {
        let mut profile = Profile::new("test");

        profile.set_env("KEY1", "value1");
        assert_eq!(profile.get_env("KEY1"), Some(&"value1".to_string()));

        profile.set_env("KEY1", "updated");
        assert_eq!(profile.get_env("KEY1"), Some(&"updated".to_string()));

        let removed = profile.remove_env("KEY1");
        assert_eq!(removed, Some("updated".to_string()));
        assert_eq!(profile.get_env("KEY1"), None);
    }

    #[test]
    fn test_profile_default_settings() {
        let profile = Profile::default();
        assert_eq!(profile.agent_cli_path, "aug");
        assert_eq!(profile.claude_args, Vec::<String>::new());
        assert_eq!(profile.stop_prompt, None);
        assert_eq!(profile.theme, "orange");
    }

    #[test]
    fn test_legacy_profile_deserialization() {
        // Old profile format without new fields should get defaults
        let toml_str = r#"
name = "old-profile"
description = "From old version"

[env]
KEY = "value"
"#;
        let profile: Profile = toml::from_str(toml_str).unwrap();
        assert_eq!(profile.name, "old-profile");
        assert_eq!(profile.agent_cli_path, "aug");
        assert_eq!(profile.theme, "orange");
        assert_eq!(profile.claude_args, Vec::<String>::new());
        assert_eq!(profile.stop_prompt, None);
        assert_eq!(profile.get_env("KEY"), Some(&"value".to_string()));
    }

    #[test]
    fn test_claude_path_alias_deserialization() {
        // Old profile format with claude_path should deserialize via alias
        let toml_str = r#"
name = "alias-test"
claude_path = "custom-claude"
theme = "blue"

[env]
"#;
        let profile: Profile = toml::from_str(toml_str).unwrap();
        assert_eq!(profile.agent_cli_path, "custom-claude");
        assert_eq!(profile.theme, "blue");
    }

    #[test]
    fn test_migration_from_old_config() {
        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().to_path_buf();
        let profiles_dir = config_dir.join("profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        // Write old-format config.toml
        let old_config = r##"
current_profile = "default"
claude_path = "cug"
claude_args = ["--verbose"]
theme = "#ffff00"
"##;
        fs::write(config_dir.join("config.toml"), old_config).unwrap();

        // Write a bare profile (old format)
        let old_profile = r##"
name = "default"
description = "Default profile"

[env]
SOME_KEY = "some_value"
"##;
        fs::write(profiles_dir.join("default.toml"), old_profile).unwrap();

        // Create manager — should trigger migration
        let manager = ProfileManager::with_config_dir(config_dir.clone()).unwrap();

        // Verify config.toml is now simplified
        let config = manager.load_app_config().unwrap();
        assert_eq!(config.current_profile, "default");

        // Verify config.toml no longer has old fields
        let content = fs::read_to_string(config_dir.join("config.toml")).unwrap();
        assert!(!content.contains("claude_path"));
        assert!(!content.contains("claude_args"));
        assert!(!content.contains("theme"));

        // Verify profile now has the migrated settings
        let profile = manager.load_profile("default").unwrap();
        assert_eq!(profile.agent_cli_path, "cug");
        assert_eq!(profile.claude_args, vec!["--verbose".to_string()]);
        assert_eq!(profile.theme, "#ffff00");
        assert_eq!(profile.get_env("SOME_KEY"), Some(&"some_value".to_string()));
    }

    #[test]
    fn test_migration_idempotent() {
        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().to_path_buf();
        let profiles_dir = config_dir.join("profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        // Write old-format config.toml
        let old_config = r##"
current_profile = "default"
claude_path = "cug"
theme = "#ffff00"
"##;
        fs::write(config_dir.join("config.toml"), old_config).unwrap();

        // First migration
        let manager1 = ProfileManager::with_config_dir(config_dir.clone()).unwrap();
        let profile1 = manager1.load_profile("default").unwrap();

        // Second initialization — should not change anything
        let manager2 = ProfileManager::with_config_dir(config_dir).unwrap();
        let profile2 = manager2.load_profile("default").unwrap();

        assert_eq!(profile1.agent_cli_path, profile2.agent_cli_path);
        assert_eq!(profile1.theme, profile2.theme);
    }

    #[test]
    fn test_profile_settings_roundtrip() {
        let (manager, _temp) = test_manager();

        let mut profile = Profile::new("full");
        profile.agent_cli_path = "/usr/local/bin/claude".to_string();
        profile.claude_args = vec!["--dangerously-skip-permissions".to_string(), "--timeout".to_string(), "300".to_string()];
        profile.stop_prompt = Some("Stop if I ask you to review the code.".to_string());
        profile.theme = "green".to_string();
        profile.set_env("API_KEY", "test-key");

        manager.save_profile(&profile).unwrap();

        let loaded = manager.load_profile("full").unwrap();
        assert_eq!(loaded.agent_cli_path, "/usr/local/bin/claude");
        assert_eq!(loaded.claude_args, vec!["--dangerously-skip-permissions", "--timeout", "300"]);
        assert_eq!(loaded.stop_prompt, Some("Stop if I ask you to review the code.".to_string()));
        assert_eq!(loaded.theme, "green");
        assert_eq!(loaded.get_env("API_KEY"), Some(&"test-key".to_string()));
    }
}
