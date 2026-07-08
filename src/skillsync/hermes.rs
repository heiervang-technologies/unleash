use super::{home_dir, Fidelity, Harness, Skill, SkillAdapter, SkillSyncError};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct HermesAdapter;

impl SkillAdapter for HermesAdapter {
    fn harness(&self) -> Harness {
        Harness::Hermes
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Reference
    }

    fn root(&self) -> PathBuf {
        home_dir().join(".hermes")
    }

    fn discover(&self) -> Result<Vec<Skill>, SkillSyncError> {
        super::context_reference_discover(&self.root().join("AGENTS.md"))
    }

    fn install(&self, skill: &Skill) -> Result<(), SkillSyncError> {
        super::context_reference_install(&self.root().join("AGENTS.md"), skill)
    }

    fn uninstall(&self, name: &str) -> Result<(), SkillSyncError> {
        super::context_reference_uninstall(&self.root().join("AGENTS.md"), name)
    }
}
