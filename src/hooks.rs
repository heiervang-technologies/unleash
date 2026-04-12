//! Hook management for Claude Code
//!
//! unleash acts as the central hook manager for Claude Code.
//! It tracks the Claude installation, manages hooks in ~/.claude/settings.json,
//! and syncs hooks from bundled plugins.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use which::which;

/// Claude Code installation info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeInstallation {
    /// Path to the claude binary
    pub binary_path: PathBuf,
    /// Path to the Claude Code package directory
    pub package_dir: PathBuf,
    /// Installed version
    pub version: String,
    /// Path to user settings (~/.claude/settings.json)
    pub settings_path: PathBuf,
}

impl ClaudeInstallation {
    /// Detect Claude Code installation.
    /// Skips the blocking `claude --version` subprocess call — version is
    /// fetched lazily only when needed (see `get_version()`).
    pub fn detect() -> io::Result<Self> {
        // Find claude binary
        let binary_path = which("claude").map_err(|_| {
            io::Error::new(io::ErrorKind::NotFound, "Claude Code not found in PATH")
        })?;

        // Resolve symlinks to find package directory
        let resolved = fs::canonicalize(&binary_path)?;
        let package_dir = resolved
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not determine package directory",
                )
            })?;

        // Settings path
        let settings_path = dirs::home_dir()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Could not find home directory")
            })?
            .join(".claude/settings.json");

        Ok(Self {
            binary_path,
            package_dir,
            version: String::new(), // deferred — call get_version() when needed
            settings_path,
        })
    }

    /// Get Claude Code version (spawns subprocess — call only when needed)
    #[allow(dead_code)]
    pub fn get_version() -> io::Result<String> {
        let output = Command::new("claude").arg("--version").output()?;

        if output.status.success() {
            let version_str = String::from_utf8_lossy(&output.stdout);
            let version = version_str
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .replace(" (Claude Code)", "");
            Ok(version)
        } else {
            Err(io::Error::other("Failed to get Claude version"))
        }
    }
}

/// Hook event types supported by Claude Code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    Stop,
    PreToolUse,
    PostToolUse,
    PreCompact,
    Notification,
    SessionStart,
    SubagentStart,
    SubagentStop,
    Setup,
    UserPromptSubmit,
    SessionEnd,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::Stop => "Stop",
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::PreCompact => "PreCompact",
            HookEvent::Notification => "Notification",
            HookEvent::SessionStart => "SessionStart",
            HookEvent::SubagentStart => "SubagentStart",
            HookEvent::SubagentStop => "SubagentStop",
            HookEvent::Setup => "Setup",
            HookEvent::UserPromptSubmit => "UserPromptSubmit",
            HookEvent::SessionEnd => "SessionEnd",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Stop" => Some(HookEvent::Stop),
            "PreToolUse" => Some(HookEvent::PreToolUse),
            "PostToolUse" => Some(HookEvent::PostToolUse),
            "PreCompact" => Some(HookEvent::PreCompact),
            "Notification" => Some(HookEvent::Notification),
            "SessionStart" => Some(HookEvent::SessionStart),
            "SubagentStart" => Some(HookEvent::SubagentStart),
            "SubagentStop" => Some(HookEvent::SubagentStop),
            "Setup" => Some(HookEvent::Setup),
            "UserPromptSubmit" => Some(HookEvent::UserPromptSubmit),
            "SessionEnd" => Some(HookEvent::SessionEnd),
            _ => None,
        }
    }
}

/// Manages hooks for Claude Code
pub struct HookManager {
    /// Claude installation info
    pub installation: ClaudeInstallation,
    /// Path to hooks directory
    hooks_dir: PathBuf,
}

impl HookManager {
    /// Create a new HookManager
    pub fn new() -> io::Result<Self> {
        let installation = ClaudeInstallation::detect()?;

        let hooks_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("unleash/hooks");

        fs::create_dir_all(&hooks_dir)?;

        Ok(Self {
            installation,
            hooks_dir,
        })
    }

    /// Get path to a hook script
    pub fn hook_script_path(&self, name: &str) -> PathBuf {
        self.hooks_dir.join(name)
    }

    /// Read current Claude Code settings
    pub fn read_settings(&self) -> io::Result<Value> {
        if self.installation.settings_path.exists() {
            let content = fs::read_to_string(&self.installation.settings_path)?;
            serde_json::from_str(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
        } else {
            Ok(json!({}))
        }
    }

    /// Write Claude Code settings
    pub fn write_settings(&self, settings: &Value) -> io::Result<()> {
        // Ensure ~/.claude directory exists
        if let Some(parent) = self.installation.settings_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(settings)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        fs::write(&self.installation.settings_path, content)
    }

    /// Install a hook script to the hooks directory
    pub fn install_hook_script(&self, name: &str, content: &str) -> io::Result<PathBuf> {
        let path = self.hook_script_path(name);
        fs::write(&path, content)?;

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms)?;
        }

        Ok(path)
    }

    /// Extract script basename from a command for deduplication
    fn command_basename(command: &str) -> String {
        // Handle commands with env vars like "HOOK_EVENT=Stop script.sh"
        let script_part = command.split_whitespace().last().unwrap_or(command);
        // Get basename
        std::path::Path::new(script_part)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| script_part.to_string())
    }

    /// Register a hook in Claude Code settings
    pub fn register_hook(
        &self,
        event: HookEvent,
        command: &str,
        matcher: Option<&str>,
    ) -> io::Result<()> {
        let mut settings = self.read_settings()?;

        // Ensure hooks object exists
        if settings.get("hooks").is_none() {
            settings["hooks"] = json!({});
        }

        let hooks = settings["hooks"]
            .as_object_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "hooks is not an object"))?;

        let event_name = event.as_str();

        // Get or create the event array
        if !hooks.contains_key(event_name) {
            hooks.insert(event_name.to_string(), json!([]));
        }

        let event_hooks = hooks
            .get_mut(event_name)
            .and_then(|v| v.as_array_mut())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "event hooks is not an array")
            })?;

        // Check if this hook already exists (by exact match OR basename match for scripts)
        let new_basename = Self::command_basename(command);
        let mut found_exact = false;
        let mut updated_existing = false;

        for h in event_hooks.iter_mut() {
            if let Some(hooks) = h.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                for hook in hooks.iter_mut() {
                    if let Some(c) = hook
                        .get("command")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string())
                    {
                        if c == command {
                            found_exact = true;
                        } else if Self::command_basename(&c) == new_basename {
                            // Basename matches but path differs — update to new path
                            hook["command"] = json!(command);
                            updated_existing = true;
                        }
                    }
                }
            }
        }

        if !found_exact && !updated_existing {
            let mut hook_config = json!({
                "hooks": [{
                    "type": "command",
                    "command": command
                }]
            });

            if let Some(m) = matcher {
                hook_config["matcher"] = json!(m);
            }

            event_hooks.push(hook_config);
        }

        self.write_settings(&settings)
    }

    /// Unregister a hook by command
    pub fn unregister_hook(&self, event: HookEvent, command: &str) -> io::Result<bool> {
        let mut settings = self.read_settings()?;

        let hooks = match settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
            Some(h) => h,
            None => return Ok(false),
        };

        let event_name = event.as_str();
        let event_hooks = match hooks.get_mut(event_name).and_then(|v| v.as_array_mut()) {
            Some(h) => h,
            None => return Ok(false),
        };

        let initial_len = event_hooks.len();
        event_hooks.retain(|h| {
            !h.get("hooks")
                .and_then(|hooks| hooks.as_array())
                .map(|hooks| {
                    hooks.iter().any(|hook| {
                        hook.get("command")
                            .and_then(|c| c.as_str())
                            .map(|c| c == command)
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        });

        let removed = event_hooks.len() < initial_len;

        if removed {
            self.write_settings(&settings)?;
        }

        Ok(removed)
    }

    /// List all registered hooks
    pub fn list_hooks(&self) -> io::Result<HashMap<String, Vec<String>>> {
        let settings = self.read_settings()?;
        let mut result = HashMap::new();

        if let Some(hooks) = settings.get("hooks").and_then(|h| h.as_object()) {
            for (event, event_hooks) in hooks {
                if let Some(arr) = event_hooks.as_array() {
                    let commands: Vec<String> = arr
                        .iter()
                        .flat_map(|h| {
                            h.get("hooks")
                                .and_then(|hooks| hooks.as_array())
                                .map(|hooks| {
                                    hooks
                                        .iter()
                                        .filter_map(|hook| {
                                            hook.get("command")
                                                .and_then(|c| c.as_str())
                                                .map(|s| s.to_string())
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        })
                        .collect();
                    result.insert(event.clone(), commands);
                }
            }
        }

        Ok(result)
    }

    /// Install default hooks
    pub fn install_default_hooks(&self) -> io::Result<()> {
        // Install PreCompact hook
        let compact_script = r#"#!/usr/bin/env bash
# compact-notify.sh - Notify Claude that compaction is complete
#
# This hook runs after conversation compaction and returns a message
# to help Claude understand what happened.

set -euo pipefail

# Output format for Claude Code hooks
cat <<'EOF'
{
  "continue": true,
  "message": "COMPACT COMPLETE. Previous context has been summarized. Continue with your current task."
}
EOF
"#;

        let script_path = self.install_hook_script("compact-notify.sh", compact_script)?;
        self.register_hook(HookEvent::PreCompact, script_path.to_str().unwrap(), None)?;

        println!("Installed default hooks:");
        println!("  - PreCompact: {}", script_path.display());

        Ok(())
    }

    /// Sync hooks from bundled plugins
    pub fn sync_plugin_hooks(&self, plugin_dirs: &[PathBuf]) -> io::Result<()> {
        for plugin_dir in plugin_dirs {
            let hooks_json = plugin_dir.join("hooks/hooks.json");
            if hooks_json.exists() {
                self.sync_plugin_hook_file(&hooks_json, plugin_dir)?;
            }
        }
        Ok(())
    }

    /// Sync hooks from a single plugin's hooks.json
    fn sync_plugin_hook_file(&self, hooks_json: &PathBuf, plugin_dir: &Path) -> io::Result<()> {
        let content = fs::read_to_string(hooks_json)?;
        let hooks: Value = serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Handle new format with "hooks" wrapper
        let hooks_obj = if let Some(obj) = hooks.get("hooks").and_then(|h| h.as_object()) {
            obj
        } else if let Some(obj) = hooks.as_object() {
            obj
        } else {
            return Ok(());
        };

        for (event_name, event_config) in hooks_obj {
            if let Some(event) = HookEvent::from_str(event_name) {
                // Handle array format
                if let Some(arr) = event_config.as_array() {
                    for config in arr {
                        self.process_hook_config(event, config, plugin_dir)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Process a single hook configuration
    fn process_hook_config(
        &self,
        event: HookEvent,
        config: &Value,
        plugin_dir: &Path,
    ) -> io::Result<()> {
        if let Some(hooks) = config.get("hooks").and_then(|h| h.as_array()) {
            for hook in hooks {
                if let Some(command) = hook.get("command").and_then(|c| c.as_str()) {
                    // Expand ${CLAUDE_PLUGIN_ROOT}
                    let expanded_command =
                        command.replace("${CLAUDE_PLUGIN_ROOT}", plugin_dir.to_str().unwrap_or(""));
                    let matcher = config.get("matcher").and_then(|m| m.as_str());
                    self.register_hook(event, &expanded_command, matcher)?;
                }
            }
        }

        Ok(())
    }

    /// Get summary of installation
    pub fn summary(&self) -> String {
        format!(
            "Claude Code Installation:\n  Binary: {}\n  Package: {}\n  Version: {}\n  Settings: {}",
            self.installation.binary_path.display(),
            self.installation.package_dir.display(),
            self.installation.version,
            self.installation.settings_path.display()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── Test helper ─────────────────────────────────────────

    /// Create a HookManager backed by a temp directory (no real Claude needed)
    fn test_manager(tmp: &Path) -> HookManager {
        let settings_path = tmp.join("settings.json");
        let hooks_dir = tmp.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        // Write empty settings
        fs::write(&settings_path, "{}").unwrap();

        HookManager {
            installation: ClaudeInstallation {
                binary_path: PathBuf::from("/usr/bin/claude"),
                package_dir: PathBuf::from("/usr/lib/claude"),
                version: "test".to_string(),
                settings_path,
            },
            hooks_dir,
        }
    }

    // ── HookEvent roundtrip ──────────────────────────────────

    const ALL_EVENTS: &[HookEvent] = &[
        HookEvent::Stop,
        HookEvent::PreToolUse,
        HookEvent::PostToolUse,
        HookEvent::PreCompact,
        HookEvent::Notification,
        HookEvent::SessionStart,
        HookEvent::SubagentStart,
        HookEvent::SubagentStop,
        HookEvent::Setup,
        HookEvent::UserPromptSubmit,
        HookEvent::SessionEnd,
    ];

    #[test]
    fn test_hook_event_roundtrip_all_variants() {
        for &event in ALL_EVENTS {
            let s = event.as_str();
            let parsed = HookEvent::from_str(s);
            assert_eq!(parsed, Some(event), "roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_hook_event_from_str_invalid() {
        assert_eq!(HookEvent::from_str("Invalid"), None);
        assert_eq!(HookEvent::from_str(""), None);
        assert_eq!(HookEvent::from_str("stop"), None); // case-sensitive
        assert_eq!(HookEvent::from_str("STOP"), None);
    }

    // ── command_basename ─────────────────────────────────────

    #[test]
    fn test_command_basename_simple_script() {
        assert_eq!(HookManager::command_basename("my-hook.sh"), "my-hook.sh");
    }

    #[test]
    fn test_command_basename_absolute_path() {
        assert_eq!(
            HookManager::command_basename("/usr/local/bin/hook.sh"),
            "hook.sh"
        );
    }

    #[test]
    fn test_command_basename_with_env_prefix() {
        assert_eq!(
            HookManager::command_basename("HOOK_EVENT=Stop /path/to/script.sh"),
            "script.sh"
        );
    }

    #[test]
    fn test_command_basename_with_args() {
        // Takes the last whitespace-separated token
        assert_eq!(
            HookManager::command_basename("env FOO=bar /opt/hooks/run.sh"),
            "run.sh"
        );
    }

    #[test]
    fn test_command_basename_no_path_separator() {
        assert_eq!(HookManager::command_basename("hook"), "hook");
    }

    #[test]
    fn test_command_basename_plugin_root_expansion() {
        assert_eq!(
            HookManager::command_basename("${CLAUDE_PLUGIN_ROOT}/hooks-handlers/auto-stop.sh"),
            "auto-stop.sh"
        );
    }

    // ── register_hook ───────────────────────────────────────

    #[test]
    fn test_register_hook_creates_event_array() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        mgr.register_hook(HookEvent::PreCompact, "/path/to/hook.sh", None)
            .unwrap();

        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(
            hooks[0]["hooks"][0]["command"].as_str().unwrap(),
            "/path/to/hook.sh"
        );
    }

    #[test]
    fn test_register_hook_deduplicates_exact_match() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        mgr.register_hook(HookEvent::PreCompact, "/path/to/hook.sh", None)
            .unwrap();
        mgr.register_hook(HookEvent::PreCompact, "/path/to/hook.sh", None)
            .unwrap();

        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(hooks.len(), 1, "duplicate should not be added");
    }

    #[test]
    fn test_register_hook_updates_path_on_basename_match() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        mgr.register_hook(HookEvent::PreCompact, "/old/path/hook.sh", None)
            .unwrap();
        mgr.register_hook(HookEvent::PreCompact, "/new/path/hook.sh", None)
            .unwrap();

        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(hooks.len(), 1, "basename match should update, not add");
        assert_eq!(
            hooks[0]["hooks"][0]["command"].as_str().unwrap(),
            "/new/path/hook.sh"
        );
    }

    #[test]
    fn test_register_hook_allows_different_basenames() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        mgr.register_hook(HookEvent::PreCompact, "/path/hook-a.sh", None)
            .unwrap();
        mgr.register_hook(HookEvent::PreCompact, "/path/hook-b.sh", None)
            .unwrap();

        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(hooks.len(), 2, "different basenames should both be kept");
    }

    #[test]
    fn test_register_hook_with_matcher() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        mgr.register_hook(HookEvent::PreToolUse, "/path/hook.sh", Some("Bash"))
            .unwrap();

        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(hooks[0]["matcher"].as_str().unwrap(), "Bash");
    }

    // ── unregister_hook ─────────────────────────────────────

    #[test]
    fn test_unregister_hook_removes_matching() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        mgr.register_hook(HookEvent::PreCompact, "/path/hook.sh", None)
            .unwrap();
        let removed = mgr
            .unregister_hook(HookEvent::PreCompact, "/path/hook.sh")
            .unwrap();

        assert!(removed);
        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(hooks.len(), 0);
    }

    #[test]
    fn test_unregister_hook_returns_false_for_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        let removed = mgr
            .unregister_hook(HookEvent::PreCompact, "/path/hook.sh")
            .unwrap();
        assert!(!removed);
    }

    // ── list_hooks ──────────────────────────────────────────

    #[test]
    fn test_list_hooks_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        let hooks = mgr.list_hooks().unwrap();
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_list_hooks_returns_registered() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        mgr.register_hook(HookEvent::PreCompact, "/path/hook.sh", None)
            .unwrap();
        mgr.register_hook(HookEvent::Stop, "/path/stop.sh", None)
            .unwrap();

        let hooks = mgr.list_hooks().unwrap();
        assert_eq!(hooks.len(), 2);
        assert!(hooks.contains_key("PreCompact"));
        assert!(hooks.contains_key("Stop"));
    }

    // ── sync_plugin_hooks ───────────────────────────────────

    #[test]
    fn test_sync_plugin_hooks_from_hooks_json() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        // Create a fake plugin directory with hooks.json
        let plugin_dir = tmp.path().join("plugins/my-plugin");
        fs::create_dir_all(plugin_dir.join("hooks")).unwrap();
        fs::write(
            plugin_dir.join("hooks/hooks.json"),
            r#"{
                "hooks": {
                    "PreCompact": [{
                        "hooks": [{
                            "type": "command",
                            "command": "${CLAUDE_PLUGIN_ROOT}/handlers/compact.sh"
                        }]
                    }],
                    "UserPromptSubmit": [{
                        "hooks": [{
                            "type": "command",
                            "command": "${CLAUDE_PLUGIN_ROOT}/handlers/userprompt.sh"
                        }]
                    }]
                }
            }"#,
        )
        .unwrap();

        mgr.sync_plugin_hooks(&[plugin_dir.clone()]).unwrap();

        let settings = mgr.read_settings().unwrap();

        // Check PreCompact hook was registered with expanded path
        let pc_hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(pc_hooks.len(), 1);
        let cmd = pc_hooks[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(
            cmd.contains("/plugins/my-plugin/handlers/compact.sh"),
            "CLAUDE_PLUGIN_ROOT should be expanded: {cmd}"
        );
        assert!(
            !cmd.contains("${CLAUDE_PLUGIN_ROOT}"),
            "variable should be resolved: {cmd}"
        );

        // Check UserPromptSubmit hook was also registered
        let ups_hooks = settings["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(ups_hooks.len(), 1);
    }

    #[test]
    fn test_sync_plugin_hooks_skips_missing_hooks_json() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        // Plugin dir exists but has no hooks/hooks.json
        let plugin_dir = tmp.path().join("plugins/no-hooks");
        fs::create_dir_all(&plugin_dir).unwrap();

        mgr.sync_plugin_hooks(&[plugin_dir]).unwrap();

        let hooks = mgr.list_hooks().unwrap();
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_sync_plugin_hooks_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        let plugin_dir = tmp.path().join("plugins/my-plugin");
        fs::create_dir_all(plugin_dir.join("hooks")).unwrap();
        fs::write(
            plugin_dir.join("hooks/hooks.json"),
            r#"{
                "hooks": {
                    "PreCompact": [{
                        "hooks": [{
                            "type": "command",
                            "command": "${CLAUDE_PLUGIN_ROOT}/hook.sh"
                        }]
                    }]
                }
            }"#,
        )
        .unwrap();

        // Sync twice
        mgr.sync_plugin_hooks(&[plugin_dir.clone()]).unwrap();
        mgr.sync_plugin_hooks(&[plugin_dir]).unwrap();

        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(hooks.len(), 1, "second sync should not duplicate hooks");
    }

    #[test]
    fn test_sync_updates_path_when_plugin_dir_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        // First sync from old location
        let old_dir = tmp.path().join("plugins-old/my-plugin");
        fs::create_dir_all(old_dir.join("hooks")).unwrap();
        fs::write(
            old_dir.join("hooks/hooks.json"),
            r#"{"hooks":{"PreCompact":[{"hooks":[{"type":"command","command":"${CLAUDE_PLUGIN_ROOT}/hook.sh"}]}]}}"#,
        )
        .unwrap();
        mgr.sync_plugin_hooks(&[old_dir]).unwrap();

        // Second sync from new location (same basename hook.sh)
        let new_dir = tmp.path().join("plugins-new/my-plugin");
        fs::create_dir_all(new_dir.join("hooks")).unwrap();
        fs::write(
            new_dir.join("hooks/hooks.json"),
            r#"{"hooks":{"PreCompact":[{"hooks":[{"type":"command","command":"${CLAUDE_PLUGIN_ROOT}/hook.sh"}]}]}}"#,
        )
        .unwrap();
        mgr.sync_plugin_hooks(&[new_dir.clone()]).unwrap();

        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(hooks.len(), 1, "should update path, not add second entry");
        let cmd = hooks[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(
            cmd.contains("plugins-new"),
            "path should be updated to new location: {cmd}"
        );
    }

    #[test]
    fn test_sync_preserves_existing_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        // Register a manual hook first
        mgr.register_hook(HookEvent::PreCompact, "/manual/compact-notify.sh", None)
            .unwrap();

        // Then sync a plugin hook with a different basename
        let plugin_dir = tmp.path().join("plugins/supercompact");
        fs::create_dir_all(plugin_dir.join("hooks")).unwrap();
        fs::write(
            plugin_dir.join("hooks/hooks.json"),
            r#"{"hooks":{"PreCompact":[{"hooks":[{"type":"command","command":"${CLAUDE_PLUGIN_ROOT}/supercompact-precompact.sh"}]}]}}"#,
        )
        .unwrap();
        mgr.sync_plugin_hooks(&[plugin_dir]).unwrap();

        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(hooks.len(), 2, "plugin hook should be added alongside manual hook");
    }

    #[test]
    fn test_sync_into_empty_event_array() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        // Write settings with an empty PreCompact array (the bug scenario)
        fs::write(
            &mgr.installation.settings_path,
            r#"{"hooks":{"PreCompact":[]}}"#,
        )
        .unwrap();

        let plugin_dir = tmp.path().join("plugins/supercompact");
        fs::create_dir_all(plugin_dir.join("hooks")).unwrap();
        fs::write(
            plugin_dir.join("hooks/hooks.json"),
            r#"{"hooks":{"PreCompact":[{"hooks":[{"type":"command","command":"${CLAUDE_PLUGIN_ROOT}/hook.sh"}]}]}}"#,
        )
        .unwrap();
        mgr.sync_plugin_hooks(&[plugin_dir]).unwrap();

        let settings = mgr.read_settings().unwrap();
        let hooks = settings["hooks"]["PreCompact"].as_array().unwrap();
        assert_eq!(
            hooks.len(),
            1,
            "sync should populate an empty event array"
        );
    }

    #[test]
    fn test_sync_multiple_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = test_manager(tmp.path());

        // Plugin A: PreCompact hook
        let plugin_a = tmp.path().join("plugins/plugin-a");
        fs::create_dir_all(plugin_a.join("hooks")).unwrap();
        fs::write(
            plugin_a.join("hooks/hooks.json"),
            r#"{"hooks":{"PreCompact":[{"hooks":[{"type":"command","command":"${CLAUDE_PLUGIN_ROOT}/a-hook.sh"}]}]}}"#,
        )
        .unwrap();

        // Plugin B: UserPromptSubmit hook
        let plugin_b = tmp.path().join("plugins/plugin-b");
        fs::create_dir_all(plugin_b.join("hooks")).unwrap();
        fs::write(
            plugin_b.join("hooks/hooks.json"),
            r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"${CLAUDE_PLUGIN_ROOT}/b-hook.sh"}]}]}}"#,
        )
        .unwrap();

        mgr.sync_plugin_hooks(&[plugin_a, plugin_b]).unwrap();

        let hooks = mgr.list_hooks().unwrap();
        assert!(hooks.contains_key("PreCompact"));
        assert!(hooks.contains_key("UserPromptSubmit"));
    }
}
