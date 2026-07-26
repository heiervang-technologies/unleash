//! OpenAI-compatible HTTP gateway for Unleash-managed agent instances.
//!
//! The primary execution mode keeps a headful harness in a PTY, injects one
//! serialized API turn at a time, and projects appended native history through
//! OpenAI response shapes. The secondary headless mode starts one resumed
//! process per turn but shares the same history and HTTP compatibility layer.

mod api;
mod history;
mod pty;

use crate::agents::AgentType;
use crate::config::ProfileManager;
use crate::interchange::CliFormat;
use history::{HistoryTracker, InstanceIdentity, TurnResult, TurnUpdate};
use std::io::{self, Write};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, OwnedMutexGuard};

pub struct ServeOptions {
    pub profile: String,
    pub host: IpAddr,
    pub port: u16,
    pub instance_name: Option<String>,
    pub model: Option<String>,
    pub session: Option<String>,
    pub headless: bool,
    pub unsafe_permissions: bool,
    pub api_key: Option<String>,
    pub allow_remote: bool,
    pub turn_timeout: Duration,
    pub agent_args: Vec<String>,
}

#[derive(Clone)]
pub(crate) enum Driver {
    Headful {
        input: Arc<pty::HeadfulInput>,
        tracker: Arc<Mutex<HistoryTracker>>,
        timeout: Duration,
    },
    Headless {
        executable: PathBuf,
        base_args: Arc<Vec<String>>,
        tracker: Arc<Mutex<HistoryTracker>>,
        timeout: Duration,
    },
    #[cfg(test)]
    Test { result: TurnResult },
}

#[derive(Debug)]
pub(crate) enum TurnStreamEvent {
    Update(TurnUpdate),
    Complete(TurnResult),
    Error(String),
}

impl Driver {
    pub(crate) fn start_turn(
        &self,
        prompt: String,
        gate: OwnedMutexGuard<()>,
    ) -> mpsc::Receiver<TurnStreamEvent> {
        let (sender, receiver) = mpsc::channel(64);
        let driver = self.clone();
        tokio::task::spawn_blocking(move || {
            let _gate = gate;
            let result = driver.run_turn(&prompt, |update| {
                let _ = sender.blocking_send(TurnStreamEvent::Update(update));
            });
            let event = match result {
                Ok(result) => TurnStreamEvent::Complete(result),
                Err(error) => TurnStreamEvent::Error(error),
            };
            let _ = sender.blocking_send(event);
        });
        receiver
    }

    fn run_turn<F>(&self, prompt: &str, emit: F) -> Result<TurnResult, String>
    where
        F: FnMut(TurnUpdate),
    {
        match self {
            Self::Headful {
                input,
                tracker,
                timeout,
            } => {
                let _turn = input
                    .turn
                    .lock()
                    .map_err(|_| "headful input lock was poisoned".to_string())?;
                let mut tracker = tracker
                    .lock()
                    .map_err(|_| "session tracker lock was poisoned".to_string())?;
                let baseline = tracker.capture_baseline();
                inject_prompt(input, prompt)?;
                let result = tracker.collect_completed_turn(prompt, &baseline, *timeout, emit);
                if result
                    .as_ref()
                    .is_err_and(|error| error.contains("did not complete within"))
                {
                    // The native process may still be mid-turn. Do not release
                    // the conversation gate and accept another prompt into an
                    // indeterminate session.
                    input.terminate();
                }
                result
            }
            Self::Headless {
                executable,
                base_args,
                tracker,
                timeout,
            } => {
                let mut tracker = tracker
                    .lock()
                    .map_err(|_| "session tracker lock was poisoned".to_string())?;
                let baseline = tracker.capture_baseline();
                let mut args = base_args.as_ref().clone();
                args.push("-p".to_string());
                args.push(prompt.to_string());
                if let Some(session_id) = tracker.native_session_id() {
                    args.push("--resume".to_string());
                    args.push(session_id.to_string());
                }

                let mut child = Command::new(executable)
                    .args(&args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .map_err(|error| format!("failed to start headless agent: {error}"))?;
                let started = Instant::now();
                let status = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break status,
                        Ok(None) if started.elapsed() < *timeout => {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        Ok(None) => {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err(format!(
                                "headless agent turn exceeded {} seconds",
                                timeout.as_secs()
                            ));
                        }
                        Err(error) => {
                            let _ = child.kill();
                            return Err(format!(
                                "failed while waiting for headless agent: {error}"
                            ));
                        }
                    }
                };
                if !status.success() {
                    return Err(format!(
                        "headless agent exited with {}",
                        status
                            .code()
                            .map_or_else(|| "a signal".to_string(), |code| code.to_string())
                    ));
                }
                tracker.collect_after_process(prompt, &baseline, emit)
            }
            #[cfg(test)]
            Self::Test { result } => {
                let mut emit = emit;
                emit(TurnUpdate::Text(result.text.clone()));
                Ok(result.clone())
            }
        }
    }
}

pub fn serve(options: ServeOptions) -> io::Result<()> {
    validate_network_options(&options)?;

    let manager = ProfileManager::new()?;
    let profile = manager.load_profile(&options.profile).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("could not load profile '{}': {error}", options.profile),
        )
    })?;
    let agent_type = profile
        .agent_type()
        .ok_or_else(|| io::Error::other("custom profiles need a declared native history format"))?;
    let (format, harness) = gateway_harness(&agent_type)?;
    let configured_model = options.model.clone().or(profile.defaults.model.clone());
    let instance_name = options
        .instance_name
        .clone()
        .unwrap_or_else(|| options.profile.clone());
    let identity = Arc::new(RwLock::new(InstanceIdentity::new(
        instance_name,
        configured_model.clone(),
        harness.to_string(),
        if options.headless {
            "headless"
        } else {
            "headful"
        },
        options.instance_name.is_some(),
    )));
    let tracker = Arc::new(Mutex::new(
        HistoryTracker::new(format, options.session.as_deref(), Arc::clone(&identity))
            .map_err(io::Error::other)?,
    ));

    let executable = std::env::current_exe()?;
    let mut child_args = vec![options.profile.clone()];
    if options.unsafe_permissions {
        // Explicitly override profiles whose defaults opt into safe mode.
        child_args.push("--yolo".to_string());
    } else {
        child_args.push("--safe".to_string());
    }
    if let Some(model) = &configured_model {
        child_args.push("--model".to_string());
        child_args.push(model.clone());
    }
    if !options.headless {
        if let Some(session_id) = tracker
            .lock()
            .map_err(|_| io::Error::other("session tracker lock was poisoned"))?
            .native_session_id()
            .map(String::from)
        {
            child_args.push("--resume".to_string());
            child_args.push(session_id);
        }
    }
    if let Some(name) = &options.instance_name {
        if matches!(agent_type, AgentType::Claude | AgentType::Clanker) {
            child_args.push("--name".to_string());
            child_args.push(name.clone());
        }
    }
    child_args.extend(options.agent_args.clone());

    let address = SocketAddr::new(options.host, options.port);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(address).await?;
        let bound_address = listener.local_addr()?;
        let shown_identity = identity.read().expect("identity poisoned").clone();
        eprintln!(
            "OpenAI-compatible gateway: http://{bound_address}/v1 (model: {})",
            shown_identity.model_id()
        );

        if options.headless {
            let driver = Driver::Headless {
                executable,
                base_args: Arc::new(child_args),
                tracker,
                timeout: options.turn_timeout,
            };
            api::run(listener, identity, driver, options.api_key, async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
        } else {
            let mut process = pty::spawn(&executable, &child_args, &[])?;
            let driver = Driver::Headful {
                input: Arc::clone(&process.input),
                tracker,
                timeout: options.turn_timeout,
            };
            let mut exited = process.take_exit_receiver();
            let serve_result = api::run(listener, identity, driver, options.api_key, async move {
                tokio::select! {
                    _ = &mut exited => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            })
            .await;
            process.terminate();
            serve_result
        }
    })
}

fn inject_prompt(input: &pty::HeadfulInput, prompt: &str) -> Result<(), String> {
    // Bracketed paste keeps newlines inside a single editor submission. Strip
    // terminal control characters so untrusted API text cannot terminate the
    // paste, signal the process, or inject control sequences into the harness.
    let sanitized = sanitize_prompt(prompt);
    let mut writer = input
        .writer
        .lock()
        .map_err(|_| "headful PTY writer lock was poisoned".to_string())?;
    writer
        .write_all(b"\x1b[200~")
        .and_then(|_| writer.write_all(sanitized.as_bytes()))
        .and_then(|_| writer.write_all(b"\x1b[201~\r"))
        .and_then(|_| writer.flush())
        .map_err(|error| format!("failed to submit prompt to headful agent: {error}"))
}

fn sanitize_prompt(prompt: &str) -> String {
    prompt
        .chars()
        .filter_map(|character| match character {
            '\n' | '\t' => Some(character),
            '\r' => None,
            character if character.is_control() => Some('�'),
            character => Some(character),
        })
        .collect()
}

fn validate_network_options(options: &ServeOptions) -> io::Result<()> {
    if options
        .api_key
        .as_deref()
        .is_some_and(|key| key.trim().is_empty())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--api-key and UNLEASH_API_KEY cannot be empty",
        ));
    }
    if !options.host.is_loopback() && !options.allow_remote {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "non-loopback binds require --allow-remote",
        ));
    }
    if !options.host.is_loopback()
        && options
            .api_key
            .as_deref()
            .is_none_or(|key| key.trim().is_empty())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "non-loopback binds require --api-key or UNLEASH_API_KEY",
        ));
    }
    if options.turn_timeout.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--turn-timeout-secs must be greater than zero",
        ));
    }
    Ok(())
}

fn gateway_harness(agent_type: &AgentType) -> io::Result<(CliFormat, &'static str)> {
    match agent_type {
        AgentType::Claude => Ok((CliFormat::ClaudeCode, "claude-code")),
        AgentType::Codex => Ok((CliFormat::Codex, "codex")),
        AgentType::Clanker => Ok((CliFormat::Codex, "clanker")),
        AgentType::Antigravity => Ok((CliFormat::GeminiCli, "antigravity")),
        AgentType::Gemini => Ok((CliFormat::GeminiCli, "gemini-cli")),
        AgentType::OpenCode => Ok((CliFormat::OpenCode, "opencode")),
        AgentType::Pi => Ok((CliFormat::Pi, "pi")),
        AgentType::Hermes => Ok((CliFormat::Hermes, "hermes")),
        AgentType::Unleash | AgentType::Custom(_) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "the gateway needs a harness with a supported native history parser",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::path::Path;

    fn options(host: IpAddr) -> ServeOptions {
        ServeOptions {
            profile: "claude".into(),
            host,
            port: 8787,
            instance_name: None,
            model: None,
            session: None,
            headless: false,
            unsafe_permissions: false,
            api_key: None,
            allow_remote: false,
            turn_timeout: Duration::from_secs(10),
            agent_args: Vec::new(),
        }
    }

    #[test]
    fn loopback_bind_needs_no_authentication() {
        assert!(validate_network_options(&options(Ipv4Addr::LOCALHOST.into())).is_ok());
        assert!(validate_network_options(&options(Ipv6Addr::LOCALHOST.into())).is_ok());
    }

    #[test]
    fn remote_bind_needs_explicit_opt_in_and_key() {
        let mut remote = options(Ipv4Addr::UNSPECIFIED.into());
        assert!(validate_network_options(&remote).is_err());
        remote.allow_remote = true;
        assert!(validate_network_options(&remote).is_err());
        remote.api_key = Some("secret".into());
        assert!(validate_network_options(&remote).is_ok());
    }

    #[test]
    fn configured_api_keys_cannot_be_empty() {
        let mut local = options(Ipv4Addr::LOCALHOST.into());
        local.api_key = Some("  ".into());
        assert!(validate_network_options(&local).is_err());
    }

    #[test]
    fn api_prompts_cannot_inject_terminal_controls() {
        assert_eq!(
            sanitize_prompt("one\r\n\u{1b}[201~\u{3}two\tthree"),
            "one\n�[201~�two\tthree"
        );
    }

    #[test]
    fn codex_turn_runs_through_pty_and_native_history() {
        let _environment = crate::test_env::lock();
        let temporary = tempfile::tempdir().unwrap();
        let session_dir = temporary.path().join("sessions/2026/07/26");
        std::fs::create_dir_all(&session_dir).unwrap();
        let history = session_dir.join("rollout-2026-07-26T10-00-00-pty-session.jsonl");
        let submitted_path = temporary.path().join("submitted.txt");
        let ready_path = temporary.path().join("ready");
        std::fs::write(
            &history,
            "{\"timestamp\":\"2026-07-26T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"pty-session\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.145.0\"}}\n",
        )
        .unwrap();
        let saved_home = std::env::var_os("CODEX_HOME");
        // SAFETY: guarded by the crate-wide environment lock and restored.
        unsafe { std::env::set_var("CODEX_HOME", temporary.path()) };

        let identity = Arc::new(RwLock::new(InstanceIdentity::new(
            "pty-codex".into(),
            Some("gpt-5.6".into()),
            "codex".into(),
            "headful",
            true,
        )));
        let tracker = Arc::new(Mutex::new(
            HistoryTracker::new(
                CliFormat::Codex,
                Some("codex:pty-session"),
                Arc::clone(&identity),
            )
            .unwrap(),
        ));
        let script = r#"
stty -echo
: > "$READY_PATH"
IFS= read -r submitted
printf '%s' "$submitted" > "$SUBMITTED_PATH"
printf '%s\n' \
'{"timestamp":"2026-07-26T10:01:00Z","type":"turn_context","payload":{"model":"gpt-5.6"}}' \
'{"timestamp":"2026-07-26T10:01:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"PTY turn"}]}}' \
'{"timestamp":"2026-07-26T10:01:02Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Projected through PTY."}]}}' \
'{"timestamp":"2026-07-26T10:01:03Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"Projected through PTY."}}' \
>> "$HISTORY_PATH"
"#;
        let args = vec!["-c".to_string(), script.to_string()];
        let environment = vec![
            (
                "HISTORY_PATH".to_string(),
                history.to_string_lossy().into_owned(),
            ),
            (
                "SUBMITTED_PATH".to_string(),
                submitted_path.to_string_lossy().into_owned(),
            ),
            (
                "READY_PATH".to_string(),
                ready_path.to_string_lossy().into_owned(),
            ),
        ];
        let process = pty::spawn(Path::new("/bin/sh"), &args, &environment).unwrap();
        for _ in 0..50 {
            if ready_path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(ready_path.exists(), "PTY fixture did not become ready");
        let driver = Driver::Headful {
            input: Arc::clone(&process.input),
            tracker,
            timeout: Duration::from_secs(5),
        };

        let result = driver.run_turn("PTY turn", |_| {}).unwrap();
        assert_eq!(result.text, "Projected through PTY.");
        assert!(std::fs::read_to_string(submitted_path)
            .unwrap()
            .contains("PTY turn"));
        assert_eq!(
            identity.read().unwrap().native_session_id.as_deref(),
            Some("pty-session")
        );
        process.terminate();

        match saved_home {
            Some(value) => {
                // SAFETY: guarded by the crate-wide environment lock.
                unsafe { std::env::set_var("CODEX_HOME", value) }
            }
            None => {
                // SAFETY: guarded by the crate-wide environment lock.
                unsafe { std::env::remove_var("CODEX_HOME") }
            }
        }
    }
}
