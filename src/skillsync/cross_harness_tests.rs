#[cfg(test)]
mod tests {
    use crate::skillsync::{
        self, agy::AgyAdapter, claude::ClaudeAdapter, codex::CodexAdapter, copy_skill_dir,
        gemini::GeminiAdapter, hermes::HermesAdapter, materialize_skill_dir,
        opencode::OpenCodeAdapter, parse_skill_dir, pi::PiAdapter, Harness, Skill, SkillAdapter,
    };
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn env_guard() -> MutexGuard<'static, ()> {
        env_lock().lock().unwrap_or_else(|err| err.into_inner())
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

    fn write_minimal_skill_manifest(env: &SandboxEnv, gemini: bool, agy: bool) {
        let manifest_path = env
            .home()
            .join(".local/share/unleash/skills/skillsync.toml");
        fs::create_dir_all(manifest_path.parent().expect("manifest parent")).expect("manifest dir");
        fs::write(
            manifest_path,
            format!(
                r#"[skills."minimal-skill".enabled]
claude = true
opencode = true
codex = true
gemini = {gemini}
agy = {agy}
pi = true
hermes = true
"#
            ),
        )
        .expect("write manifest");
    }

    #[test]
    fn skill_names_reject_path_traversal() {
        let _guard = env_guard();
        let env = SandboxEnv::new();
        let mut skill = fixture("minimal-skill");
        skill.name = "../escape".to_string();

        let err = CodexAdapter
            .install(&skill)
            .expect_err("invalid name rejected");
        assert!(err.to_string().contains("invalid skill name"));
        assert!(!env.home().join(".codex/escape.md").exists());

        let hub_dest = env.home().join(".local/share/unleash/skills/../escape");
        let err = materialize_skill_dir(&skill, &hub_dest).expect_err("invalid hub name rejected");
        assert!(err.to_string().contains("invalid skill name"));
        assert!(!env.home().join(".local/share/unleash/escape").exists());
    }

    #[test]
    fn multiline_frontmatter_description_is_preserved() {
        let temp = tempfile::tempdir().expect("temp skill");
        let skill_dir = temp.path().join("multiline-skill");
        fs::create_dir_all(&skill_dir).expect("skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: multiline-skill
description: >
  Use this skill for one
  and two.
---
Body text.
"#,
        )
        .expect("write skill");

        let skill = parse_skill_dir(&skill_dir).expect("parse multiline frontmatter");
        assert_eq!(skill.name, "multiline-skill");
        assert_eq!(skill.description, "Use this skill for one and two.");
    }

    #[test]
    fn synthetic_skills_round_trip_through_adapters() {
        let _guard = env_guard();
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
        let _guard = env_guard();
        let env = SandboxEnv::new();

        let source = env.home().join(".claude/skills/minimal-skill");
        copy_skill_dir(&fixtures_root().join("minimal-skill"), &source).expect("copy fixture");

        let changes = skillsync::sync(Some(Harness::Claude), false).expect("sync from temp claude");
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

    #[test]
    fn shared_gemini_agy_target_is_not_deleted_when_one_alias_is_disabled() {
        let _guard = env_guard();
        let env = SandboxEnv::new();

        let source = env.home().join(".claude/skills/minimal-skill");
        copy_skill_dir(&fixtures_root().join("minimal-skill"), &source).expect("copy fixture");
        write_minimal_skill_manifest(&env, true, false);

        let changes = skillsync::sync(Some(Harness::Claude), false).expect("sync from temp claude");
        assert!(changes
            .iter()
            .any(|line| line == "installed minimal-skill -> gemini/agy"));
        assert!(env
            .home()
            .join(".gemini/commands/minimal-skill.toml")
            .is_file());
    }

    #[test]
    fn shared_gemini_agy_target_group_is_skipped_when_source_is_gemini() {
        let _guard = env_guard();
        let env = SandboxEnv::new();
        let skill = fixture("minimal-skill");

        GeminiAdapter
            .install(&skill)
            .expect("install gemini source");
        write_minimal_skill_manifest(&env, true, false);

        let changes = skillsync::sync(Some(Harness::Gemini), false).expect("sync from gemini");
        assert!(!changes
            .iter()
            .any(|line| line.contains("minimal-skill") && line.contains("gemini/agy")));
        assert!(env
            .home()
            .join(".gemini/commands/minimal-skill.toml")
            .is_file());
    }

    #[test]
    fn sync_from_hub_does_not_delete_hub_skill_dir() {
        let _guard = env_guard();
        let env = SandboxEnv::new();

        let hub_skill = env.home().join(".local/share/unleash/skills/minimal-skill");
        copy_skill_dir(&fixtures_root().join("minimal-skill"), &hub_skill).expect("copy fixture");

        let changes = skillsync::sync(None, false).expect("sync from hub");
        assert!(changes.iter().any(|line| line.contains("minimal-skill")));
        assert!(hub_skill.join("SKILL.md").is_file());
    }

    #[test]
    fn sync_delete_orphans_removes_sandboxed_targets_and_manifest_entry() {
        let _guard = env_guard();
        let env = SandboxEnv::new();

        let source = env.home().join(".claude/skills/minimal-skill");
        copy_skill_dir(&fixtures_root().join("minimal-skill"), &source).expect("copy fixture");
        skillsync::sync(Some(Harness::Claude), false).expect("initial sync");
        fs::remove_dir_all(&source).expect("remove source skill");

        let changes = skillsync::sync(Some(Harness::Claude), true).expect("orphan cleanup");
        assert!(changes
            .iter()
            .any(|line| line == "deleted orphan minimal-skill from hub"));

        assert!(!env.home().join(".codex/prompts/minimal-skill.md").exists());
        assert!(!env
            .home()
            .join(".gemini/commands/minimal-skill.toml")
            .exists());
        assert!(!env
            .home()
            .join(".local/share/unleash/skills/minimal-skill")
            .exists());

        let manifest = fs::read_to_string(
            env.home()
                .join(".local/share/unleash/skills/skillsync.toml"),
        )
        .expect("manifest");
        assert!(!manifest.contains("minimal-skill"));
    }
}
