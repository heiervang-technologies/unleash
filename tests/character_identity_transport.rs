#![cfg(unix)]

use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use unleash::config::AppConfig;
use unleash::config::CustomAgentConfig;
use unleash::config::Profile;
use unleash::config::ProfileManager;

struct Harness {
    _root: tempfile::TempDir,
    home: PathBuf,
    config_base: PathBuf,
    data_home: PathBuf,
    cache_home: PathBuf,
    codex_home: PathBuf,
    profile_codex_home: PathBuf,
    capture: PathBuf,
}

impl Harness {
    fn new() -> Self {
        Self::with_profile("codex", "codex")
    }

    fn with_profile(profile_name: &str, binary_name: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let config_base = root.path().join("config");
        let data_home = root.path().join("data");
        let cache_home = root.path().join("cache");
        let codex_home = root.path().join("ambient-codex-home");
        let profile_codex_home = root.path().join("profile-codex-home");
        let capture = root.path().join("capture");
        for path in [
            &home,
            &config_base,
            &data_home,
            &cache_home,
            &codex_home,
            &profile_codex_home,
            &capture,
        ] {
            fs::create_dir_all(path).unwrap();
        }

        let target = root.path().join("bin").join(binary_name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(
            &target,
            r#"#!/bin/sh
set -eu
capture=${IDENTITY_CAPTURE_DIR:?}

if [ "${1-}" = character ]; then
  printf '%s\n' "$*" >> "$capture/resolver.calls"
  printf '%s' "${CODEX_HOME-unset}" > "$capture/resolver.codex_home"
  case "${RESOLVER_MODE-ok}:${3-}" in
    ok:Cleo)
      printf '%s\n' '{"schemaVersion":1,"ok":true,"input":"Cleo","id":"chloe","displayName":"Chloe","manifestPath":"/tmp/fixture/character.json","matchKind":"explicit_alias"}'
      ;;
    ok:Rusty)
      printf '%s\n' '{"schemaVersion":1,"ok":true,"input":"Rusty","id":"clanker","displayName":"Rusty Clanker","manifestPath":"/tmp/fixture/character.json","matchKind":"explicit_alias"}'
      ;;
    malformed:*)
      printf '%s\n' '{"schemaVersion":1,"ok":true}'
      ;;
    mismatch:*)
      printf '%s\n' '{"schemaVersion":1,"ok":true,"input":"Rusty","id":"chloe","displayName":"Chloe","manifestPath":"/tmp/fixture/character.json","matchKind":"explicit_alias"}'
      ;;
    *)
      printf '%s\n' 'not found' >&2
      exit 1
      ;;
  esac
  exit 0
fi

iteration_file="$capture/iteration"
iteration=0
if [ -f "$iteration_file" ]; then
  iteration=$(cat "$iteration_file")
fi
iteration=$((iteration + 1))
printf '%s' "$iteration" > "$iteration_file"
: > "$capture/argv.$iteration"
for arg in "$@"; do
  printf '%s\n' "$arg" >> "$capture/argv.$iteration"
done
printf '%s' "${CLANKER_ID-unset}" > "$capture/clanker_id.$iteration"
printf '%s' "$PWD" > "$capture/cwd.$iteration"
printf '%s' "${TMUX-unset}" > "$capture/tmux.$iteration"

if [ "${TRIGGER_ONE_RESTART-0}" = 1 ] && [ "$iteration" = 1 ]; then
  mkdir -p "${XDG_CACHE_HOME:?}/unleash/process-restart"
  : > "${XDG_CACHE_HOME}/unleash/process-restart/restart-trigger-${AGENT_WRAPPER_PID:?}"
fi
"#,
        )
        .unwrap();
        let mut permissions = fs::metadata(&target).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&target, permissions).unwrap();

        let manager = ProfileManager::with_config_dir(config_base.join("unleash")).unwrap();
        let mut profile = Profile::new(profile_name);
        profile.agent_cli_path = target.to_string_lossy().into_owned();
        profile.env = HashMap::from([
            ("AU_HYPRLAND_FOCUS".to_string(), "0".to_string()),
            (
                "CODEX_HOME".to_string(),
                profile_codex_home.to_string_lossy().into_owned(),
            ),
            (
                "IDENTITY_CAPTURE_DIR".to_string(),
                capture.to_string_lossy().into_owned(),
            ),
        ]);
        manager.save_profile(&profile).unwrap();
        let custom_agents = if profile_name == "clanker" {
            let mut polyfill = unleash::agents::AgentDefinition::codex().polyfill;
            polyfill.name_flag = None;
            vec![CustomAgentConfig {
                name: "clanker".to_string(),
                binary: "clanker".to_string(),
                description: "Clanker Code fixture".to_string(),
                polyfill,
                github_repo: None,
                npm_package: None,
                asset_template: None,
                enabled: true,
            }]
        } else {
            Vec::new()
        };
        manager
            .save_app_config(&AppConfig {
                current_profile: profile_name.to_string(),
                custom_agents,
                ..AppConfig::default()
            })
            .unwrap();

        Self {
            _root: root,
            home,
            config_base,
            data_home,
            cache_home,
            codex_home,
            profile_codex_home,
            capture,
        }
    }

    fn command(&self, cwd: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_unleash"));
        command
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.config_base)
            .env("XDG_DATA_HOME", &self.data_home)
            .env("XDG_CACHE_HOME", &self.cache_home)
            .env("CODEX_HOME", &self.codex_home)
            .env("AU_HYPRLAND_FOCUS", "0")
            .env_remove("AGENT_CMD")
            .env_remove("AGENT_UNLEASH")
            .env_remove("UNLEASH_POLYFILL_ACTIVE");
        command
    }

    fn argv(&self, iteration: usize) -> Vec<String> {
        fs::read_to_string(self.capture.join(format!("argv.{iteration}")))
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn captured(&self, name: &str, iteration: usize) -> String {
        fs::read_to_string(self.capture.join(format!("{name}.{iteration}"))).unwrap()
    }
}

fn assert_name_pair(args: &[String], expected: &str) {
    assert!(
        args.windows(2).any(|pair| pair == ["--name", expected]),
        "missing --name {expected:?} in {args:?}"
    );
}

#[test]
fn explicit_alias_transports_original_argv_and_canonical_env_across_workspaces_and_tmux() {
    for (requested, canonical, tmux) in [
        ("Cleo", "chloe", None),
        ("Rusty", "clanker", Some("fixture")),
    ] {
        let harness = Harness::new();
        let workspace = harness.home.join(format!("workspace-{canonical}"));
        fs::create_dir_all(&workspace).unwrap();
        let mut command = harness.command(&workspace);
        command
            .args(["codex", "--name", requested])
            .env("CLANKER_ID", "stale-parent-value");
        match tmux {
            Some(value) => {
                command.env("TMUX", value);
            }
            None => {
                command.env_remove("TMUX");
            }
        }

        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_name_pair(&harness.argv(1), requested);
        assert_eq!(
            fs::read_to_string(harness.capture.join("resolver.calls"))
                .unwrap()
                .trim(),
            format!("character resolve {requested} --json --materialize-builtin")
        );
        assert_eq!(harness.captured("clanker_id", 1), canonical);
        assert_eq!(harness.captured("cwd", 1), workspace.to_string_lossy());
        assert_eq!(harness.captured("tmux", 1), tmux.unwrap_or("unset"));
        assert!(!harness.config_base.join("unleash/characters").exists());
    }
}

#[test]
fn bare_launch_never_resolves_and_preserves_inherited_identity_behavior() {
    let harness = Harness::new();
    let workspace = harness.home.join("bare-workspace");
    fs::create_dir_all(&workspace).unwrap();

    let output = harness
        .command(&workspace)
        .arg("codex")
        .env("CLANKER_ID", "inherited")
        .env_remove("TMUX")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!harness.capture.join("resolver.calls").exists());
    assert!(!harness.argv(1).iter().any(|arg| arg == "--name"));
    assert_eq!(harness.captured("clanker_id", 1), "inherited");
}

#[test]
fn live_clanker_profile_uses_codex_identity_and_restart_semantics() {
    let harness = Harness::with_profile("clanker", "clanker");
    let output = harness
        .command(&harness.home)
        .args(["clanker", "--name", "Cleo"])
        .env("TRIGGER_ONE_RESTART", "1")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for iteration in [1, 2] {
        assert_name_pair(&harness.argv(iteration), "Cleo");
        assert_eq!(harness.captured("clanker_id", iteration), "chloe");
    }
    assert_eq!(&harness.argv(2)[..2], ["resume", "--last"]);
}

#[test]
fn profile_environment_wins_for_character_resolution_context() {
    let harness = Harness::new();
    let output = harness
        .command(&harness.home)
        .args(["codex", "--name", "Cleo"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(harness.codex_home, harness.profile_codex_home);
    assert_eq!(
        fs::read_to_string(harness.capture.join("resolver.codex_home")).unwrap(),
        harness.profile_codex_home.to_string_lossy()
    );
}

#[test]
fn malformed_mismatched_or_unresolved_identity_fails_before_child_launch() {
    for mode in ["malformed", "mismatch", "missing"] {
        let harness = Harness::new();
        let output = harness
            .command(&harness.home)
            .args(["codex", "--name", "Cleo"])
            .env("RESOLVER_MODE", mode)
            .output()
            .unwrap();

        assert!(!output.status.success());
        assert!(!harness.capture.join("argv.1").exists());
    }
}

#[test]
fn named_meta_commands_skip_resolution_and_preserve_meta_behavior() {
    for meta in ["--help", "--version", "doctor"] {
        let harness = Harness::new();
        let output = harness
            .command(&harness.home)
            .args(["codex", "--name", "Cleo", meta])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{meta}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!harness.capture.join("resolver.calls").exists());
    }
}

#[test]
fn dry_run_resolves_only_explicit_names_and_prints_verified_transport() {
    let named = Harness::new();
    let output = named
        .command(&named.home)
        .args(["codex", "--name", "Cleo", "--dry-run"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(named.capture.join("resolver.calls").exists());
    assert!(!named.capture.join("argv.1").exists());
    assert!(stdout.contains("--name Cleo"), "{stdout}");
    assert!(stdout.contains("CLANKER_ID=chloe"), "{stdout}");

    let bare = Harness::new();
    let output = bare
        .command(&bare.home)
        .args(["codex", "--dry-run"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!bare.capture.join("resolver.calls").exists());
    assert!(!bare.capture.join("argv.1").exists());
    assert!(!stdout.contains("CLANKER_ID"), "{stdout}");
}

#[test]
fn restart_retains_verified_identity_and_original_name() {
    let harness = Harness::new();
    let output = harness
        .command(&harness.home)
        .args(["codex", "--name", "Cleo"])
        .env("TRIGGER_ONE_RESTART", "1")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for iteration in [1, 2] {
        assert_name_pair(&harness.argv(iteration), "Cleo");
        assert_eq!(harness.captured("clanker_id", iteration), "chloe");
    }
    assert_eq!(&harness.argv(2)[..2], ["resume", "--last"]);
}

#[test]
fn explicit_name_coexists_with_resume_and_fork_subcommands() {
    let resume = Harness::new();
    let output = resume
        .command(&resume.home)
        .args(["codex", "--name", "Cleo", "--resume", "thread-1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = resume.argv(1);
    assert_eq!(&args[..2], ["resume", "thread-1"]);
    assert_name_pair(&args, "Cleo");
    assert_eq!(resume.captured("clanker_id", 1), "chloe");

    let fork = Harness::new();
    let output = fork
        .command(&fork.home)
        .args(["codex", "--name", "Cleo", "--fork"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = fork.argv(1);
    assert_eq!(args.first().map(String::as_str), Some("fork"));
    assert_name_pair(&args, "Cleo");
    assert_eq!(fork.captured("clanker_id", 1), "chloe");
}

fn write_ucf_lineage(harness: &Harness, name: &str) -> (PathBuf, serde_json::Value) {
    let session_dir = harness.data_home.join("unleash/sessions");
    fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join(format!("{name}.ucf.jsonl"));
    let header = json!({
        "type": "session",
        "ucf_version": "1.0.0",
        "session_id": format!("{name}-session"),
        "created_at": "2026-07-22T00:00:00Z",
        "updated_at": "2026-07-22T00:01:00Z",
        "source_cli": "foreign-fixture",
        "source_version": "1.0.0",
        "title": "Lineage fixture",
        "parent_session_id": "parent-session",
        "extensions": {"foreign": {"opaque": [1, 2, 3]}}
    });
    let message = json!({
        "type": "message",
        "id": "message-1",
        "timestamp": "2026-07-22T00:01:00Z",
        "completed_at": "2026-07-22T00:01:01Z",
        "role": "user",
        "content": [{"type": "text", "text": "lineage"}],
        "metadata": {},
        "extensions": {"foreign": {"message": true}}
    });
    fs::write(
        &path,
        format!(
            "{}\n{}\n",
            serde_json::to_string(&header).unwrap(),
            serde_json::to_string(&message).unwrap()
        ),
    )
    .unwrap();
    (path, header)
}

#[test]
fn ucf_resume_keeps_lineage_unchanged_alongside_name_and_identity_env() {
    let harness = Harness::new();
    let (path, expected_header) = write_ucf_lineage(&harness, "lineage");

    let output = harness
        .command(&harness.home)
        .args(["codex", "--name", "Cleo", "--ucf", "lineage"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = harness.argv(1);
    assert_name_pair(&args, "Cleo");
    assert!(
        args.iter().any(|arg| arg == "resume"),
        "missing resume argument in {args:?}"
    );
    assert_eq!(harness.captured("clanker_id", 1), "chloe");
    let actual_header: serde_json::Value =
        serde_json::from_str(fs::read_to_string(path).unwrap().lines().next().unwrap()).unwrap();
    assert_eq!(actual_header, expected_header);
}

#[test]
fn crossload_keeps_source_ucf_bytes_unchanged_alongside_name_and_identity_env() {
    let harness = Harness::new();
    let (path, _) = write_ucf_lineage(&harness, "crossload-lineage");
    let before = fs::read(&path).unwrap();

    let output = harness
        .command(&harness.home)
        .args([
            "codex",
            "--name",
            "Cleo",
            "--crossload",
            "ucf:crossload-lineage",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = harness.argv(1);
    assert_name_pair(&args, "Cleo");
    assert!(args.iter().any(|arg| arg == "resume"));
    assert_eq!(harness.captured("clanker_id", 1), "chloe");
    assert_eq!(fs::read(path).unwrap(), before);
}
