//! Hyprland window manager detection and integration
//!
//! Provides detection of Hyprland environment, window rule management,
//! workspace assignment, and notification support via `hyprctl`.

use std::env;
use std::io;
use std::process::Command;

/// Check if the current session is running under Hyprland
pub fn is_hyprland() -> bool {
    env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
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

/// Gather information about the running Hyprland instance.
/// Returns `None` if not running under Hyprland.
#[allow(dead_code)]
pub fn get_info() -> Option<HyprlandInfo> {
    let instance_signature = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;

    let version = Command::new("hyprctl")
        .args(["version", "-j"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|json| {
            serde_json::from_str::<serde_json::Value>(&json)
                .ok()
                .and_then(|v| v["tag"].as_str().map(|s| s.to_string()))
        });

    Some(HyprlandInfo {
        instance_signature,
        version,
    })
}

// --- Window rules ---

/// Set a window rule via `hyprctl keyword windowrule`.
/// Uses the Hyprland 0.53+ syntax: `<rule>, match:<match_type> <pattern>`
pub fn set_window_rule(rule: &str) -> io::Result<()> {
    let output = Command::new("hyprctl")
        .args(["keyword", "windowrule", rule])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("hyprctl keyword windowrule failed: {}", stderr),
        ));
    }
    Ok(())
}

/// Apply default window rules for agent-unleashed windows.
/// Matches windows with title containing "agent-unleashed".
pub fn apply_agent_window_rules() -> io::Result<()> {
    // Float agent windows by default
    set_window_rule("float on, match:title agent-unleashed")?;
    // Slight transparency for visual distinction
    set_window_rule("opacity 0.95 0.90, match:title agent-unleashed")?;
    Ok(())
}

/// Assign a window to a specific workspace by title match.
#[allow(dead_code)]
pub fn set_workspace_rule(title_match: &str, workspace: u32) -> io::Result<()> {
    set_window_rule(&format!("workspace {}, match:title {}", workspace, title_match))
}

// --- Notifications ---

/// Send a desktop notification via `hyprctl notify`.
///
/// * `urgency` - 0 = info/hint, 1 = warning, 2 = error, 3 = confused (question)
/// * `timeout_ms` - Duration in milliseconds (0 = default)
/// * `color` - RGBA color as `0xAARRGGBB`, or 0 for default
/// * `message` - The notification text
pub fn notify(urgency: u8, timeout_ms: u32, color: u64, message: &str) -> io::Result<()> {
    let color_str = if color == 0 {
        "0".to_string()
    } else {
        format!("0x{:08x}", color)
    };

    let output = Command::new("hyprctl")
        .args([
            "notify",
            &urgency.to_string(),
            &timeout_ms.to_string(),
            &color_str,
            message,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("hyprctl notify failed: {}", stderr),
        ));
    }
    Ok(())
}

/// Send an info notification (green tint, 5 second default)
pub fn notify_info(message: &str) -> io::Result<()> {
    notify(0, 5000, 0, message)
}

/// Send a warning notification (yellow tint, 8 second default)
pub fn notify_warning(message: &str) -> io::Result<()> {
    notify(1, 8000, 0, message)
}

/// Send an error notification (red tint, 10 second default)
#[allow(dead_code)]
pub fn notify_error(message: &str) -> io::Result<()> {
    notify(2, 10000, 0, message)
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
}
