use std::process::Command;

#[test]
fn test_unleash_sessions_doctor_cli() {
    let bin_path = env!("CARGO_BIN_EXE_unleash");
    let tmp = tempfile::tempdir().unwrap();
    let unleash_dir = tmp.path().join("unleash");
    std::fs::create_dir_all(&unleash_dir).unwrap();

    // Create a mock crossload-index.json. Since all sources will be gone,
    // they should be classified as source-gone.
    let index_data = serde_json::json!({
        "entries": {
            "claude:nonexistent-source->pi": {
                "target_session_id": "target-123",
                "target_path": "/tmp/nonexistent-target.jsonl",
                "source_updated_at": "2026-05-27T12:00:00Z"
            }
        }
    });
    std::fs::write(
        unleash_dir.join("crossload-index.json"),
        serde_json::to_string_pretty(&index_data).unwrap(),
    )
    .unwrap();

    // Run doctor command without gc (should report source-gone)
    let output = Command::new(bin_path)
        .args(["sessions", "doctor", "--json"])
        .env("XDG_DATA_HOME", tmp.path())
        .output()
        .expect("failed to execute unleash");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let reports: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(reports.is_array());
    let arr = reports.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["status"], "source-gone");
    assert_eq!(arr[0]["source_id"], "nonexistent-source");

    // Index file should still have the entry
    let content = std::fs::read_to_string(unleash_dir.join("crossload-index.json")).unwrap();
    assert!(content.contains("nonexistent-source"));

    // Run doctor command with gc (should remove the entry)
    let output_gc = Command::new(bin_path)
        .args(["sessions", "doctor", "--gc", "--json"])
        .env("XDG_DATA_HOME", tmp.path())
        .output()
        .expect("failed to execute unleash");

    assert!(output_gc.status.success());
    let stderr = String::from_utf8_lossy(&output_gc.stderr);
    assert!(stderr.contains("Removed 1 source-gone entries."));

    // Index file should now have 0 entries
    let content_after = std::fs::read_to_string(unleash_dir.join("crossload-index.json")).unwrap();
    let reloaded: serde_json::Value = serde_json::from_str(&content_after).unwrap();
    let entries = reloaded["entries"].as_object().unwrap();
    assert!(entries.is_empty());
}
