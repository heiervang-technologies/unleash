//! Profile configuration management
//!
//! Profiles are stored in ~/.config/agent-unleashed/profiles/
//! Each profile is a TOML file with environment variables for agent sessions.
//! Legacy path ~/.config/claude-unleashed/ is also checked for backwards compatibility.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

/// A profile containing environment variables for a Claude session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    /// Display name for the profile
    pub name: String,
    /// Optional description
    #[serde(default)]
    pub description: String,
    /// Environment variables to set when launching Claude
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            description: "Default profile".to_string(),
            env: HashMap::new(),
        }
    }
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            env: HashMap::new(),
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

/// Global app configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// The currently selected profile name
    #[serde(default = "default_profile_name")]
    pub current_profile: String,
    /// Path to claude executable (default: "cug" for full unleashed features)
    #[serde(default = "default_claude_path")]
    pub claude_path: String,
    /// Additional arguments to pass to claude
    #[serde(default)]
    pub claude_args: Vec<String>,
    /// Custom stop-hook prompt for auto-mode (None = use default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_prompt: Option<String>,
    /// Color theme name (e.g., "orange", "blue", "green")
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Mascot preset ID (e.g., "claude", "qwen", "openai", "gemini", "generic")
    #[serde(default = "default_mascot")]
    pub mascot_preset: String,
}

fn default_profile_name() -> String {
    "default".to_string()
}

fn default_theme() -> String {
    "orange".to_string()
}

fn default_mascot() -> String {
    "claude".to_string()
}

fn default_claude_path() -> String {
    // Default to cug (cu go) for full unleashed features:
    // - Auto-patching for auto mode
    // - Restart/resurrection support
    // - Plugin loading
    // - Extended timeouts
    "cug".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            current_profile: default_profile_name(),
            claude_path: default_claude_path(),
            claude_args: Vec::new(),
            stop_prompt: None,
            theme: default_theme(),
            mascot_preset: default_mascot(),
        }
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

        // Create default profile if none exist
        if manager.list_profiles()?.is_empty() {
            manager.save_profile(&Profile::default())?;
        }

        Ok(manager)
    }

    /// Get the default config directory (~/.config/agent-unleashed)
    /// Falls back to legacy path (~/.config/claude-unleashed) if it exists
    pub fn default_config_dir() -> io::Result<PathBuf> {
        let config_base = dirs::config_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find config directory"))?;

        let new_path = config_base.join("agent-unleashed");
        let legacy_path = config_base.join("claude-unleashed");

        // Prefer new path if it exists, otherwise use legacy if it exists, otherwise use new
        if new_path.exists() {
            Ok(new_path)
        } else if legacy_path.exists() {
            Ok(legacy_path)
        } else {
            Ok(new_path) // Default to new path for fresh installs
        }
    }

    /// Get path to a profile file
    fn profile_path(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(format!("{}.toml", name))
    }

    /// Get path to the app config file
    fn app_config_path(&self) -> PathBuf {
        self.config_dir.join("config.toml")
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
        profile.set_env("ANTHROPIC_API_KEY", "sk-test-123");
        profile.set_env("ANTHROPIC_BASE_URL", "https://custom.api.com");

        manager.save_profile(&profile).unwrap();

        let loaded = manager.load_profile("test").unwrap();
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.description, "Test profile");
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
    fn test_app_config() {
        let (manager, _temp) = test_manager();

        let mut config = AppConfig::default();
        config.current_profile = "custom".to_string();
        config.claude_args = vec!["--verbose".to_string()];

        manager.save_app_config(&config).unwrap();

        let loaded = manager.load_app_config().unwrap();
        assert_eq!(loaded.current_profile, "custom");
        assert_eq!(loaded.claude_args, vec!["--verbose".to_string()]);
    }

    #[test]
    fn test_profile_with_api_key() {
        let profile = Profile::with_api_key("work", "sk-work-key");
        assert_eq!(profile.name, "work");
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
}
