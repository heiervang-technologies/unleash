use std::io;
use std::sync::mpsc;

use crate::json_output::{self, VersionListItem, VersionListOutput, VersionOutput};
use super::{InstallResult, VersionManager};

pub fn list_versions(json: bool) -> io::Result<()> {
    let vm = VersionManager::new();
    let versions = vm.get_version_list();
    let current = vm.get_installed_version();

    if json {
        let output = VersionListOutput {
            currently_installed: current,
            versions: versions
                .into_iter()
                .map(|info| VersionListItem {
                    version: info.version,
                    is_installed: info.is_installed,
                })
                .collect(),
        };
        json_output::print_json(&output);
    } else {
        println!("Claude Code Versions:");
        println!();

        for info in versions {
            let installed = if info.is_installed {
                " [installed]"
            } else {
                ""
            };
            println!("  v{}{}", info.version, installed);
        }

        if let Some(v) = current {
            println!();
            println!("Currently installed: v{}", v);
        }
    }

    Ok(())
}

/// Install a specific version
pub fn install_version(version: &str, json: bool) -> io::Result<()> {
    let vm = VersionManager::new();

    if !json {
        println!("Installing Claude Code v{}...", version);
    }

    let install_result = vm.install_version(version)?;
    if !install_result.success {
        if !json {
            eprintln!(
                "Install failed: {}",
                install_result.error.unwrap_or_default()
            );
            if !install_result.stderr.is_empty() {
                eprintln!("{}", install_result.stderr);
            }
        }
        return Err(io::Error::other(format!(
            "Failed to install Claude Code {}",
            version
        )));
    }

    if !json {
        println!("Done!");
    } else {
        json_output::print_success_json(&format!(
            "Successfully installed Claude Code v{}",
            version
        ));
    }

    Ok(())
}

/// Show current version
#[allow(dead_code)]
pub fn show_current() -> io::Result<()> {
    let vm = VersionManager::new();
    match vm.get_installed_version() {
        Some(v) => println!("Claude Code version: {}", v),
        None => println!("Claude Code is not installed"),
    }
    Ok(())
}

/// Show current version as JSON
pub fn show_current_json() {
    let cu_version = env!("CARGO_PKG_VERSION");
    let vm = VersionManager::new();
    let installed = vm.get_installed_version();
    let is_installed = installed.is_some();
    let claude_code_version = installed.unwrap_or_else(|| "not installed".to_string());

    let output = VersionOutput {
        unleash_version: cu_version.to_string(),
        claude_code_version,
        claude_code_installed: is_installed,
    };

    json_output::print_json(&output);
}

/// Install the latest available version of `agent`, streaming log lines to `log_tx`.
/// Returns `(resolved_version, InstallResult)`.
pub fn install_latest_streaming(
    agent: crate::agents::AgentType,
    log_tx: mpsc::Sender<String>,
) -> io::Result<(String, InstallResult)> {
    use crate::agents::AgentType;
    let vm = VersionManager::new();
    match agent {
        AgentType::Claude => {
            let versions = vm.get_version_list();
            let v = versions
                .into_iter()
                .filter(|i| !i.version.contains('-')) // Filter out pre-releases
                .find(|i| !i.is_installed)
                .or_else(|| {
                    vm.get_version_list()
                        .into_iter()
                        .find(|i| !i.version.contains('-'))
                })
                .map(|i| i.version)
                .ok_or_else(|| io::Error::other("no Claude version available"))?;
            let r = vm.install_version_streaming(&v, log_tx)?;
            Ok((v, r))
        }
        AgentType::Codex => {
            let installed = which::which("codex").ok().and(None::<String>); // just need presence
            let versions = vm.get_codex_version_list(installed.as_deref());
            let v = versions
                .into_iter()
                .next()
                .map(|i| i.version)
                .ok_or_else(|| io::Error::other("no Codex version available"))?;
            let r = vm.install_codex_version_streaming(&v, log_tx)?;
            Ok((v, r))
        }
        AgentType::Clanker => {
            let _ = log_tx
                .send("Building Clanker Code from the fork-owned release branch...".to_string());
            let mut manager = crate::agents::AgentManager::new()?;
            match manager.update_agent(AgentType::Clanker) {
                Ok(message) => {
                    let revision = crate::clanker::installed_revision()
                        .unwrap_or_else(|| "latest".to_string());
                    let version = crate::clanker::revision_label(&revision).to_string();
                    let _ = log_tx.send(message.clone());
                    Ok((
                        version,
                        InstallResult {
                            success: true,
                            stdout: message,
                            stderr: String::new(),
                            error: None,
                        },
                    ))
                }
                Err(err) => Ok((
                    "latest".to_string(),
                    InstallResult {
                        success: false,
                        stdout: String::new(),
                        stderr: err.to_string(),
                        error: Some(err.to_string()),
                    },
                )),
            }
        }
        AgentType::Gemini => {
            let versions = vm.get_gemini_version_list(None);
            let v = versions
                .into_iter()
                .find(|i| !i.version.contains('-'))
                .map(|i| i.version)
                .ok_or_else(|| io::Error::other("no Gemini version available"))?;
            let r = vm.install_gemini_version_streaming(&v, log_tx)?;
            Ok((v, r))
        }
        AgentType::Antigravity => {
            let versions = vm.get_antigravity_version_list(None);
            let v = versions
                .into_iter()
                .next()
                .map(|i| i.version)
                .ok_or_else(|| io::Error::other("no Antigravity version available"))?;
            let r = vm.install_antigravity_version_streaming(&v, log_tx)?;
            Ok((v, r))
        }
        AgentType::OpenCode => {
            let versions = vm.get_opencode_version_list(None);
            let v = versions
                .into_iter()
                .next()
                .map(|i| i.version)
                .ok_or_else(|| io::Error::other("no OpenCode version available"))?;
            let r = vm.install_opencode_version_streaming(&v, log_tx)?;
            Ok((v, r))
        }
        AgentType::Pi => {
            let versions = vm.get_pi_version_list(None);
            let v = versions
                .into_iter()
                .find(|i| !i.version.contains('-'))
                .map(|i| i.version)
                .ok_or_else(|| io::Error::other("no Pi version available"))?;
            let r = vm.install_pi_version_streaming(&v, log_tx)?;
            Ok((v, r))
        }
        AgentType::Hermes => {
            let versions = vm.get_hermes_version_list(None);
            let v = versions
                .into_iter()
                .next()
                .map(|i| i.version)
                .ok_or_else(|| io::Error::other("no Hermes version available"))?;
            let r = vm.install_hermes_version_streaming(&v, log_tx)?;
            Ok((v, r))
        }
        AgentType::Unleash | AgentType::Custom(_) => Err(io::Error::other(format!(
            "{} cannot be installed via the wizard",
            agent.display_name()
        ))),
    }
}

