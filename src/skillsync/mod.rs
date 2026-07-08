//! Skill synchronization across supported agent harnesses.
//!
//! The hub format is the Agent Skills directory format: a directory named after
//! the skill with a `SKILL.md` file containing `name` and `description`
//! frontmatter.

pub mod agy;
pub mod claude;
pub mod codex;
pub mod gemini;
pub mod hermes;
pub mod opencode;
pub mod pi;

#[cfg(test)]
mod cross_harness_tests;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    OpenCode,
    Codex,
    Gemini,
    Agy,
    Pi,
    Hermes,
}

impl Harness {
    pub const ALL: [Harness; 7] = [
        Harness::Claude,
        Harness::OpenCode,
        Harness::Codex,
        Harness::Gemini,
        Harness::Agy,
        Harness::Pi,
        Harness::Hermes,
    ];

    pub fn adapter(self) -> Box<dyn SkillAdapter> {
        match self {
            Harness::Claude => Box::new(claude::ClaudeAdapter),
            Harness::OpenCode => Box::new(opencode::OpenCodeAdapter),
            Harness::Codex => Box::new(codex::CodexAdapter),
            Harness::Gemini => Box::new(gemini::GeminiAdapter),
            Harness::Agy => Box::new(agy::AgyAdapter),
            Harness::Pi => Box::new(pi::PiAdapter),
            Harness::Hermes => Box::new(hermes::HermesAdapter),
        }
    }
}

const CLAUDE_TARGET_GROUP: &[Harness] = &[Harness::Claude];
const OPENCODE_TARGET_GROUP: &[Harness] = &[Harness::OpenCode];
const CODEX_TARGET_GROUP: &[Harness] = &[Harness::Codex];
const GEMINI_AGY_TARGET_GROUP: &[Harness] = &[Harness::Gemini, Harness::Agy];
const PI_TARGET_GROUP: &[Harness] = &[Harness::Pi];
const HERMES_TARGET_GROUP: &[Harness] = &[Harness::Hermes];
const TARGET_GROUPS: [&[Harness]; 6] = [
    CLAUDE_TARGET_GROUP,
    OPENCODE_TARGET_GROUP,
    CODEX_TARGET_GROUP,
    GEMINI_AGY_TARGET_GROUP,
    PI_TARGET_GROUP,
    HERMES_TARGET_GROUP,
];

fn target_group_label(group: &[Harness]) -> String {
    group
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

impl fmt::Display for Harness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Harness::Claude => write!(f, "claude"),
            Harness::OpenCode => write!(f, "opencode"),
            Harness::Codex => write!(f, "codex"),
            Harness::Gemini => write!(f, "gemini"),
            Harness::Agy => write!(f, "agy"),
            Harness::Pi => write!(f, "pi"),
            Harness::Hermes => write!(f, "hermes"),
        }
    }
}

impl std::str::FromStr for Harness {
    type Err = SkillSyncError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" | "claude-code" => Ok(Self::Claude),
            "opencode" => Ok(Self::OpenCode),
            "codex" => Ok(Self::Codex),
            "gemini" | "gemini-cli" => Ok(Self::Gemini),
            "agy" | "antigravity" | "antigravity-cli" => Ok(Self::Agy),
            "pi" | "pi-coding-agent" => Ok(Self::Pi),
            "hermes" | "hermes-agent" => Ok(Self::Hermes),
            "hub" => Err(SkillSyncError::InvalidHarness(
                "hub is a sync source, not a harness".into(),
            )),
            _ => Err(SkillSyncError::InvalidHarness(format!(
                "unknown skills harness: {s}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Fidelity {
    Native,
    Degraded,
    Reference,
}

impl fmt::Display for Fidelity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fidelity::Native => write!(f, "Native"),
            Fidelity::Degraded => write!(f, "Degraded"),
            Fidelity::Reference => write!(f, "Reference"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillLocation {
    pub harness: Harness,
    pub fidelity: Fidelity,
    pub skill: Skill,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillStatus {
    pub name: String,
    pub description: String,
    pub present: BTreeMap<Harness, bool>,
    pub enabled: BTreeMap<Harness, bool>,
}

pub trait SkillAdapter {
    fn harness(&self) -> Harness;
    fn fidelity(&self) -> Fidelity;
    fn root(&self) -> PathBuf;
    fn discover(&self) -> Result<Vec<Skill>, SkillSyncError>;
    fn install(&self, skill: &Skill) -> Result<(), SkillSyncError>;
    fn uninstall(&self, name: &str) -> Result<(), SkillSyncError>;
}

#[derive(Debug)]
pub enum SkillSyncError {
    Io(io::Error),
    TomlDe(toml::de::Error),
    TomlSer(toml::ser::Error),
    InvalidHarness(String),
    InvalidSkill(String),
}

impl fmt::Display for SkillSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::TomlDe(e) => write!(f, "TOML parse error: {e}"),
            Self::TomlSer(e) => write!(f, "TOML write error: {e}"),
            Self::InvalidHarness(e) => write!(f, "{e}"),
            Self::InvalidSkill(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SkillSyncError {}

impl From<io::Error> for SkillSyncError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::de::Error> for SkillSyncError {
    fn from(e: toml::de::Error) -> Self {
        Self::TomlDe(e)
    }
}

impl From<toml::ser::Error> for SkillSyncError {
    fn from(e: toml::ser::Error) -> Self {
        Self::TomlSer(e)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AvailabilityManifest {
    #[serde(default)]
    pub skills: BTreeMap<String, SkillAvailability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAvailability {
    #[serde(default = "default_enabled")]
    pub enabled: BTreeMap<Harness, bool>,
}

fn default_enabled() -> BTreeMap<Harness, bool> {
    Harness::ALL.into_iter().map(|h| (h, true)).collect()
}

impl AvailabilityManifest {
    pub fn is_enabled(&self, skill: &str, harness: Harness) -> bool {
        self.skills
            .get(skill)
            .and_then(|s| s.enabled.get(&harness).copied())
            .unwrap_or(true)
    }

    pub fn ensure_skill(&mut self, skill: &str) {
        self.skills
            .entry(skill.to_string())
            .or_insert_with(|| SkillAvailability {
                enabled: default_enabled(),
            });
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| home_dir().join(".local/share"))
        .join("unleash")
        .join("skills")
}

pub fn hub_root() -> PathBuf {
    data_dir()
}

fn manifest_path() -> PathBuf {
    data_dir().join("skillsync.toml")
}

pub fn load_manifest() -> Result<AvailabilityManifest, SkillSyncError> {
    let path = manifest_path();
    if !path.exists() {
        return Ok(AvailabilityManifest::default());
    }
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}

pub fn save_manifest(manifest: &AvailabilityManifest) -> Result<(), SkillSyncError> {
    let path = manifest_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string_pretty(manifest)?)?;
    Ok(())
}

pub fn parse_skill_dir(path: &Path) -> Result<Skill, SkillSyncError> {
    let skill_md = path.join("SKILL.md");
    let content = fs::read_to_string(&skill_md)?;
    let (frontmatter, body) = split_frontmatter(&content)?;
    let name = frontmatter_value(frontmatter, "name")
        .or_else(|| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
        })
        .ok_or_else(|| {
            SkillSyncError::InvalidSkill(format!("missing name in {}", skill_md.display()))
        })?;
    validate_skill_name(&name)?;
    let description = frontmatter_value(frontmatter, "description").unwrap_or_default();
    Ok(Skill {
        name,
        description,
        body: body.trim_start().to_string(),
        path: path.to_path_buf(),
    })
}

fn split_frontmatter(content: &str) -> Result<(&str, &str), SkillSyncError> {
    let rest = if let Some(rest) = content.strip_prefix("---\n") {
        rest
    } else if let Some(rest) = content.strip_prefix("---\r\n") {
        rest
    } else {
        return Ok(("", content));
    };
    let Some(end) = rest.find("\n---") else {
        return Err(SkillSyncError::InvalidSkill(
            "unterminated SKILL.md frontmatter".into(),
        ));
    };
    let frontmatter = &rest[..end];
    let body = &rest[end + 4..];
    Ok((frontmatter, body))
}

fn frontmatter_value(frontmatter: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = frontmatter.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        let Some((k, v)) = trimmed.split_once(':') else {
            i += 1;
            continue;
        };
        if k.trim() != key {
            i += 1;
            continue;
        }

        let value = v.trim();
        if !value.is_empty() && value != "|" && value != ">" {
            return Some(unquote_yaml_scalar(value));
        }

        let mut collected = Vec::new();
        i += 1;
        while i < lines.len() {
            let next = lines[i];
            if !next.starts_with(' ') && !next.starts_with('\t') && next.contains(':') {
                break;
            }
            let piece = next.trim();
            if !piece.is_empty() {
                collected.push(piece);
            }
            i += 1;
        }
        if !collected.is_empty() {
            return Some(collected.join(" "));
        }
        return None;
    }
    None
}

fn unquote_yaml_scalar(value: &str) -> String {
    value.trim_matches('"').trim_matches('\'').to_string()
}

pub fn validate_skill_name(name: &str) -> Result<(), SkillSyncError> {
    if name.is_empty() || name.len() > 64 {
        return Err(SkillSyncError::InvalidSkill(format!(
            "invalid skill name '{name}': must be 1-64 characters"
        )));
    }
    if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return Err(SkillSyncError::InvalidSkill(format!(
            "invalid skill name '{name}': hyphens cannot lead, trail, or repeat"
        )));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(SkillSyncError::InvalidSkill(format!(
            "invalid skill name '{name}': use lowercase letters, digits, and hyphens only"
        )));
    }
    Ok(())
}

pub fn discover_skill_dirs(root: &Path) -> Result<Vec<Skill>, SkillSyncError> {
    let mut skills = Vec::new();
    if !root.is_dir() {
        return Ok(skills);
    }
    discover_skill_dirs_inner(root, &mut skills)?;
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

fn discover_skill_dirs_inner(dir: &Path, skills: &mut Vec<Skill>) -> Result<(), SkillSyncError> {
    if dir
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| matches!(n, ".git" | ".github" | ".hub" | ".archive" | ".system"))
    {
        return Ok(());
    }
    if dir.join("SKILL.md").is_file() {
        skills.push(parse_skill_dir(dir)?);
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            discover_skill_dirs_inner(&entry.path(), skills)?;
        }
    }
    Ok(())
}

pub fn copy_skill_dir(src: &Path, dest: &Path) -> Result<(), SkillSyncError> {
    if same_path(src, dest) {
        return Ok(());
    }
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    copy_dir_recursive(src, dest)?;
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub fn materialize_skill_dir(skill: &Skill, dest: &Path) -> Result<(), SkillSyncError> {
    validate_skill_name(&skill.name)?;
    if skill.path.is_dir() && skill.path.join("SKILL.md").is_file() {
        copy_skill_dir(&skill.path, dest)?;
        return Ok(());
    }
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::create_dir_all(dest)?;
    fs::write(dest.join("SKILL.md"), render_skill_md(skill))?;
    Ok(())
}

fn render_skill_md(skill: &Skill) -> String {
    format!(
        "---\nname: \"{}\"\ndescription: \"{}\"\n---\n\n{}",
        skill.name.replace('"', "\\\""),
        skill.description.replace('"', "\\\""),
        skill.body.trim_start()
    )
}

pub fn render_codex_prompt(skill: &Skill) -> String {
    format!(
        "# Codex Custom Prompt: {name}\n\n\
> [!NOTE]\n\
> This instruction set was automatically generated and synchronized from the Unleash skill **\"{name}\"**.\n\
> **Description:** {description}\n\n\
---\n\n\
## Instructions for the Model\n\
You are acting with the \"{name}\" skill active. Adhere strictly to the guidelines and workflows specified below whenever the user requests actions related to this skill's scope:\n\n\
{body}",
        name = skill.name,
        description = skill.description,
        body = skill.body.trim_start()
    )
}

pub fn render_gemini_command(skill: &Skill) -> String {
    format!(
        "# Synced via Unleash skillsync\n\
description = \"{description} (Synced Skill)\"\n\n\
prompt = \"\"\"\n\
# Custom Command: /{name}\n\n\
You are executing a custom command synchronized from the Unleash skill '{name}'.\n\n\
**Scope/Description**: {description}\n\
**User Arguments**: {{{{args}}}}\n\n\
Please execute this task by following these skill instructions:\n\n\
{body}\n\n\
---\n\
Use the provided user arguments (if any) to parameterize this run (e.g. specifying target environments, branches, or tags).\n\
\"\"\"\n",
        name = skill.name,
        description = escape_toml_basic_string(&skill.description),
        body = skill.body.trim_start().replace("\"\"\"", "\\\"\\\"\\\"")
    )
}

pub fn render_context_reference(skill: &Skill) -> String {
    let skill_md = skill.path.join("SKILL.md");
    format!(
        "<!-- unleash-skillsync-start: {name} -->\n\
### 🛠️ Synced Skill: {name}\n\n\
*   **Description:** {description}\n\
*   **Source Skill Path:** [skills/{name}](file://{path})\n\n\
When asked to work on tasks matching this skill, you should locate and read the full skill instructions in the file linked above. If you cannot access the link, use these summarized guidelines:\n\
{body}\n\
<!-- unleash-skillsync-end: {name} -->\n",
        name = skill.name,
        description = skill.description,
        path = skill_md.display(),
        body = skill.body.trim_start()
    )
}

pub fn context_reference_discover(path: &Path) -> Result<Vec<Skill>, SkillSyncError> {
    let mut skills = Vec::new();
    if !path.exists() {
        return Ok(skills);
    }
    let content = fs::read_to_string(path)?;
    let mut rest = content.as_str();
    while let Some(start_idx) = rest.find("<!-- unleash-skillsync-start: ") {
        rest = &rest[start_idx + "<!-- unleash-skillsync-start: ".len()..];
        let Some(name_end) = rest.find(" -->") else {
            break;
        };
        let name = rest[..name_end].trim().to_string();
        validate_skill_name(&name)?;
        let block_start = name_end + " -->".len();
        let end_marker = format!("<!-- unleash-skillsync-end: {name} -->");
        let Some(end_idx) = rest[block_start..].find(&end_marker) else {
            break;
        };
        let block = &rest[block_start..block_start + end_idx];
        let description = block
            .lines()
            .find_map(|line| line.trim().strip_prefix("*   **Description:** "))
            .unwrap_or_default()
            .to_string();
        let body = block
            .split("If you cannot access the link, use these summarized guidelines:\n")
            .nth(1)
            .unwrap_or_default()
            .trim()
            .to_string();
        skills.push(Skill {
            name,
            description,
            body,
            path: path.to_path_buf(),
        });
        rest = &rest[block_start + end_idx + end_marker.len()..];
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

pub fn context_reference_install(path: &Path, skill: &Skill) -> Result<(), SkillSyncError> {
    validate_skill_name(&skill.name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = fs::read_to_string(path).unwrap_or_default();
    let replacement = render_context_reference(skill);
    fs::write(
        path,
        replace_context_block(&existing, &skill.name, &replacement),
    )?;
    Ok(())
}

pub fn context_reference_uninstall(path: &Path, name: &str) -> Result<(), SkillSyncError> {
    validate_skill_name(name)?;
    if !path.exists() {
        return Ok(());
    }
    let existing = fs::read_to_string(path)?;
    fs::write(path, replace_context_block(&existing, name, ""))?;
    Ok(())
}

fn replace_context_block(existing: &str, name: &str, replacement: &str) -> String {
    let start = format!("<!-- unleash-skillsync-start: {name} -->");
    let end = format!("<!-- unleash-skillsync-end: {name} -->");
    if let Some(start_idx) = existing.find(&start) {
        if let Some(rel_end_idx) = existing[start_idx..].find(&end) {
            let end_idx = start_idx + rel_end_idx + end.len();
            let mut out = String::new();
            out.push_str(existing[..start_idx].trim_end());
            if !out.is_empty() && !replacement.trim().is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(replacement.trim_end());
            let tail = existing[end_idx..].trim_start();
            if !tail.is_empty() {
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(tail);
            }
            if !out.is_empty() {
                out.push('\n');
            }
            return out;
        }
    }

    let mut out = existing.trim_end().to_string();
    if !out.is_empty() && !replacement.trim().is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(replacement.trim_end());
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn escape_toml_basic_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), SkillSyncError> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct DirectoryAdapter {
    pub harness: Option<Harness>,
    pub fidelity: Option<Fidelity>,
    pub root: Option<PathBuf>,
}

impl DirectoryAdapter {
    pub fn new(harness: Harness, fidelity: Fidelity, root: PathBuf) -> Self {
        Self {
            harness: Some(harness),
            fidelity: Some(fidelity),
            root: Some(root),
        }
    }
}

impl SkillAdapter for DirectoryAdapter {
    fn harness(&self) -> Harness {
        self.harness.expect("adapter harness")
    }

    fn fidelity(&self) -> Fidelity {
        self.fidelity.expect("adapter fidelity")
    }

    fn root(&self) -> PathBuf {
        self.root.clone().expect("adapter root")
    }

    fn discover(&self) -> Result<Vec<Skill>, SkillSyncError> {
        discover_skill_dirs(&self.root())
    }

    fn install(&self, skill: &Skill) -> Result<(), SkillSyncError> {
        validate_skill_name(&skill.name)?;
        let root = self.root();
        fs::create_dir_all(&root)?;
        copy_skill_dir(&skill.path, &root.join(&skill.name))
    }

    fn uninstall(&self, name: &str) -> Result<(), SkillSyncError> {
        validate_skill_name(name)?;
        let path = self.root().join(name);
        if path.exists() {
            fs::remove_dir_all(path)?;
        }
        Ok(())
    }
}

pub fn discover_all() -> Result<Vec<SkillLocation>, SkillSyncError> {
    let mut out = Vec::new();
    for harness in Harness::ALL {
        let adapter = harness.adapter();
        for skill in adapter.discover()? {
            out.push(SkillLocation {
                harness,
                fidelity: adapter.fidelity(),
                skill,
            });
        }
    }
    Ok(out)
}

pub fn discover_hub() -> Result<Vec<Skill>, SkillSyncError> {
    discover_skill_dirs(&hub_root())
}

pub fn sync(source: Option<Harness>, delete_orphans: bool) -> Result<Vec<String>, SkillSyncError> {
    let mut manifest = load_manifest()?;
    let source_skills = match source {
        Some(harness) => harness.adapter().discover()?,
        None => discover_hub()?,
    };
    let source_names: BTreeSet<String> = source_skills
        .iter()
        .map(|skill| skill.name.clone())
        .collect();
    let mut changes = Vec::new();

    fs::create_dir_all(hub_root())?;
    for skill in &source_skills {
        manifest.ensure_skill(&skill.name);
        materialize_skill_dir(skill, &hub_root().join(&skill.name))?;
    }

    if delete_orphans && source.is_some() {
        for skill in discover_hub()? {
            if source_names.contains(&skill.name) {
                continue;
            }

            for group in TARGET_GROUPS {
                if source.is_some_and(|source| group.contains(&source)) {
                    continue;
                }
                let label = target_group_label(group);
                group[0].adapter().uninstall(&skill.name)?;
                changes.push(format!("deleted orphan {} from {}", skill.name, label));
            }

            let hub_path = hub_root().join(&skill.name);
            if hub_path.exists() {
                fs::remove_dir_all(hub_path)?;
                changes.push(format!("deleted orphan {} from hub", skill.name));
            }
            manifest.skills.remove(&skill.name);
        }
    }

    let hub_skills = discover_hub()?;
    for skill in &hub_skills {
        manifest.ensure_skill(&skill.name);
        for group in TARGET_GROUPS {
            if source.is_some_and(|source| group.contains(&source)) {
                continue;
            }
            let label = target_group_label(group);
            let adapter = group[0].adapter();
            if group
                .iter()
                .any(|harness| manifest.is_enabled(&skill.name, *harness))
            {
                adapter.install(skill)?;
                changes.push(format!("installed {} -> {}", skill.name, label));
            } else {
                adapter.uninstall(&skill.name)?;
                changes.push(format!("uninstalled {} from {}", skill.name, label));
            }
        }
    }
    save_manifest(&manifest)?;
    Ok(changes)
}

pub fn diff(source: Option<Harness>, delete_orphans: bool) -> Result<Vec<String>, SkillSyncError> {
    let mut planned = Vec::new();
    let manifest = load_manifest()?;
    let skills = match source {
        Some(harness) => harness.adapter().discover()?,
        None => discover_hub()?,
    };
    let source_names: BTreeSet<String> = skills.iter().map(|skill| skill.name.clone()).collect();
    for skill in &skills {
        for group in TARGET_GROUPS {
            if source.is_some_and(|source| group.contains(&source)) {
                continue;
            }
            let label = target_group_label(group);
            if group
                .iter()
                .any(|harness| manifest.is_enabled(&skill.name, *harness))
            {
                planned.push(format!("would install {} -> {}", skill.name, label));
            } else {
                planned.push(format!("would uninstall {} from {}", skill.name, label));
            }
        }
    }
    if delete_orphans && source.is_some() {
        for skill in discover_hub()? {
            if source_names.contains(&skill.name) {
                continue;
            }
            for group in TARGET_GROUPS {
                if source.is_some_and(|source| group.contains(&source)) {
                    continue;
                }
                planned.push(format!(
                    "would delete orphan {} from {}",
                    skill.name,
                    target_group_label(group)
                ));
            }
            planned.push(format!("would delete orphan {} from hub", skill.name));
        }
    }
    Ok(planned)
}

pub fn status() -> Result<Vec<SkillStatus>, SkillSyncError> {
    let manifest = load_manifest()?;
    let mut by_name: BTreeMap<String, SkillStatus> = BTreeMap::new();
    let mut descriptions: BTreeMap<String, String> = BTreeMap::new();

    for skill in discover_hub()? {
        descriptions.insert(skill.name.clone(), skill.description.clone());
        by_name
            .entry(skill.name.clone())
            .or_insert_with(|| SkillStatus {
                name: skill.name.clone(),
                description: skill.description.clone(),
                present: BTreeMap::new(),
                enabled: BTreeMap::new(),
            });
    }

    for loc in discover_all()? {
        descriptions
            .entry(loc.skill.name.clone())
            .or_insert_with(|| loc.skill.description.clone());
        let entry = by_name
            .entry(loc.skill.name.clone())
            .or_insert_with(|| SkillStatus {
                name: loc.skill.name.clone(),
                description: loc.skill.description.clone(),
                present: BTreeMap::new(),
                enabled: BTreeMap::new(),
            });
        entry.present.insert(loc.harness, true);
    }

    for entry in by_name.values_mut() {
        if let Some(desc) = descriptions.get(&entry.name) {
            entry.description = desc.clone();
        }
        for harness in Harness::ALL {
            entry.present.entry(harness).or_insert(false);
            entry
                .enabled
                .insert(harness, manifest.is_enabled(&entry.name, harness));
        }
    }
    Ok(by_name.into_values().collect())
}

pub fn skill_names(skills: &[SkillLocation]) -> BTreeSet<String> {
    skills.iter().map(|s| s.skill.name.clone()).collect()
}
