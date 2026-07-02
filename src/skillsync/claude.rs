use super::{home_dir, DirectoryAdapter, Fidelity, Harness, Skill, SkillAdapter, SkillSyncError};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct ClaudeAdapter;

impl ClaudeAdapter {
    fn inner(&self) -> DirectoryAdapter {
        DirectoryAdapter::new(
            Harness::Claude,
            Fidelity::Native,
            home_dir().join(".claude/skills"),
        )
    }
}

impl SkillAdapter for ClaudeAdapter {
    fn harness(&self) -> Harness {
        self.inner().harness()
    }
    fn fidelity(&self) -> Fidelity {
        self.inner().fidelity()
    }
    fn root(&self) -> PathBuf {
        self.inner().root()
    }
    fn discover(&self) -> Result<Vec<Skill>, SkillSyncError> {
        self.inner().discover()
    }
    fn install(&self, skill: &Skill) -> Result<(), SkillSyncError> {
        self.inner().install(skill)
    }
    fn uninstall(&self, name: &str) -> Result<(), SkillSyncError> {
        self.inner().uninstall(name)
    }
}
