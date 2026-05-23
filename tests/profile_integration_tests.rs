use std::process::Command;

fn verify_dry_run_output(stdout: &str, expected_bin: &str) {
    let dry_run_line = stdout
        .lines()
        .find(|line| line.starts_with("Would execute:"))
        .expect("Should contain 'Would execute:' line");

    let tokens: Vec<&str> = dry_run_line.split_whitespace().collect();
    assert!(
        tokens.len() >= 3,
        "Line should have at least 'Would execute:' and binary name: {:?}",
        dry_run_line
    );

    let executed_bin = tokens[2];
    assert!(
        executed_bin == expected_bin || executed_bin.ends_with(&format!("/{}", expected_bin)),
        "Expected executed binary to be or end with '{}', but got '{}' in line: '{}'",
        expected_bin,
        executed_bin,
        dry_run_line
    );
}

#[test]
fn test_agy_profile_dry_run() {
    let bin_path = env!("CARGO_BIN_EXE_unleash");
    let output = Command::new(bin_path)
        .args(["agy", "--dry-run"])
        .output()
        .expect("failed to execute unleash");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    verify_dry_run_output(&stdout, "agy");
}

#[test]
fn test_hermes_profile_dry_run() {
    let bin_path = env!("CARGO_BIN_EXE_unleash");
    let output = Command::new(bin_path)
        .args(["hermes", "--dry-run"])
        .output()
        .expect("failed to execute unleash");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    verify_dry_run_output(&stdout, "hermes");
}
