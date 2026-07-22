use serde::Deserialize;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::Command;

pub(crate) const CLANKER_ID_ENV: &str = "CLANKER_ID";

#[derive(Debug, Deserialize)]
struct CharacterResolution {
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
    ok: bool,
    input: String,
    id: Option<String>,
}

/// Resolve an explicit character name through the configured target binary.
///
/// Unleash deliberately owns no character registry. The target Clanker Code
/// executable is the authority for aliases, collisions, and canonical ids.
pub(crate) fn resolve_clanker_id(
    agent_cmd: &Path,
    requested_name: &str,
    profile_env: &HashMap<String, String>,
) -> io::Result<String> {
    let output = Command::new(agent_cmd)
        .args([
            "character",
            "resolve",
            requested_name,
            "--json",
            "--materialize-builtin",
        ])
        .envs(profile_env)
        .output()
        .map_err(|err| {
            io::Error::new(
                err.kind(),
                format!(
                    "failed to run character resolver from {}: {err}",
                    agent_cmd.display()
                ),
            )
        })?;

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if detail.is_empty() {
            format!("character resolver exited with {}", output.status)
        } else {
            format!("character resolver exited with {}: {detail}", output.status)
        };
        return Err(io::Error::new(io::ErrorKind::InvalidInput, message));
    }

    let resolution: CharacterResolution =
        serde_json::from_slice(&output.stdout).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("character resolver returned invalid JSON: {err}"),
            )
        })?;
    if resolution.schema_version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "character resolver returned unsupported schemaVersion {}",
                resolution.schema_version
            ),
        ));
    }
    if !resolution.ok {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "character resolver returned ok=false with a successful exit status",
        ));
    }
    if resolution.input != requested_name {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "character resolver response did not echo the requested name",
        ));
    }

    resolution
        .id
        .filter(|id| !id.trim().is_empty() && id.trim() == id)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "character resolver did not return a canonical id",
            )
        })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn resolver_script(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("clanker");
        std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).unwrap();
        (dir, script)
    }

    #[test]
    fn resolves_alias_to_canonical_id_through_target_binary() {
        let (_dir, script) = resolver_script(
            r#"test "$1" = character
test "$2" = resolve
test "$3" = Cleo
test "$4" = --json
test "$5" = --materialize-builtin
printf '%s\n' '{"schemaVersion":1,"ok":true,"input":"Cleo","id":"chloe","displayName":"Chloe","manifestPath":"/tmp/character.json","matchKind":"explicit_alias"}'"#,
        );

        assert_eq!(
            resolve_clanker_id(&script, "Cleo", &HashMap::new()).unwrap(),
            "chloe"
        );
    }

    #[test]
    fn rejects_unresolved_or_malformed_identity() {
        let (_dir, unresolved) = resolver_script("echo not-found >&2\nexit 1");
        assert_eq!(
            resolve_clanker_id(&unresolved, "missing", &HashMap::new())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );

        let (_dir, malformed) = resolver_script("printf '%s\\n' '{\"ok\":true}'");
        assert_eq!(
            resolve_clanker_id(&malformed, "chloe", &HashMap::new())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
