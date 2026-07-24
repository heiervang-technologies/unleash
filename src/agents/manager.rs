use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

use super::install::{
    atomic_install_binary, detect_arch_os, install_extracted_binary, pick_asset_name,
};
use super::{AgentDefinition, AgentType};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentVersion {
    /// Current installed version
    pub installed: Option<String>,
    /// Latest available version
    pub latest: Option<String>,
    /// Binary path
    pub binary_path: Option<PathBuf>,
    /// Last checked timestamp
    pub last_checked: Option<u64>,
}

/// Agent manager for handling multiple code agents
pub struct AgentManager {
    /// Agent definitions
    agents: HashMap<AgentType, AgentDefinition>,
    /// Version cache
    versions: HashMap<AgentType, AgentVersion>,
    /// Config directory
    config_dir: PathBuf,
}

impl AgentManager {
    /// Create a new AgentManager
    pub fn new() -> io::Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Config directory not found"))?
            .join("unleash");

        fs::create_dir_all(&config_dir)?;

        let mut manager = Self {
            agents: HashMap::new(),
            versions: HashMap::new(),
            config_dir,
        };

        // Register default agents
        manager.register_agent(AgentDefinition::claude());
        manager.register_agent(AgentDefinition::codex());
        manager.register_agent(AgentDefinition::clanker());
        manager.register_agent(AgentDefinition::gemini());
        manager.register_agent(AgentDefinition::antigravity());
        manager.register_agent(AgentDefinition::opencode());
        manager.register_agent(AgentDefinition::pi());
        manager.register_agent(AgentDefinition::hermes());

        // Register user-defined custom agents from the AppConfig. Without this,
        // any `unleash agents <cmd> <custom-name>` invocation hits "Agent not
        // found" before reaching the explicit "not yet supported" branch.
        // Failure to read the config is non-fatal (e.g. first-time install) —
        // built-ins keep working.
        if let Ok(mgr) = crate::config::ProfileManager::new() {
            if let Ok(app_config) = mgr.load_app_config() {
                for custom in &app_config.custom_agents {
                    if !custom.enabled || AgentType::from_str(&custom.name).is_some() {
                        continue;
                    }
                    manager.register_agent(AgentDefinition::from_custom_config(custom));
                }
            }
        }

        // Load cached versions
        manager.load_version_cache()?;

        Ok(manager)
    }

    /// Constructor variant for tests: takes pre-built custom agent definitions
    /// instead of reading from disk. Lets unit tests exercise the custom-agent
    /// surface (status, list, check, info) without env-var fiddling.
    #[cfg(test)]
    pub fn new_with_custom_for_tests(custom: Vec<AgentDefinition>) -> io::Result<Self> {
        let tmp = tempfile::tempdir()?;
        let mut manager = Self {
            agents: HashMap::new(),
            versions: HashMap::new(),
            config_dir: tmp.path().to_path_buf(),
        };
        manager.register_agent(AgentDefinition::claude());
        manager.register_agent(AgentDefinition::codex());
        manager.register_agent(AgentDefinition::clanker());
        manager.register_agent(AgentDefinition::gemini());
        manager.register_agent(AgentDefinition::antigravity());
        manager.register_agent(AgentDefinition::opencode());
        manager.register_agent(AgentDefinition::pi());
        manager.register_agent(AgentDefinition::hermes());
        for c in custom {
            manager.register_agent(c);
        }
        // Leak the tempdir so the config_dir path stays valid for the
        // lifetime of the manager. Tests are short-lived; this is acceptable
        // here even though it would be a leak in production code.
        std::mem::forget(tmp);
        Ok(manager)
    }

    /// Register an agent definition
    pub fn register_agent(&mut self, agent: AgentDefinition) {
        self.agents.insert(agent.agent_type.clone(), agent);
    }

    /// Get an agent definition
    pub fn get_agent(&self, agent_type: AgentType) -> Option<&AgentDefinition> {
        self.agents.get(&agent_type)
    }

    /// List all registered agents
    pub fn list_agents(&self) -> Vec<&AgentDefinition> {
        let mut agents = Vec::with_capacity(self.agents.len());
        for agent_type in AgentType::builtin() {
            if let Some(agent) = self.agents.get(agent_type) {
                agents.push(agent);
            }
        }
        let mut custom: Vec<_> = self
            .agents
            .values()
            .filter(|agent| matches!(agent.agent_type, AgentType::Custom(_)))
            .collect();
        custom.sort_by(|left, right| left.name.cmp(&right.name));
        agents.extend(custom);
        agents
    }

    /// Resolve a user-supplied name to an AgentType.
    /// Tries the built-in alias table first (`AgentType::from_str`), then
    /// falls back to a `Custom(name)` lookup against agents registered from
    /// the user's `[[custom_agents]]` config. Returns None when no match.
    pub fn resolve_agent_type(&self, name: &str) -> Option<AgentType> {
        if let Some(t) = AgentType::from_str(name) {
            return Some(t);
        }
        let custom = AgentType::Custom(name.to_string());
        if self.agents.contains_key(&custom) {
            Some(custom)
        } else {
            None
        }
    }

    fn parse_asar_version(content: &[u8]) -> Option<String> {
        let pattern1 = b"\"name\": \"antigravity\"";
        let pattern2 = b"\"name\":\"antigravity\"";
        let pos = content
            .windows(pattern1.len())
            .position(|w| w == pattern1)
            .or_else(|| content.windows(pattern2.len()).position(|w| w == pattern2))?;

        let search_slice = &content[pos..pos + std::cmp::min(1000, content.len() - pos)];
        let version_pattern = b"\"version\"";
        let v_pos = search_slice
            .windows(version_pattern.len())
            .position(|w| w == version_pattern)?;

        let val_slice = &search_slice[v_pos + version_pattern.len()..];

        let mut start_idx = None;
        let mut colon_found = false;
        for (i, &b) in val_slice.iter().enumerate() {
            if b == b':' {
                colon_found = true;
            } else if b == b'"' && colon_found {
                start_idx = Some(i + 1);
                break;
            }
        }

        let start = start_idx?;
        let end_slice = &val_slice[start..];
        let end = end_slice.iter().position(|&b| b == b'"')?;

        String::from_utf8(end_slice[..end].to_vec()).ok()
    }

    /// Get installed version for an agent
    pub fn get_installed_version(&mut self, agent_type: AgentType) -> io::Result<Option<String>> {
        let agent = self
            .agents
            .get(&agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Agent not found"))?;

        if agent_type == AgentType::Clanker {
            let version = crate::clanker::installed_product_version();
            let entry = self.versions.entry(agent_type).or_default();
            entry.installed = version.clone();
            entry.binary_path = version
                .as_ref()
                .and_then(|_| dirs::home_dir().map(|home| home.join(".local/bin/clanker")));
            return Ok(version);
        }

        if agent_type == AgentType::Antigravity {
            // Prefer the CLI binary version. The desktop Antigravity app and
            // the `agy` companion CLI use different version lines, and this
            // manager is reporting the CLI.
            let mut version = None;
            if let Ok(bin_path) = which::which(&agent.binary) {
                if let Ok(output) = Command::new(&bin_path).arg("--version").output() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    version = Self::parse_version(&stdout)
                        .or_else(|| Self::parse_version(&stderr))
                        .or_else(|| stdout.lines().next().map(|s| s.trim().to_string()))
                        .or_else(|| stderr.lines().next().map(|s| s.trim().to_string()));
                }
            }

            // Fallback to Electron app.asar paths only when no `agy` binary is
            // available. This can identify a desktop-only install, but should
            // not override the actual CLI version.
            let paths = [
                PathBuf::from("/opt/Antigravity/resources/app.asar"), // Arch Linux / pacman default
                PathBuf::from("/Applications/Antigravity.app/Contents/Resources/app.asar"), // macOS default
            ];
            if version.is_none() {
                for path in &paths {
                    if path.exists() {
                        if let Ok(content) = fs::read(path) {
                            if let Some(v) = Self::parse_asar_version(&content) {
                                version = Some(v);
                                break;
                            }
                        }
                    }
                }
            }

            // Update cache
            let entry = self.versions.entry(agent_type).or_default();
            entry.installed = version.clone();
            entry.binary_path = which::which(&agent.binary).ok();

            return Ok(version);
        }

        // Try to get version from binary
        let binary = agent.binary.clone();
        let output = Command::new(&binary).arg("--version").output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout_str = String::from_utf8_lossy(&out.stdout);
                let mut version = Self::parse_version(&stdout_str);

                // Some agents (e.g. pi) write --version to stderr.
                if version.is_none() {
                    let stderr_str = String::from_utf8_lossy(&out.stderr);
                    version = Self::parse_version(&stderr_str);
                }

                // Codex reports "0.0.0" from source builds — fall back to git tag
                if agent_type == AgentType::Codex && version.as_deref() == Some("0.0.0") {
                    version = Self::codex_version_from_git_tag();
                }

                // Hermes reports both a SemVer ("v0.13.0") and a CalVer date
                // ("2026.5.7") on the same line. Upstream tags releases by
                // CalVer, so the GH "latest" comparison only works against the
                // CalVer — extract it from the parenthesized suffix.
                if agent_type == AgentType::Hermes {
                    let stdout_str = String::from_utf8_lossy(&out.stdout);
                    let stderr_str = String::from_utf8_lossy(&out.stderr);
                    if let Some(v) = Self::parse_hermes_calver(&stdout_str)
                        .or_else(|| Self::parse_hermes_calver(&stderr_str))
                    {
                        version = Some(v);
                    }
                }

                // Update cache
                let entry = self.versions.entry(agent_type).or_default();
                entry.installed = version.clone();
                entry.binary_path = which::which(&binary).ok();

                Ok(version)
            }
            _ => Ok(None),
        }
    }

    /// Get codex version from git tag in the cached source repo.
    /// Codex uses workspace version "0.0.0" so --version is useless;
    /// the real version comes from git tags like "rust-v0.116.0".
    fn codex_version_from_git_tag() -> Option<String> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("unleash/codex-source");

        if !cache_dir.join(".git").exists() {
            return None;
        }

        let output = Command::new("git")
            .args(["describe", "--tags", "--abbrev=0"])
            .current_dir(&cache_dir)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Tags are like "rust-v0.116.0" — strip "rust-v" prefix
        Some(
            tag.trim_start_matches("rust-v")
                .trim_start_matches('v')
                .to_string(),
        )
    }

    /// Get a GitHub token for API auth (needed for private repos).
    fn github_token() -> Option<String> {
        if let Ok(token) = std::env::var("GH_TOKEN") {
            return Some(token);
        }
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            return Some(token);
        }
        Command::new("gh")
            .args(["auth", "token"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Parse version string from command output
    pub(super) fn parse_version(output: &str) -> Option<String> {
        // Handle various version formats:
        // "claude 2.1.22" -> "2.1.22"
        // "codex 0.1.0" -> "0.1.0"
        // "v1.2.3" -> "1.2.3"
        let line = output.lines().next()?;
        let parts: Vec<&str> = line.split_whitespace().collect();

        for part in parts {
            let cleaned = part.trim_start_matches('v').trim_end_matches(')');
            if cleaned
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
            {
                return Some(cleaned.to_string());
            }
        }

        None
    }

    /// Pull the CalVer date out of `hermes --version` output. The format is
    /// "Hermes Agent v<semver> (<calver>)" on the first line. We need the
    /// CalVer to match upstream's GitHub release tags.
    pub(super) fn parse_hermes_calver(output: &str) -> Option<String> {
        let line = output.lines().next()?;
        let start = line.rfind('(')?;
        let end = line.rfind(')')?;
        if end <= start + 1 {
            return None;
        }
        let inner = line[start + 1..end].trim();
        if inner.chars().next()?.is_ascii_digit() {
            Some(inner.to_string())
        } else {
            None
        }
    }

    /// Get latest version from GitHub
    pub fn get_latest_version(&mut self, agent_type: AgentType) -> io::Result<Option<String>> {
        if agent_type == AgentType::Clanker {
            let revision = crate::clanker::latest_revision()?;
            let version = crate::clanker::revision_label(&revision).to_string();
            let entry = self.versions.entry(agent_type).or_default();
            // Cache the full revision for equality checks. The abbreviated
            // label is presentation-only and cannot be revision authority.
            entry.latest = Some(revision);
            entry.last_checked = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );
            return Ok(Some(version));
        }

        let agent = self
            .agents
            .get(&agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Agent not found"))?;

        let repo = match &agent.github_repo {
            Some(r) => r.clone(),
            None => return Ok(None),
        };

        // Use GitHub API to get latest release
        let url = format!("https://api.github.com/repos/{}/releases/latest", repo);

        let mut cmd = Command::new("curl");
        cmd.args(["-s", "-H", "Accept: application/vnd.github.v3+json"]);
        // Add auth for private repos
        if let Some(token) = Self::github_token() {
            cmd.arg("-H").arg(format!("Authorization: token {}", token));
        }
        let output = cmd.arg(&url).output()?;

        if !output.status.success() {
            return Ok(None);
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let tag = json.get("tag_name").and_then(|t| t.as_str()).map(|s| {
            // Handle tags like "rust-v0.116.0" (Codex) and "v1.2.3" (others)
            s.trim_start_matches("rust-v")
                .trim_start_matches('v')
                .to_string()
        });

        // Update cache
        if let Some(ref version) = tag {
            let entry = self.versions.entry(agent_type).or_default();
            entry.latest = Some(version.clone());
            entry.last_checked = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );
        }

        Ok(tag)
    }

    /// Check if an update is available
    pub fn check_update(&mut self, agent_type: AgentType) -> io::Result<bool> {
        if agent_type == AgentType::Clanker {
            let latest = crate::clanker::latest_revision()?;
            let installed = self.get_installed_version(AgentType::Clanker)?;
            return Ok(installed.is_none() || crate::clanker::update_available(&latest));
        }

        let installed = self.get_installed_version(agent_type.clone())?;
        let latest = self.get_latest_version(agent_type)?;

        match (installed, latest) {
            (Some(i), Some(l)) => Ok(crate::version::version_less_than(&i, &l)),
            _ => Ok(false),
        }
    }

    /// Update an agent to latest version
    pub fn update_agent(&mut self, agent_type: AgentType) -> io::Result<String> {
        // Validate agent exists
        self.agents
            .get(&agent_type)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Agent not found"))?;

        match agent_type {
            AgentType::Unleash => Err(io::Error::other(
                "Use `unleash update` to update unleash itself",
            )),
            AgentType::Claude => self.update_claude(),
            AgentType::Codex => self.update_codex(),
            AgentType::Clanker => self.update_clanker(),
            AgentType::Antigravity => self.update_antigravity(),
            AgentType::Gemini => self.update_npm_agent("@google/gemini-cli", "Gemini CLI"),
            AgentType::OpenCode => self.update_opencode(),
            AgentType::Pi => self.update_npm_agent("@mariozechner/pi-coding-agent", "Pi"),
            AgentType::Hermes => self.update_hermes(),
            AgentType::Custom(name) => self.update_custom(&name),
        }
    }

    /// Update a custom agent. Implements the Shape B (convention) + Shape A
    /// (asset_template escape hatch) install path agreed on issue #338.
    ///
    /// 1. Loads the custom agent's config from AppConfig.
    /// 2. Fetches the latest GitHub release for the configured `github_repo`.
    /// 3. Picks an asset name — either by substituting placeholders in
    ///    `asset_template` (Shape A) or by walking a small set of
    ///    `<name>-<arch>-<os>` conventions (Shape B).
    /// 4. Downloads, extracts (if `.tar.gz` / `.zip`), and installs the binary
    ///    to `~/.local/bin/<binary>`.
    fn update_custom(&self, name: &str) -> io::Result<String> {
        let mgr = crate::config::ProfileManager::new()?;
        self.update_custom_with_manager(name, &mgr)
    }

    /// Testable inner of `update_custom` — takes an explicit ProfileManager
    /// so the config-resolution + github_repo-missing branches are exercisable
    /// against a tempdir without touching the user's real `~/.config/unleash`.
    pub(super) fn update_custom_with_manager(
        &self,
        name: &str,
        mgr: &crate::config::ProfileManager,
    ) -> io::Result<String> {
        let app_config = mgr.load_app_config()?;
        let cfg = app_config
            .custom_agents
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| {
                io::Error::other(format!(
                    "Custom agent '{}' is not registered. Run `unleash agents add {}` first.",
                    name, name
                ))
            })?;

        let repo = cfg.github_repo.as_deref().ok_or_else(|| {
            io::Error::other(format!(
                "Custom agent '{}' has no github_repo set. Add one to ~/.config/unleash/config.toml \
                 under `[[custom_agents]]` so the updater knows where to fetch releases.",
                name
            ))
        })?;

        let (arch, os) = detect_arch_os().ok_or_else(|| {
            io::Error::other(
                "Unsupported platform: custom-agent install only supports Linux/macOS on x86_64/aarch64.",
            )
        })?;

        // Fetch latest release
        let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
        let curl_args: Vec<String> = vec![
            "-s".into(),
            "-H".into(),
            "Accept: application/vnd.github.v3+json".into(),
            url,
        ];
        let resp = Command::new("curl")
            .args(&curl_args)
            .output()
            .map_err(|e| io::Error::other(format!("curl failed: {}", e)))?;
        if !resp.status.success() {
            return Err(io::Error::other(format!(
                "Failed to fetch release from GitHub ({}): {}",
                repo,
                String::from_utf8_lossy(&resp.stderr)
            )));
        }
        let json: serde_json::Value = serde_json::from_slice(&resp.stdout)
            .map_err(|e| io::Error::other(format!("Malformed GitHub response: {}", e)))?;

        let tag = json
            .get("tag_name")
            .and_then(|t| t.as_str())
            .ok_or_else(|| io::Error::other("No tag_name in GitHub release response"))?;
        let version = tag.trim_start_matches("rust-v").trim_start_matches('v');

        let asset_names: Vec<String> = json
            .get("assets")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let picked = pick_asset_name(
            name,
            cfg.asset_template.as_deref(),
            arch,
            os,
            version,
            tag,
            &asset_names,
        )
        .ok_or_else(|| {
            io::Error::other(format!(
                "Could not find a matching asset in {}'s release {}. \
                 Available assets: [{}]. \
                 Set `asset_template = \"...\"` in the [[custom_agents]] block to override.",
                repo,
                tag,
                asset_names.join(", ")
            ))
        })?;

        let download_url = format!(
            "https://github.com/{}/releases/download/{}/{}",
            repo, tag, picked
        );

        // Download
        let tmp_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(format!("unleash/custom-download-{}", cfg.name));
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir)?;
        let tmp_archive = tmp_dir.join(&picked);
        let dl = Command::new("curl")
            .args(["-fsSL", "-o", &tmp_archive.to_string_lossy(), &download_url])
            .output()?;
        if !dl.status.success() {
            return Err(io::Error::other(format!(
                "Download failed ({}): {}",
                download_url,
                String::from_utf8_lossy(&dl.stderr)
            )));
        }

        // Install path
        let install_dir = dirs::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?
            .join(".local/bin");
        fs::create_dir_all(&install_dir)?;
        let install_path = install_dir.join(&cfg.binary);

        // Extract or copy
        if picked.ends_with(".tar.gz") || picked.ends_with(".tgz") {
            let extract = Command::new("tar")
                .args([
                    "xzf",
                    &tmp_archive.to_string_lossy(),
                    "-C",
                    &tmp_dir.to_string_lossy(),
                ])
                .output()?;
            if !extract.status.success() {
                return Err(io::Error::other(format!(
                    "tar extraction failed: {}",
                    String::from_utf8_lossy(&extract.stderr)
                )));
            }
            install_extracted_binary(&tmp_dir, &cfg.binary, &install_path)?;
        } else if picked.ends_with(".zip") {
            let extract = Command::new("unzip")
                .args([
                    "-o",
                    &tmp_archive.to_string_lossy(),
                    "-d",
                    &tmp_dir.to_string_lossy(),
                ])
                .output()?;
            if !extract.status.success() {
                return Err(io::Error::other(format!(
                    "unzip extraction failed: {}",
                    String::from_utf8_lossy(&extract.stderr)
                )));
            }
            install_extracted_binary(&tmp_dir, &cfg.binary, &install_path)?;
        } else {
            // Plain binary
            atomic_install_binary(&tmp_archive, &install_path)?;
        }

        // Both branches install atomically and already chmod +x the binary
        // before it becomes visible at `install_path`.

        let _ = fs::remove_dir_all(&tmp_dir);

        Ok(format!(
            "Installed {} {} to {}",
            cfg.name,
            version,
            install_path.display()
        ))
    }

    /// Update Claude Code via npm
    fn update_claude(&self) -> io::Result<String> {
        let output = crate::version::VersionManager::npm_global_command()
            .args(["install", "-g", "@anthropic-ai/claude-code@latest"])
            .output()?;

        if output.status.success() {
            Ok("Claude Code updated successfully".to_string())
        } else {
            Err(io::Error::other(format!(
                "Failed to update Claude Code: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    /// Update Codex — prefer prebuilt binary, fall back to source build
    fn update_codex(&self) -> io::Result<String> {
        let install_path = dirs::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?
            .join(".local/bin/codex");
        fs::create_dir_all(install_path.parent().unwrap())?;

        // Try prebuilt binary first
        match Self::install_codex_binary(&install_path) {
            Ok(msg) => return Ok(msg),
            Err(e) => {
                eprintln!(
                    "Prebuilt binary install failed ({}), falling back to source build...",
                    e
                );
            }
        }

        // Fallback: build from source (requires cargo)
        if which::which("cargo").is_err() {
            return Err(io::Error::other(
                "No prebuilt Codex binary for this platform and cargo is not installed. \
                 Install Rust (rustup.rs) or download Codex manually from https://github.com/openai/codex/releases"
            ));
        }

        Self::build_codex_from_source(&install_path)
    }

    fn update_clanker(&self) -> io::Result<String> {
        let outcome = crate::clanker::install_latest()?;
        Ok(format!(
            "Clanker Code {} ({}) installed to {}",
            outcome.product_version,
            crate::clanker::revision_label(&outcome.revision),
            outcome.install_path.display()
        ))
    }

    /// Download and install prebuilt Codex binary from GitHub releases
    fn install_codex_binary(install_path: &std::path::Path) -> io::Result<String> {
        // Detect platform triple
        let triple = Self::detect_platform_triple()
            .ok_or_else(|| io::Error::other("Unsupported platform for prebuilt binary"))?;

        let asset_name = format!("codex-{}.tar.gz", triple);

        // Get latest release tag
        let tag_output = Command::new("curl")
            .args([
                "-s",
                "-H",
                "Accept: application/vnd.github.v3+json",
                "https://api.github.com/repos/openai/codex/releases/latest",
            ])
            .output()?;

        let json: serde_json::Value = serde_json::from_slice(&tag_output.stdout)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let tag = json
            .get("tag_name")
            .and_then(|t| t.as_str())
            .ok_or_else(|| io::Error::other("Could not determine latest Codex release tag"))?;

        let version = tag.trim_start_matches("rust-v").trim_start_matches('v');

        // Check if asset exists in this release
        let has_asset = json
            .get("assets")
            .and_then(|a| a.as_array())
            .map(|assets| {
                assets
                    .iter()
                    .any(|a| a.get("name").and_then(|n| n.as_str()) == Some(&asset_name))
            })
            .unwrap_or(false);

        if !has_asset {
            return Err(io::Error::other(format!(
                "No prebuilt binary '{}' found in release {}",
                asset_name, tag
            )));
        }

        let download_url = format!(
            "https://github.com/openai/codex/releases/download/{}/{}",
            tag, asset_name
        );

        // Download to temp file
        let tmp_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("unleash/codex-download");
        fs::create_dir_all(&tmp_dir)?;
        let tmp_archive = tmp_dir.join(&asset_name);

        let dl_output = Command::new("curl")
            .args(["-fsSL", "-o", &tmp_archive.to_string_lossy(), &download_url])
            .output()?;

        if !dl_output.status.success() {
            return Err(io::Error::other(format!(
                "Download failed: {}",
                String::from_utf8_lossy(&dl_output.stderr)
            )));
        }

        // Extract — codex binary is at the root of the tar.gz
        let extract_output = Command::new("tar")
            .args([
                "xzf",
                &tmp_archive.to_string_lossy(),
                "-C",
                &tmp_dir.to_string_lossy(),
            ])
            .output()?;

        if !extract_output.status.success() {
            return Err(io::Error::other(format!(
                "Extraction failed: {}",
                String::from_utf8_lossy(&extract_output.stderr)
            )));
        }

        // Find the codex binary — named codex-<triple> inside the archive
        let extracted_binary = tmp_dir.join(format!("codex-{}", triple));
        let extracted_fallback = tmp_dir.join("codex");
        let binary_path = if extracted_binary.exists() {
            &extracted_binary
        } else if extracted_fallback.exists() {
            &extracted_fallback
        } else {
            return Err(io::Error::other(format!(
                "Extracted archive does not contain 'codex-{}' or 'codex' binary",
                triple
            )));
        };

        // Install atomically (chmod +x happens on the staged temp before the
        // rename, so a running `codex` is never overwritten in place).
        atomic_install_binary(binary_path, install_path)?;

        // Cleanup
        let _ = fs::remove_dir_all(&tmp_dir);

        Ok(format!("Codex {} installed from prebuilt binary", version))
    }

    /// Detect the platform triple for prebuilt binary downloads
    fn detect_platform_triple() -> Option<&'static str> {
        // Codex's Linux releases are statically-linked musl builds; the gnu
        // targets were dropped upstream around rust-v0.118. The musl binaries
        // run fine on glibc systems thanks to static linking.
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return Some("x86_64-unknown-linux-musl");
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            return Some("aarch64-unknown-linux-musl");
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            return Some("aarch64-apple-darwin");
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            return Some("x86_64-apple-darwin");
        }
        #[allow(unreachable_code)]
        None
    }

    /// Build Codex from source (fallback when no prebuilt binary available)
    fn build_codex_from_source(install_path: &std::path::Path) -> io::Result<String> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("unleash/codex-source");

        let mut progress = Vec::new();

        // Clone or update the repo in cache
        if cache_dir.join(".git").exists() {
            progress.push(format!("Updating codex source at {}", cache_dir.display()));
            let output = Command::new("git")
                .args(["pull", "--ff-only"])
                .current_dir(&cache_dir)
                .output()?;

            if !output.status.success() {
                fs::remove_dir_all(&cache_dir)?;
                progress.push("Pull failed, re-cloning...".to_string());
            }
        }

        if !cache_dir.join(".git").exists() {
            progress.push("Cloning openai/codex from GitHub...".to_string());
            fs::create_dir_all(cache_dir.parent().unwrap())?;
            let output = Command::new("git")
                .args([
                    "clone",
                    "--depth",
                    "1",
                    "https://github.com/openai/codex.git",
                    &cache_dir.to_string_lossy(),
                ])
                .output()?;

            if !output.status.success() {
                return Err(io::Error::other(format!(
                    "Failed to clone codex: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }

            // Fetch tags so `git describe --tags` works on shallow clones
            let _ = Command::new("git")
                .args(["fetch", "--tags", "--depth=1"])
                .current_dir(&cache_dir)
                .output();
        }

        let codex_rs_dir = cache_dir.join("codex-rs");
        if !codex_rs_dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Codex codex-rs directory not found in cloned repo",
            ));
        }

        progress.push("Building codex from source (this may take a while)...".to_string());
        let output = Command::new("cargo")
            .args(["build", "--release", "-p", "codex-cli"])
            .current_dir(&codex_rs_dir)
            .output()?;

        if output.status.success() {
            let binary_path = codex_rs_dir.join("target/release/codex");
            atomic_install_binary(&binary_path, install_path)?;

            progress.push(format!(
                "Codex built and installed to {}",
                install_path.display()
            ));
            Ok(progress.join("\n"))
        } else {
            Err(io::Error::other(format!(
                "Failed to build Codex: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    /// Update OpenCode using its built-in upgrade command
    fn update_opencode(&self) -> io::Result<String> {
        if which::which("opencode").is_ok() {
            let output = Command::new("opencode")
                .args(["upgrade", "latest"])
                .output()?;

            if output.status.success() {
                Ok("OpenCode updated successfully".to_string())
            } else {
                Err(io::Error::other(format!(
                    "Failed to update OpenCode: {}",
                    String::from_utf8_lossy(&output.stderr)
                )))
            }
        } else {
            self.update_npm_agent("opencode-ai", "OpenCode")
        }
    }

    /// Update an npm-based agent to latest version
    fn update_npm_agent(&self, package: &str, name: &str) -> io::Result<String> {
        let output = crate::version::VersionManager::npm_global_command()
            .args(["install", "-g", &format!("{}@latest", package)])
            .output()?;

        if output.status.success() {
            Ok(format!("{} updated successfully", name))
        } else {
            Err(io::Error::other(format!(
                "Failed to update {}: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    /// Update Antigravity CLI via an AUR helper (yay/paru). Antigravity has
    /// no public npm or GitHub-releases channel, so this is the only way to
    /// upgrade it programmatically on Arch-family systems. On every other
    /// OS, returns an honest error pointing at the antigravity.google
    /// download page rather than the old "managed by pacman/yay" lie.
    fn update_antigravity(&self) -> io::Result<String> {
        use std::process::Command;

        let helper = ["yay", "paru"]
            .iter()
            .find(|h| Command::new(*h).arg("--version").output().is_ok());

        let Some(helper) = helper else {
            return Err(io::Error::other(
                "Antigravity CLI has no npm/GitHub release channel. \
                 Install via your distro's AUR helper (yay/paru — package \
                 `antigravity-cli`) or download from https://antigravity.google",
            ));
        };

        let output = Command::new(helper)
            .args(["-S", "--noconfirm", "--needed", "antigravity-cli"])
            .stdin(std::process::Stdio::null())
            .output()?;

        if output.status.success() {
            Ok("Antigravity CLI updated successfully".to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(io::Error::other(format!(
                "{} -S antigravity-cli failed: {}",
                helper, stderr
            )))
        }
    }

    /// Update Hermes via the official curl bash installer.
    /// Hermes' installer always installs the latest version — there is no
    /// version pin argument. `--skip-setup` bypasses the interactive setup
    /// wizard, which the installer otherwise drives by reading from /dev/tty
    /// even when piped from curl.
    ///
    /// install.sh's update path does `git pull --ff-only`, which aborts when
    /// the local clone has diverged from origin/main (upstream rebases,
    /// stray local commits). We pre-reset to upstream so the ff-only pull
    /// always succeeds — see `VersionManager::reset_diverged_hermes_clone`
    /// for the rationale and `install_hermes_version_streaming` for the
    /// TUI-side caller.
    fn update_hermes(&self) -> io::Result<String> {
        if let Some(dir) = crate::version::VersionManager::hermes_install_dir() {
            let branch = std::env::var("HERMES_BRANCH").unwrap_or_else(|_| "main".to_string());
            crate::version::VersionManager::reset_diverged_hermes_clone(
                &dir,
                &branch,
                &mut |msg| eprintln!("{}", msg),
            );
        }

        let output = Command::new("bash")
            .args([
                "-c",
                "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash -s -- --skip-setup",
            ])
            .stdin(std::process::Stdio::null())
            .output()?;

        if output.status.success() {
            Ok("Hermes Agent updated successfully".to_string())
        } else {
            Err(io::Error::other(format!(
                "Failed to update Hermes Agent: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    /// Get version cache file path
    fn version_cache_path(&self) -> PathBuf {
        self.config_dir.join("agent-versions.json")
    }

    /// Load version cache from disk
    fn load_version_cache(&mut self) -> io::Result<()> {
        let path = self.version_cache_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            self.versions = serde_json::from_str(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        }
        Ok(())
    }

    /// Save version cache to disk
    pub fn save_version_cache(&self) -> io::Result<()> {
        let path = self.version_cache_path();
        let content = serde_json::to_string_pretty(&self.versions)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        crate::config::atomic_write(&path, &content)
    }

    /// Get status summary for all agents
    pub fn status_summary(&mut self) -> Vec<(AgentType, Option<String>, Option<String>, bool)> {
        let agent_types: Vec<AgentType> = self.agents.keys().cloned().collect();
        let mut results = Vec::new();

        for agent_type in agent_types {
            let installed = self
                .get_installed_version(agent_type.clone())
                .ok()
                .flatten();
            let latest_revision = self
                .versions
                .get(&agent_type)
                .and_then(|v| v.latest.clone());
            let installed_revision = (agent_type == AgentType::Clanker)
                .then(crate::clanker::installed_revision)
                .flatten();
            let update_available = status_update_available(
                &agent_type,
                installed.as_deref(),
                latest_revision.as_deref(),
                installed_revision.as_deref(),
            );
            let latest = if agent_type == AgentType::Clanker {
                latest_revision
                    .as_deref()
                    .map(crate::clanker::revision_label)
                    .map(str::to_string)
            } else {
                latest_revision
            };
            results.push((agent_type, installed, latest, update_available));
        }

        results
    }
}

pub(super) fn status_update_available(
    agent_type: &AgentType,
    installed_version: Option<&str>,
    latest: Option<&str>,
    installed_revision: Option<&str>,
) -> bool {
    match agent_type {
        AgentType::Clanker => latest.is_some_and(|revision| installed_revision != Some(revision)),
        _ => match (installed_version, latest) {
            (Some(installed), Some(latest)) => crate::version::version_less_than(installed, latest),
            _ => false,
        },
    }
}
