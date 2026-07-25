use serde_json::Value;
use std::io::Write;
use std::process::{Command, Output, Stdio};

const UI_FIXTURE: &str = include_str!("../src/stream/tests/fixtures/claude-ui-stream.jsonl");

fn run_stream(extra_args: &[&str], input: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_unleash"));
    command
        .args(["stream", "--harness", "claude-code"])
        .args(extra_args)
        .env_remove("AGENT_CMD")
        .env_remove("AGENT_UNLEASH")
        .env_remove("UNLEASH_POLYFILL_ACTIVE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn unleash stream");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write fixture");
    child.wait_with_output().expect("stream output")
}

fn json_lines(output: &Output) -> Vec<Value> {
    assert!(
        output.status.success(),
        "status={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("canonical JSON event"))
        .collect()
}

#[test]
fn public_stream_filter_exposes_attended_interactions_with_join_keys() {
    let output = run_stream(&[], UI_FIXTURE);
    let events = json_lines(&output);

    assert!(!events.is_empty());
    assert!(events.iter().all(|event| {
        event.get("session_id").and_then(Value::as_str) == Some("ui-1")
            && event.get("type").and_then(Value::as_str).is_some()
    }));
    assert!(events
        .iter()
        .any(|event| event["type"] == "interaction_request"));
    assert!(events.iter().any(|event| {
        event["type"] == "message"
            && event
                .pointer("/data/content/0/name")
                .and_then(Value::as_str)
                == Some("AskUserQuestion")
    }));
}

#[test]
fn public_stream_filter_headless_suppresses_only_ui_dispatch() {
    let events = json_lines(&run_stream(&["--headless"], UI_FIXTURE));

    assert!(events
        .iter()
        .all(|event| event["type"] != "interaction_request"));
    assert!(events.iter().any(|event| {
        event["type"] == "message"
            && event
                .pointer("/data/content/0/name")
                .and_then(Value::as_str)
                == Some("AskUserQuestion")
    }));
}

#[test]
fn public_stream_filter_rejects_unknown_harnesses() {
    let output = Command::new(env!("CARGO_BIN_EXE_unleash"))
        .args(["stream", "--harness", "imaginary"])
        .env_remove("AGENT_CMD")
        .env_remove("AGENT_UNLEASH")
        .env_remove("UNLEASH_POLYFILL_ACTIVE")
        .stdin(Stdio::null())
        .output()
        .expect("run unleash stream");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no stream adapter"));
}
