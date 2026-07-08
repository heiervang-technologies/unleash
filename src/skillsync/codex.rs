use super::{
    home_dir, render_codex_prompt, validate_skill_name, Fidelity, Harness, Skill, SkillAdapter,
    SkillSyncError,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct CodexAdapter;

impl SkillAdapter for CodexAdapter {
    fn harness(&self) -> Harness {
        Harness::Codex
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Degraded
    }

    fn root(&self) -> PathBuf {
        home_dir().join(".codex/prompts")
    }

    fn discover(&self) -> Result<Vec<Skill>, SkillSyncError> {
        let root = self.root();
        let mut skills = Vec::new();
        if !root.is_dir() {
            return Ok(skills);
        }
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Some(skill) = parse_codex_prompt(&path)? {
                skills.push(skill);
            }
        }
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(skills)
    }

    fn install(&self, skill: &Skill) -> Result<(), SkillSyncError> {
        validate_skill_name(&skill.name)?;
        fs::create_dir_all(self.root())?;
        fs::write(
            self.root().join(format!("{}.md", skill.name)),
            render_codex_prompt(skill),
        )?;
        Ok(())
    }

    fn uninstall(&self, name: &str) -> Result<(), SkillSyncError> {
        validate_skill_name(name)?;
        let path = self.root().join(format!("{name}.md"));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

fn parse_codex_prompt(path: &Path) -> Result<Option<Skill>, SkillSyncError> {
    let content = fs::read_to_string(path)?;
    let Some(first) = content.lines().next() else {
        return Ok(None);
    };
    let Some(name) = first.strip_prefix("# Codex Custom Prompt: ") else {
        return Ok(None);
    };
    validate_skill_name(name.trim())?;
    let description = content
        .lines()
        .find_map(|line| line.strip_prefix("> **Description:** "))
        .unwrap_or_default()
        .to_string();
    let body = content
        .split("scope:\n\n")
        .nth(1)
        .unwrap_or_default()
        .to_string();
    Ok(Some(Skill {
        name: name.trim().to_string(),
        description,
        body,
        path: path.to_path_buf(),
    }))
}
