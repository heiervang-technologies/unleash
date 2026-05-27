//! Local-tmux pane discovery and focus.
//!
//! Provides two things:
//! - `discover_running_panes()`: parse `director list` to find agent panes
//!   running on this host. Used by the TUI Profiles screen to show a
//!   "● running on %X" chip on rows whose directory matches a live pane.
//! - `focus_pane(pane_id)`: switch tmux focus to a pane AND bring its
//!   hosting terminal window to the front on the desktop (Hyprland).
//!
//! `unleash pane focus <id>` exposes the focus action as a CLI subcommand
//! so any clickable surface (TUI hotkey, OSC8 hyperlink emitted by other
//! unleash commands, desktop launcher) can trigger it uniformly.

use std::io;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct RunningPane {
    /// tmux pane id, e.g. "%12"
    pub pane_id: String,
    pub session: String,
    /// User-facing pane label from `director list`. Empty string if unset.
    pub name: String,
    /// Agent binary in the pane (claude / codex / agy / pi / ...).
    pub agent: String,
    /// Resolved absolute directory the pane is running in. director prints
    /// it tilde-expanded; we normalize back to absolute for comparison.
    pub directory: PathBuf,
}

/// Run `director list` and parse its tabular output.
///
/// Output format (from `director` source):
///   PANE   SESSION  NAME  AGENT  PERSONA  DIRECTORY  STATUS  AGE
///   %12    work     name  claude -        ~/ht/repo  ● running 3m
///
/// Returns an empty vec on any failure (director missing, parse error, etc.).
/// The TUI treats absence as "no running panes here" which is the right
/// default — never error a chip out of the user's way.
pub fn discover_running_panes() -> Vec<RunningPane> {
    let Ok(output) = Command::new("director").arg("list").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_director_list(&text)
}

fn parse_director_list(text: &str) -> Vec<RunningPane> {
    let mut panes = Vec::new();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let home_str = home.to_string_lossy().into_owned();

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with("PANE")
            || trimmed.starts_with("-")
            || trimmed.starts_with("=")
        {
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        // Minimum required columns: PANE SESSION NAME AGENT PERSONA DIRECTORY
        if cols.len() < 6 {
            continue;
        }
        if !cols[0].starts_with('%') {
            continue;
        }
        let directory_raw = cols[5];
        let directory = if let Some(rest) = directory_raw.strip_prefix("~/") {
            PathBuf::from(format!("{}/{}", home_str, rest))
        } else if directory_raw == "~" {
            home.clone()
        } else {
            PathBuf::from(directory_raw)
        };
        panes.push(RunningPane {
            pane_id: cols[0].to_string(),
            session: cols[1].to_string(),
            name: if cols[2] == "-" {
                String::new()
            } else {
                cols[2].to_string()
            },
            agent: cols[3].to_string(),
            directory,
        });
    }
    panes
}

/// Switch tmux focus to `pane_id` and raise the hosting terminal window
/// on the Hyprland desktop. Returns the human-readable target description
/// so the caller can confirm to the user.
pub fn focus_pane(pane_id: &str) -> io::Result<String> {
    if !pane_id.starts_with('%') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("pane id must start with '%': got '{pane_id}'"),
        ));
    }

    // Resolve session + client info up front so we fail fast on bad pane.
    let session = tmux_query(pane_id, "#{session_name}")?;
    if session.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("tmux pane '{pane_id}' has no session — not on this host?"),
        ));
    }

    // 1) Move tmux's currently-attached clients (if any) to this pane.
    let _ = Command::new("tmux")
        .args(["select-pane", "-t", pane_id])
        .status();
    let _ = Command::new("tmux")
        .args(["switch-client", "-t", &session])
        .status();

    // 2) Raise the desktop window holding a tmux client for this session.
    //    Match by PID of any tmux client attached to the session.
    let client_pids = tmux_client_pids_for_session(&session);
    if !client_pids.is_empty() {
        let _ = focus_hyprland_window_by_pid(&client_pids);
    }

    Ok(format!("{pane_id} (session {session})"))
}

fn tmux_query(pane_id: &str, fmt: &str) -> io::Result<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "-t", pane_id, fmt])
        .output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(io::Error::other(format!(
            "tmux display-message -t {pane_id}: {err}"
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn tmux_client_pids_for_session(session: &str) -> Vec<u32> {
    let Ok(output) = Command::new("tmux")
        .args([
            "list-clients",
            "-t",
            session,
            "-F",
            "#{client_pid}",
        ])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<u32>().ok())
        .collect()
}

/// Walk parent PIDs of `pid` up the process tree until we hit one that owns
/// a Hyprland window. Terminal emulators spawn a shell which spawns tmux —
/// the tmux client_pid is the shell, not the terminal window, so we have to
/// climb.
fn focus_hyprland_window_by_pid(client_pids: &[u32]) -> io::Result<()> {
    let clients_json = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()?;
    if !clients_json.status.success() {
        return Err(io::Error::other("hyprctl clients -j failed"));
    }
    let json: serde_json::Value =
        serde_json::from_slice(&clients_json.stdout).map_err(io::Error::other)?;
    let arr = json.as_array().ok_or_else(|| {
        io::Error::other("hyprctl clients output is not a JSON array")
    })?;

    // Build pid -> address map once.
    let mut pid_to_addr: std::collections::HashMap<u32, String> =
        std::collections::HashMap::new();
    for c in arr {
        if let (Some(pid), Some(addr)) = (
            c.get("pid").and_then(|v| v.as_u64()),
            c.get("address").and_then(|v| v.as_str()),
        ) {
            pid_to_addr.insert(pid as u32, addr.to_string());
        }
    }

    for &start in client_pids {
        if let Some(addr) = find_window_address_for_pid(start, &pid_to_addr) {
            let _ = Command::new("hyprctl")
                .args(["dispatch", "focuswindow", &format!("address:{}", addr)])
                .status();
            return Ok(());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no Hyprland window found for any tmux client pid",
    ))
}

fn find_window_address_for_pid(
    start: u32,
    pid_to_addr: &std::collections::HashMap<u32, String>,
) -> Option<String> {
    let mut pid = start;
    for _ in 0..16 {
        if let Some(addr) = pid_to_addr.get(&pid) {
            return Some(addr.clone());
        }
        pid = match parent_pid(pid) {
            Some(0) | None => return None,
            Some(p) => p,
        };
    }
    None
}

fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // /proc/<pid>/stat format: pid (comm) state ppid ...
    // comm can contain spaces and parens, so split on the last ')' first.
    let close = stat.rfind(')')?;
    let after = &stat[close + 1..];
    let parts: Vec<&str> = after.split_whitespace().collect();
    parts.get(1)?.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_director_list_skips_header_and_dashes() {
        let sample = "\
PANE   SESSION          NAME                     AGENT      PERSONA  DIRECTORY                STATUS     AGE
%2     Work             oliver                   claude     -        ~/Work                   ● running 1h
%3     Work             -                        agy        -        ~/Work                   ○ idle   1h
%5     cloud            snoop-kube               claude     cloud    ~/ht/cloud               ● running 3m";
        let panes = parse_director_list(sample);
        assert_eq!(panes.len(), 3);
        assert_eq!(panes[0].pane_id, "%2");
        assert_eq!(panes[0].name, "oliver");
        assert_eq!(panes[0].agent, "claude");
        // "-" becomes empty name
        assert_eq!(panes[1].name, "");
        // ~/ resolves to home
        let home = dirs::home_dir().unwrap();
        assert_eq!(panes[2].directory, home.join("ht").join("cloud"));
    }

    #[test]
    fn test_parse_director_list_ignores_empty() {
        let panes = parse_director_list("");
        assert!(panes.is_empty());
    }

    #[test]
    fn test_parse_director_list_skips_non_percent_rows() {
        // Any future banner / footer line that doesn't start with `%` is skipped.
        let sample = "\
INFO: 4 panes
%12    foo  bar  claude - ~/x";
        let panes = parse_director_list(sample);
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].pane_id, "%12");
    }

    #[test]
    fn test_focus_pane_rejects_non_percent_id() {
        let err = focus_pane("12").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
