use super::{
    home_dir, render_gemini_command, Fidelity, Harness, Skill, SkillAdapter, SkillSyncError,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct GeminiAdapter;

impl SkillAdapter for GeminiAdapter {
    fn harness(&self) -> Harness {
        Harness::Gemini
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Degraded
    }

    fn root(&self) -> PathBuf {
        home_dir().join(".gemini/commands")
    }

    fn discover(&self) -> Result<Vec<Skill>, SkillSyncError> {
        discover_commands(&self.root(), Harness::Gemini)
    }

    fn install(&self, skill: &Skill) -> Result<(), SkillSyncError> {
        fs::create_dir_all(self.root())?;
        fs::write(
            self.root().join(format!("{}.toml", skill.name)),
            render_gemini_command(skill),
        )?;
        Ok(())
    }

    fn uninstall(&self, name: &str) -> Result<(), SkillSyncError> {
        let path = self.root().join(format!("{name}.toml"));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

pub(crate) fn discover_commands(
    root: &Path,
    _harness: Harness,
) -> Result<Vec<Skill>, SkillSyncError> {
    let mut skills = Vec::new();
    if !root.is_dir() {
        return Ok(skills);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        if let Some(skill) = parse_command(&path)? {
            skills.push(skill);
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

fn parse_command(path: &Path) -> Result<Option<Skill>, SkillSyncError> {
    let content = fs::read_to_string(path)?;
    if !content.starts_with("# Synced via Unleash skillsync") {
        return Ok(None);
    }
    let value: toml::Value = toml::from_str(&content)?;
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim_end_matches(" (Synced Skill)")
        .to_string();
    let prompt = value
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let name = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let body = prompt
        .split("skill instructions:\n\n")
        .nth(1)
        .and_then(|s| s.split("\n\n---\nUse the provided user arguments").next())
        .unwrap_or_default()
        .to_string();
    Ok(Some(Skill {
        name,
        description,
        body,
        path: path.to_path_buf(),
    }))
}
