use super::{
    gemini::discover_commands, home_dir, render_gemini_command, validate_skill_name, Fidelity,
    Harness, Skill, SkillAdapter, SkillSyncError,
};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct AgyAdapter;

impl SkillAdapter for AgyAdapter {
    fn harness(&self) -> Harness {
        Harness::Agy
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Degraded
    }

    fn root(&self) -> PathBuf {
        home_dir().join(".gemini/commands")
    }

    fn discover(&self) -> Result<Vec<Skill>, SkillSyncError> {
        discover_commands(&self.root(), Harness::Agy)
    }

    fn install(&self, skill: &Skill) -> Result<(), SkillSyncError> {
        validate_skill_name(&skill.name)?;
        fs::create_dir_all(self.root())?;
        fs::write(
            self.root().join(format!("{}.toml", skill.name)),
            render_gemini_command(skill),
        )?;
        Ok(())
    }

    fn uninstall(&self, name: &str) -> Result<(), SkillSyncError> {
        validate_skill_name(name)?;
        let path = self.root().join(format!("{name}.toml"));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
