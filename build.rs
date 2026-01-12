//! Build script to generate version blacklist from Cargo.toml metadata

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Read Cargo.toml
    let manifest = fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");

    // Parse the blacklist section manually (avoid adding toml as build dependency)
    let blacklist = parse_blacklist(&manifest);

    // Generate the output file
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = Path::new(&out_dir).join("blacklist.rs");

    let code = format!(
        r#"/// Official blacklist from Cargo.toml
pub const DEFAULT_BLACKLIST: &[&str] = &[{}];
"#,
        blacklist
            .iter()
            .map(|v| format!("\"{}\"", v))
            .collect::<Vec<_>>()
            .join(", ")
    );

    fs::write(&dest_path, code).expect("Failed to write blacklist.rs");

    // Rerun if Cargo.toml changes
    println!("cargo:rerun-if-changed=Cargo.toml");
}

fn parse_blacklist(manifest: &str) -> Vec<String> {
    // Look for versions = ["x.y.z", ...] in the blacklist section
    let mut in_blacklist_section = false;
    let mut versions = Vec::new();

    for line in manifest.lines() {
        let trimmed = line.trim();

        if trimmed == "[package.metadata.claude-code-blacklist]" {
            in_blacklist_section = true;
            continue;
        }

        // Exit section on new section header
        if in_blacklist_section && trimmed.starts_with('[') {
            break;
        }

        if in_blacklist_section && trimmed.starts_with("versions") {
            // Parse versions = ["2.1.5", "2.1.1", "2.1.0"]
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
