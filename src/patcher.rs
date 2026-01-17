//! Claude Code patcher for auto mode
//!
//! Applies patches to enable auto mode in Claude Code.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use which::which;

/// Cache file to track last patched version
fn version_cache_file() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("claude-unleashed/last-patched-version")
}

/// Get Claude Code installation directory
fn get_claude_dir() -> io::Result<PathBuf> {
    // Try to find via npm
    let output = Command::new("npm")
        .args(["root", "-g"])
        .output()?;

    if output.status.success() {
        let npm_root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let claude_dir = PathBuf::from(npm_root).join("@anthropic-ai/claude-code");
        if claude_dir.exists() {
            return Ok(claude_dir);
        }
    }

    // Try to find via which
    if let Ok(claude_path) = which("claude") {
        // Resolve symlinks
        let resolved = fs::canonicalize(&claude_path)?;
        // Go up from bin/claude to package root
        if let Some(parent) = resolved.parent().and_then(|p| p.parent()) {
            let claude_dir = parent.join("lib/node_modules/@anthropic-ai/claude-code");
            if claude_dir.exists() {
                return Ok(claude_dir);
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Could not find Claude Code installation",
    ))
}

/// Get installed Claude Code version
fn get_claude_version() -> io::Result<String> {
    let output = Command::new("claude")
        .arg("--version")
        .output()?;

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
        Err(io::Error::new(io::ErrorKind::Other, "Failed to get Claude version"))
    }
}

/// Find patches directory
fn get_patches_dir() -> io::Result<PathBuf> {
    // Try relative to exe
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let patches = dir.join("patches/versions");
            if patches.exists() {
                return Ok(patches);
            }
        }
    }

    // Try ~/.local/bin/patches
    if let Some(home) = dirs::home_dir() {
        let patches = home.join(".local/bin/patches/versions");
        if patches.exists() {
            return Ok(patches);
        }
    }

    // Try repo location (development)
    let repo_patches = PathBuf::from("scripts/patches/versions");
    if repo_patches.exists() {
        return Ok(repo_patches);
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Could not find patches directory",
    ))
}

/// Load patch configuration for a version
fn load_patch_config(version: &str) -> io::Result<HashMap<String, String>> {
    let patches_dir = get_patches_dir()?;
    let config_file = patches_dir.join(format!("{}.conf", version));

    if !config_file.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("No patch config for version {}", version),
        ));
    }

    let content = fs::read_to_string(&config_file)?;
    let mut config = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Parse KEY="value" or KEY=value
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            config.insert(key.to_string(), value.to_string());
        }
    }

    Ok(config)
}

/// Check if patching is needed and apply if so
pub fn check_and_patch() -> io::Result<()> {
    let version = match get_claude_version() {
        Ok(v) => v,
        Err(_) => return Ok(()), // Claude not installed, nothing to patch
    };

    // Check cached version
    if let Ok(cached) = fs::read_to_string(version_cache_file()) {
        if cached.trim() == version {
            // Already patched this version
            return Ok(());
        }
    }

    // Check if patch config exists
    if get_patches_dir().is_ok() {
        let patches_dir = get_patches_dir()?;
        let config_file = patches_dir.join(format!("{}.conf", version));
        if config_file.exists() {
            return patch();
        }
    }

    Ok(())
}

/// Apply the patch
pub fn patch() -> io::Result<()> {
    let claude_dir = get_claude_dir()?;
    let version = get_claude_version()?;

    println!("Found Claude Code at: {}", claude_dir.display());
    println!("Detected version: {}", version);

    // Load patch config
    let config = match load_patch_config(&version) {
        Ok(c) => {
            println!("Using patch config: {}.conf", version);
            c
        }
        Err(e) => {
            println!("No patch config for version {}: {}", version, e);
            return Ok(());
        }
    };

    // Get variable names from config
    let modes_var = config.get("MODES_ARRAY_VAR").map(|s| s.as_str()).unwrap_or("QP");
    let mode_var = config.get("MODE_VAR").map(|s| s.as_str()).unwrap_or("S0");

    // Read cli.js
    let cli_js = claude_dir.join("cli.js");
    if !cli_js.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "cli.js not found",
        ));
    }

    println!("Patching: {}", cli_js.display());

    let content = fs::read_to_string(&cli_js)?;

    // Check if already patched
    if content.contains("\"auto\"") {
        println!("Already patched (auto mode exists in modes array)");
        // Update cache
        fs::create_dir_all(version_cache_file().parent().unwrap())?;
        fs::write(version_cache_file(), &version)?;
        return Ok(());
    }

    // Apply patches
    let mut patched = content.clone();

    // Patch 1: Add "auto" to modes array
    // Pattern: const QP=["plan","code",...]
    let modes_pattern = format!(r#"const {}=["plan","code""#, modes_var);
    if patched.contains(&modes_pattern) {
        patched = patched.replace(
            &modes_pattern,
            &format!(r#"const {}=["auto","plan","code""#, modes_var),
        );
        println!("  + Added 'auto' to modes array");
    } else {
        println!("  ! Could not find modes array pattern");
    }

    // Patch 2: Auto-select mode when CLAUDE_AUTO_MODE is set
    // Pattern: const S0=MODES[...
    let mode_pattern = format!("const {}={}[", mode_var, modes_var);
    if patched.contains(&mode_pattern) {
        let new_mode_init = format!(
            r#"const {}=process.env.CLAUDE_AUTO_MODE==="1"?"auto":{}["#,
            mode_var, modes_var
        );
        patched = patched.replace(&mode_pattern, &new_mode_init);
        println!("  + Added auto mode selection from env");
    }

    // Write patched file
    fs::write(&cli_js, &patched)?;

    // Update cache
    fs::create_dir_all(version_cache_file().parent().unwrap())?;
    fs::write(version_cache_file(), &version)?;

    println!("Patch applied successfully!");

    Ok(())
}

/// Show current version
#[allow(dead_code)]
pub fn show_current() -> io::Result<()> {
    match get_claude_version() {
        Ok(v) => println!("Claude Code version: {}", v),
        Err(_) => println!("Claude Code is not installed"),
    }
    Ok(())
}
