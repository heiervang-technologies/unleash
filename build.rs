//! Build script to generate version whitelist and blacklist from Cargo.toml metadata

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Read Cargo.toml
    let manifest = fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");

    // Parse Claude Code whitelist and blacklist
    let whitelist = parse_version_list(&manifest, "claude-code-whitelist");
    let blacklist = parse_version_list(&manifest, "claude-code-blacklist");
    let default_mode = parse_default_mode(&manifest, "claude-code-versions");

    // Parse Codex whitelist and blacklist
    let codex_whitelist = parse_version_list(&manifest, "codex-whitelist");
    let codex_blacklist = parse_version_list(&manifest, "codex-blacklist");
    let codex_default_mode = parse_default_mode(&manifest, "codex-versions");

    // Generate the output file
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = Path::new(&out_dir).join("version_lists.rs");

    let code = format!(
        r#"/// Official Claude Code whitelist from Cargo.toml (verified working versions)
pub const DEFAULT_WHITELIST: &[&str] = &[{}];

/// Official Claude Code blacklist from Cargo.toml (versions with known issues)
pub const DEFAULT_BLACKLIST: &[&str] = &[{}];

/// Default Claude Code version filter mode from Cargo.toml
pub const DEFAULT_VERSION_FILTER_MODE: &str = "{}";

/// Official Codex whitelist from Cargo.toml (verified working versions)
pub const DEFAULT_CODEX_WHITELIST: &[&str] = &[{}];

/// Official Codex blacklist from Cargo.toml (versions with known issues)
pub const DEFAULT_CODEX_BLACKLIST: &[&str] = &[{}];

/// Default Codex version filter mode from Cargo.toml
pub const DEFAULT_CODEX_VERSION_FILTER_MODE: &str = "{}";
"#,
        format_version_array(&whitelist),
        format_version_array(&blacklist),
        default_mode,
        format_version_array(&codex_whitelist),
        format_version_array(&codex_blacklist),
        codex_default_mode
    );

    fs::write(&dest_path, code).expect("Failed to write version_lists.rs");

    // Rerun if Cargo.toml changes
    println!("cargo:rerun-if-changed=Cargo.toml");
}

fn format_version_array(versions: &[String]) -> String {
    versions
        .iter()
        .map(|v| format!("\"{}\"", v))
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_version_list(manifest: &str, section_name: &str) -> Vec<String> {
    let section_header = format!("[package.metadata.{}]", section_name);
    let mut in_section = false;
    let mut versions = Vec::new();

    for line in manifest.lines() {
        let trimmed = line.trim();

        if trimmed == section_header {
            in_section = true;
            continue;
        }

        // Exit section on new section header
        if in_section && trimmed.starts_with('[') {
            break;
        }

        if in_section && trimmed.starts_with("versions") {
            if let Some(array_start) = trimmed.find('[') {
                if let Some(array_end) = trimmed.find(']') {
                    let array_content = &trimmed[array_start + 1..array_end];
                    for item in array_content.split(',') {
                        let version = item.trim().trim_matches('"').trim_matches('\'');
                        if !version.is_empty() {
                            versions.push(version.to_string());
                        }
                    }
                }
            }
        }
    }

    versions
}

fn parse_default_mode(manifest: &str, section_name: &str) -> String {
    let section_header = format!("[package.metadata.{}]", section_name);
    let mut in_section = false;

    for line in manifest.lines() {
        let trimmed = line.trim();

        if trimmed == section_header {
            in_section = true;
            continue;
        }

        // Exit section on new section header
        if in_section && trimmed.starts_with('[') {
            break;
        }

        if in_section && trimmed.starts_with("default_mode") {
            // Parse default_mode = "whitelist" or default_mode = "blacklist"
            if let Some(eq_pos) = trimmed.find('=') {
                let value = trimmed[eq_pos + 1..].trim().trim_matches('"').trim_matches('\'');
                return value.to_string();
            }
        }
    }

    // Default to whitelist mode if not specified
    "whitelist".to_string()
}
