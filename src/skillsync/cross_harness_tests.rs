#[cfg(test)]
mod tests {
    use crate::skillsync::{
        agy::AgyAdapter, claude::ClaudeAdapter, codex::CodexAdapter, gemini::GeminiAdapter,
        hermes::HermesAdapter, opencode::OpenCodeAdapter, parse_skill_dir, pi::PiAdapter, Skill,
        SkillAdapter,
    };
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn fixture(name: &str) -> Skill {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/skillsync/tests/fixtures/synthetic")
            .join(name);
        parse_skill_dir(&path).expect("fixture parses")
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
        let temp = tempfile::tempdir().expect("temp home");
        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", temp.path());

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

        if let Some(home) = old_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }
}
