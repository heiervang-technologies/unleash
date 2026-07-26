use std::collections::HashMap;
use std::path::PathBuf;

use super::VersionInfo;

/// Embedded version lists, compiled into the binary for instant display.
/// Updated periodically and committed to the repo.
pub fn get_versions_file_path() -> PathBuf {
    // 1. Check relative to the executable's directory (works regardless of CWD)
    if let Some(exe_local) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("data/versions.json")))
    {
        if exe_local.exists() {
            return exe_local;
        }
    }

    // 2. Fallback to user's config directory
    if let Some(config_dir) = dirs::config_dir() {
        let unleashed_dir = config_dir.join("unleash");
        let _ = std::fs::create_dir_all(&unleashed_dir);
        return unleashed_dir.join("versions.json");
    }

    // 3. Fallback to temp if nothing else works
    std::env::temp_dir().join("unleash-versions.json")
}

/// Load embedded version lists from the dynamically read JSON.
/// Returns a map of agent key -> list of version strings (newest first).
pub fn load_embedded_versions() -> HashMap<String, Vec<String>> {
    let path = get_versions_file_path();
    let content = std::fs::read_to_string(&path).unwrap_or_else(|_| "".to_string());
    let parsed_disk: serde_json::Value = if content.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
    };
    let parsed_fallback: serde_json::Value =
        serde_json::from_str(include_str!("../../data/versions.json")).unwrap_or_default();
    merge_disk_and_fallback(&parsed_disk, &parsed_fallback)
}

/// Inner merge step extracted so the antigravity migration can be exercised
/// from unit tests without touching the real on-disk cache.
pub(super) fn merge_disk_and_fallback(
    parsed_disk: &serde_json::Value,
    parsed_fallback: &serde_json::Value,
) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for key in &[
        "claude",
        "codex",
        "gemini",
        "antigravity",
        "opencode",
        "pi",
        "hermes",
    ] {
        let mut versions: Vec<String> = Vec::new();
        if let Some(arr) = parsed_disk.get(key).and_then(|v| v.as_array()) {
            versions = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
        // Migration: earlier builds shipped antigravity = ["2.0.1"] as the
        // embedded "latest", but that version was never published anywhere
        // (npm 404, no GitHub release). Some refreshed caches kept the bogus
        // value alongside real entries, e.g. ["2.0.1", "1.0.3"], so any
        // occurrence means the cache is contaminated. Fall through to the
        // in-binary fallback so the fixed data/versions.json takes effect.
        if *key == "antigravity" && versions.iter().any(|v| v == "2.0.1") {
            versions.clear();
        }
        if versions.is_empty() {
            if let Some(arr) = parsed_fallback.get(key).and_then(|v| v.as_array()) {
                versions = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
            }
        }
        map.insert(key.to_string(), versions);
    }
    map
}

pub fn save_embedded_versions(map: &HashMap<crate::agents::AgentType, Vec<VersionInfo>>) {
    let mut out_map = serde_json::Map::new();

    for (agent_type, versions) in map {
        let key = match agent_type {
            crate::agents::AgentType::Unleash => continue, // unleash versions not stored here
            crate::agents::AgentType::Claude => "claude",
            crate::agents::AgentType::Codex => "codex",
            crate::agents::AgentType::Clanker => continue,
            crate::agents::AgentType::Antigravity => "antigravity",
            crate::agents::AgentType::Gemini => "gemini",
            crate::agents::AgentType::OpenCode => "opencode",
            crate::agents::AgentType::Pi => "pi",
            crate::agents::AgentType::Hermes => "hermes",
            crate::agents::AgentType::Custom(_) => continue, // skip custom agents in embedded versions
        };
        let arr: Vec<serde_json::Value> = versions
            .iter()
            .map(|v| serde_json::Value::String(v.version.clone()))
            .collect();
        out_map.insert(key.to_string(), serde_json::Value::Array(arr));
    }

    let path = get_versions_file_path();
    if let Ok(json_str) = serde_json::to_string_pretty(&serde_json::Value::Object(out_map)) {
        let _ = crate::config::atomic_write(&path, &json_str);
    }
}
