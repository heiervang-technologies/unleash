//! Hyprland window manager detection and integration
//!
//! Provides detection of Hyprland environment, window rule management,
//! workspace assignment, and notification support via `hyprctl`.

use crate::config::ProfileManager;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

/// Check if the current session is running under Hyprland
pub fn is_hyprland() -> bool {
    env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
}

/// Check if Hyprland focus integration is enabled.
/// Returns true only when ALL of:
/// 1. Running under Hyprland
/// 2. The hyprland-focus plugin is enabled in AppConfig (TUI toggle)
/// 3. AU_HYPRLAND_FOCUS env var is not "0"
pub fn is_focus_enabled() -> bool {
    if !is_hyprland() {
        return false;
    }
    if env::var("AU_HYPRLAND_FOCUS").ok().as_deref() == Some("0") {
        return false;
    }
    // Check plugin toggle from TUI config
    let config = ProfileManager::new()
        .and_then(|m| m.load_app_config())
        .unwrap_or_default();
    if config.enabled_plugins.is_empty() {
        return true; // empty = all enabled
    }
    config
        .enabled_plugins
        .contains(&"hyprland-focus".to_string())
}

/// Information about the running Hyprland instance
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HyprlandInfo {
    /// The instance signature (from HYPRLAND_INSTANCE_SIGNATURE)
    pub instance_signature: String,
    /// Hyprland version string (from `hyprctl version -j`)
    pub version: Option<String>,
}

/// Run a hyprctl command and return its stdout
#[allow(dead_code)]
pub fn hyprctl(args: &[&str]) -> io::Result<String> {
    let output = Command::new("hyprctl").args(args).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!(
            "hyprctl {} failed: {}",
            args.join(" "),
            stderr
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a hyprctl command with `-j` flag and parse JSON output
#[allow(dead_code)]
pub fn hyprctl_json(args: &[&str]) -> io::Result<serde_json::Value> {
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.push("-j");
    let stdout = hyprctl(&full_args)?;
    serde_json::from_str(&stdout).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("JSON parse error: {}", e),
        )
    })
}

/// Gather information about the running Hyprland instance.
/// Returns `None` if not running under Hyprland.
#[allow(dead_code)]
pub fn get_info() -> Option<HyprlandInfo> {
    let instance_signature = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;

    let version = hyprctl_json(&["version"])
        .ok()
        .and_then(|v| v["tag"].as_str().map(|s| s.to_string()));

    Some(HyprlandInfo {
        instance_signature,
        version,
    })
}

// --- Window rules ---

/// Set a window rule via `hyprctl keyword windowrule`.
/// Uses the Hyprland 0.53+ syntax: `<rule>, match:<match_type> <pattern>`
pub fn set_window_rule(rule: &str) -> io::Result<()> {
    hyprctl(&["keyword", "windowrule", rule])?;
    Ok(())
}

/// Apply default window rules for unleash windows using batch mode.
/// Matches windows with class `unleash` using 0.53+ regex syntax.
/// Non-blocking — fires and forgets the hyprctl call.
pub fn apply_agent_window_rules() -> io::Result<()> {
    let _ = Command::new("hyprctl")
        .args([
            "--batch",
            "keyword windowrule float on, match:class ^(unleash)$ ; \
             keyword windowrule opacity 0.95 0.9, match:class ^(unleash)$",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    Ok(())
}

/// Assign a window to a specific workspace by class match.
#[allow(dead_code)]
pub fn set_workspace_rule(class_match: &str, workspace: u32) -> io::Result<()> {
    set_window_rule(&format!(
        "workspace {}, match:class ^({})$",
        workspace, class_match
    ))
}

// --- Notifications ---

/// Notification icon types matching hyprctl notify API
#[allow(dead_code)]
pub mod icon {
    pub const WARNING: u8 = 0;
    pub const INFO: u8 = 1;
    pub const HINT: u8 = 2;
    pub const ERROR: u8 = 3;
    pub const OK: u8 = 5;
}

/// Send a desktop notification via `hyprctl notify`.
///
/// * `icon_type` - 0=Warning, 1=Info, 2=Hint, 3=Error, 5=Ok
/// * `timeout_ms` - Duration in milliseconds (0 = default)
/// * `color` - Color string like `"rgb(ff9500)"`, or `"0"` for default
/// * `message` - The notification text
pub fn notify(icon_type: u8, timeout_ms: u32, color: &str, message: &str) -> io::Result<()> {
    hyprctl(&[
        "notify",
        &icon_type.to_string(),
        &timeout_ms.to_string(),
        color,
        message,
    ])?;
    Ok(())
}

/// Send an info notification (5 second default)
pub fn notify_info(message: &str) -> io::Result<()> {
    notify(icon::INFO, 5000, "0", message)
}

/// Send a warning notification (8 second default)
pub fn notify_warning(message: &str) -> io::Result<()> {
    notify(icon::WARNING, 8000, "0", message)
}

/// Send an error notification (10 second default)
#[allow(dead_code)]
pub fn notify_error(message: &str) -> io::Result<()> {
    notify(icon::ERROR, 10000, "0", message)
}

/// Send an ok/success notification (5 second default)
#[allow(dead_code)]
pub fn notify_ok(message: &str) -> io::Result<()> {
    notify(icon::OK, 5000, "0", message)
}

// --- Focus (window opacity) ---

/// Find the hypr-window-opacity.sh script path.
/// Checks repo path first (development), then installed path.
fn focus_script_path() -> Option<PathBuf> {
    let repo_path = PathBuf::from("plugins/bundled/hyprland-focus/scripts/hypr-window-opacity.sh");
    if repo_path.exists() {
        return fs::canonicalize(&repo_path).ok();
    }
    dirs::data_local_dir()
        .map(|d| d.join("unleash/plugins/hyprland-focus/scripts/hypr-window-opacity.sh"))
        .filter(|p| p.exists())
}

/// Set window to transparent (agent is working).
/// Calls the hyprland-focus plugin's opacity script.
pub fn focus_set(wrapper_pid: u32) -> io::Result<()> {
    if !is_focus_enabled() {
        return Ok(());
    }
    let script = match focus_script_path() {
        Some(p) => p,
        None => {
            eprintln!("[unleash] focus: hypr-window-opacity.sh not found, skipping");
            return Ok(());
        }
    };
    // Fire-and-forget: don't block on the opacity script
    let _ = Command::new(&script)
        .arg("set")
        .env("AGENT_WRAPPER_PID", wrapper_pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    Ok(())
}

/// Reset window to opaque (agent is idle/stopped). Non-blocking.
pub fn focus_reset(wrapper_pid: u32) -> io::Result<()> {
    if !is_focus_enabled() {
        return Ok(());
    }
    let script = match focus_script_path() {
        Some(p) => p,
        None => return Ok(()),
    };
    // Fire-and-forget: don't block on the opacity script
    let _ = Command::new(&script)
        .arg("reset")
        .env("AGENT_WRAPPER_PID", wrapper_pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    Ok(())
}

/// Play the idle sound (agent stopped). Non-blocking.
pub fn play_idle_sound() {
    if !is_focus_enabled() {
        return;
    }
    let sound_file = focus_script_path()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .map(|scripts_dir| scripts_dir.join("../sounds/idle.wav"))
        .and_then(|p| fs::canonicalize(&p).ok())
        .filter(|p| p.exists());

    if let Some(sound) = sound_file {
        // Try pw-play first, fall back to paplay, then play
        for player in &["pw-play", "paplay", "play"] {
            if which::which(player).is_ok() {
                let _ = Command::new(player).arg(&sound).spawn();
                return;
            }
        }
    }
}

/// Clean up cached focus state file on exit.
pub fn focus_cleanup(wrapper_pid: u32) {
    let state_file = PathBuf::from(format!("/tmp/unleash-hyprfocus/{}", wrapper_pid));
    let _ = fs::remove_file(state_file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hyprland_detection() {
        // Save and clear the env var for a clean test
        let original = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");

        assert!(!is_hyprland());

        env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "test_instance_abc123");
        assert!(is_hyprland());

        // Restore original value
        match original {
            Some(val) => env::set_var("HYPRLAND_INSTANCE_SIGNATURE", val),
            None => env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
    }

    #[test]
    fn test_get_info_without_hyprland() {
        let original = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");

        assert!(get_info().is_none());

        // Restore
        if let Some(val) = original {
            env::set_var("HYPRLAND_INSTANCE_SIGNATURE", val);
        }
    }

    #[test]
    fn test_get_info_with_signature() {
        let original = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "test_sig_12345");

        let info = get_info();
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.instance_signature, "test_sig_12345");
        // version may be None if hyprctl isn't available in test env

        // Restore
        match original {
            Some(val) => env::set_var("HYPRLAND_INSTANCE_SIGNATURE", val),
            None => env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
    }

    #[test]
    fn test_icon_constants() {
        assert_eq!(icon::WARNING, 0);
        assert_eq!(icon::INFO, 1);
        assert_eq!(icon::HINT, 2);
        assert_eq!(icon::ERROR, 3);
        assert_eq!(icon::OK, 5);
    }
}
