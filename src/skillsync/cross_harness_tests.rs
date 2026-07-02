#[cfg(test)]
mod tests {
    use crate::skillsync::{
        self, agy::AgyAdapter, claude::ClaudeAdapter, codex::CodexAdapter, copy_skill_dir,
        gemini::GeminiAdapter, hermes::HermesAdapter, opencode::OpenCodeAdapter, parse_skill_dir,
        pi::PiAdapter, Harness, Skill, SkillAdapter,
    };
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn fixture(name: &str) -> Skill {
        let path = fixtures_root().join(name);
        parse_skill_dir(&path).expect("fixture parses")
    }

    fn fixtures_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/skillsync/tests/fixtures/synthetic")
    }

    struct SandboxEnv {
        old_home: Option<OsString>,
        old_xdg_data_home: Option<OsString>,
        _temp: tempfile::TempDir,
    }

    impl SandboxEnv {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("temp home");
            let old_home = std::env::var_os("HOME");
            let old_xdg_data_home = std::env::var_os("XDG_DATA_HOME");
            std::env::set_var("HOME", temp.path());
            std::env::set_var("XDG_DATA_HOME", temp.path().join(".local/share"));
            Self {
                old_home,
                old_xdg_data_home,
                _temp: temp,
            }
        }

        fn home(&self) -> &Path {
            self._temp.path()
        }
    }

    impl Drop for SandboxEnv {
        fn drop(&mut self) {
            if let Some(home) = &self.old_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(xdg) = &self.old_xdg_data_home {
                std::env::set_var("XDG_DATA_HOME", xdg);
            } else {
                std::env::remove_var("XDG_DATA_HOME");
            }
        }
    }

    fn assert_portable_eq(expected: &Skill, actual: &Skill) {
        assert_eq!(expected.name, actual.name);
        assert_eq!(expected.description, actual.description);
        assert!(
            actual.body.contains(expected.body.trim()),
            "body not preserved\nexpected:\n{}\nactual:\n{}",
            expected.body,
            actual.body
        );
    }

    fn round_trip(adapter: &dyn SkillAdapter, skill: &Skill) {
        adapter.install(skill).expect("install");
        let discovered = adapter.discover().expect("discover");
        let actual = discovered
            .iter()
            .find(|s| s.name == skill.name)
            .unwrap_or_else(|| panic!("{} not discovered in {:?}", skill.name, adapter.harness()));
        assert_portable_eq(skill, actual);
        adapter.uninstall(&skill.name).expect("uninstall");
    }

    #[test]
    fn synthetic_skills_round_trip_through_adapters() {
        let _guard = env_lock().lock().unwrap();
        let _env = SandboxEnv::new();

        let adapters: Vec<Box<dyn SkillAdapter>> = vec![
            Box::new(ClaudeAdapter),
            Box::new(CodexAdapter),
            Box::new(GeminiAdapter),
            Box::new(AgyAdapter),
            Box::new(PiAdapter),
            Box::new(HermesAdapter),
            Box::new(OpenCodeAdapter),
        ];

        for skill_name in [
            "minimal-skill",
            "helper-script-skill",
            "unicode-edgecase-skill",
        ] {
            let skill = fixture(skill_name);
            for adapter in &adapters {
                round_trip(adapter.as_ref(), &skill);
            }
        }
    }

    #[test]
    fn sync_from_claude_uses_sandboxed_home_and_data_dirs() {
        let _guard = env_lock().lock().unwrap();
        let env = SandboxEnv::new();

        let source = env.home().join(".claude/skills/minimal-skill");
        copy_skill_dir(&fixtures_root().join("minimal-skill"), &source).expect("copy fixture");

        let changes = skillsync::sync(Some(Harness::Claude)).expect("sync from temp claude");
        assert!(changes.iter().any(|line| line.contains("minimal-skill")));

        assert!(env.home().join(".codex/prompts/minimal-skill.md").is_file());
        assert!(env
            .home()
            .join(".gemini/commands/minimal-skill.toml")
            .is_file());
        assert!(env
            .home()
            .join(".local/share/unleash/skills/minimal-skill/SKILL.md")
            .is_file());
        assert!(env
            .home()
            .join(".local/share/unleash/skills/skillsync.toml")
            .is_file());
    }
}
