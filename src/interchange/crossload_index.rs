//! Persistent index of crossload operations, keyed by (source_cli, source_id,
//! target_cli). Used to make `unleash <agent> --crossload <sess>` idempotent:
//! re-crossloading an already-imported session reuses the cached target session
//! instead of creating a duplicate.
//!
//! Storage: a single JSON file under `$XDG_DATA_HOME/unleash/crossload-index.json`
//! (or `~/.local/share/unleash/crossload-index.json` by default). Each entry
//! records the target session id plus the target file path, so stale entries
//! (target session deleted on disk) can be detected and re-injected.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrossloadIndex {
    /// Keyed by `<source_cli>:<source_id>-><target_cli>` for single-file
    /// compatibility with older plaintext inspection.
    #[serde(default)]
    entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub target_session_id: String,
    /// Absolute path to the target file/db row we wrote. Empty for DB-backed
    /// targets (OpenCode) where there is no single representative file.
    pub target_path: String,
    /// The `updated_at` timestamp of the source session at the time of crossload.
    /// Used to invalidate the cache if the source session receives new messages.
    #[serde(default)]
    pub source_updated_at: Option<String>,
}

fn key(source_cli: &str, source_id: &str, target_cli: &str) -> String {
    format!("{source_cli}:{source_id}->{target_cli}")
}

fn index_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("unleash").join("crossload-index.json"))
}

/// Load the on-disk index, or an empty one if the file is missing or malformed.
pub fn load() -> CrossloadIndex {
    load_from(index_path().as_deref())
}

pub fn load_from(path: Option<&Path>) -> CrossloadIndex {
    let Some(path) = path else {
        return CrossloadIndex::default();
    };
    match std::fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => CrossloadIndex::default(),
    }
}

/// Persist the index to disk, creating parent dirs as needed.
pub fn save(index: &CrossloadIndex) -> io::Result<()> {
    let Some(path) = index_path() else {
        return Ok(());
    };
    save_to(index, &path)
}

pub fn save_to(index: &CrossloadIndex, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(index)?;
    std::fs::write(path, json)
}

impl CrossloadIndex {
    pub fn lookup(&self, source_cli: &str, source_id: &str, target_cli: &str) -> Option<&Entry> {
        self.entries.get(&key(source_cli, source_id, target_cli))
    }

    pub fn record(
        &mut self,
        source_cli: &str,
        source_id: &str,
        target_cli: &str,
        target_session_id: String,
        target_path: String,
        source_updated_at: Option<String>,
    ) {
        self.entries.insert(
            key(source_cli, source_id, target_cli),
            Entry {
                target_session_id,
                target_path,
                source_updated_at,
            },
        );
    }

    pub fn remove(&mut self, source_cli: &str, source_id: &str, target_cli: &str) {
        self.entries.remove(&key(source_cli, source_id, target_cli));
    }
}

/// True if the entry still points at a live target. File-backed targets check
/// existence on disk; DB-backed targets (empty path) are always considered live
/// — the caller is expected to have a richer check if needed.
pub fn entry_is_live(entry: &Entry) -> bool {
    if entry.target_path.is_empty() {
        return true;
    }
    Path::new(&entry.target_path).exists()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DoctorStatus {
    Live,
    TargetGone,
    SourceUpdated,
    SourceGone,
}

impl std::fmt::Display for DoctorStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            DoctorStatus::Live => "live",
            DoctorStatus::TargetGone => "target-gone",
            DoctorStatus::SourceUpdated => "source-updated",
            DoctorStatus::SourceGone => "source-gone",
        };
        write!(f, "{s}")
    }
}

#[derive(Serialize)]
pub struct DoctorReportItem {
    pub status: DoctorStatus,
    pub source_cli: String,
    pub source_id: String,
    pub target_cli: String,
    pub target_session_id: String,
    pub target_path: String,
    pub source_updated_at: Option<String>,
    pub reason: String,
}

fn parse_key(key: &str) -> Option<(String, String, String)> {
    let (src, target_cli) = key.split_once("->")?;
    let (source_cli, source_id) = src.split_once(':')?;
    Some((source_cli.to_string(), source_id.to_string(), target_cli.to_string()))
}

pub fn run_doctor(json: bool, gc: bool) -> io::Result<()> {
    let mut index = load();
    let _reports = run_doctor_impl(json, gc, &mut index, |cli, id| {
        crate::interchange::sessions::find_session(&format!("{cli}:{id}"))
    })?;
    if gc {
        save(&index)?;
    }
    Ok(())
}

pub fn run_doctor_impl<F>(
    json: bool,
    gc: bool,
    index: &mut CrossloadIndex,
    mut find_src: F,
) -> io::Result<Vec<DoctorReportItem>>
where
    F: FnMut(&str, &str) -> Option<crate::interchange::sessions::SessionInfo>,
{
    let mut reports = Vec::new();
    let mut live_count = 0;
    let mut target_gone_count = 0;
    let mut source_updated_count = 0;
    let mut source_gone_count = 0;

    let mut keys_to_remove = Vec::new();

    for (k, entry) in &index.entries {
        let Some((source_cli, source_id, target_cli)) = parse_key(k) else {
            continue;
        };

        let source_opt = find_src(&source_cli, &source_id);

        let (status, reason) = if let Some(source) = source_opt {
            if entry.source_updated_at.as_deref() != Some(&source.updated_at) {
                (DoctorStatus::SourceUpdated, "source session modified")
            } else if !entry.target_path.is_empty() && !Path::new(&entry.target_path).exists() {
                (DoctorStatus::TargetGone, "target file missing")
            } else {
                (DoctorStatus::Live, "")
            }
        } else {
            keys_to_remove.push(k.clone());
            (DoctorStatus::SourceGone, "source session not found")
        };

        match status {
            DoctorStatus::Live => live_count += 1,
            DoctorStatus::TargetGone => target_gone_count += 1,
            DoctorStatus::SourceUpdated => source_updated_count += 1,
            DoctorStatus::SourceGone => source_gone_count += 1,
        }

        reports.push(DoctorReportItem {
            status,
            source_cli,
            source_id,
            target_cli,
            target_session_id: entry.target_session_id.clone(),
            target_path: entry.target_path.clone(),
            source_updated_at: entry.source_updated_at.clone(),
            reason: reason.to_string(),
        });
    }

    if json {
        let serialized = serde_json::to_string_pretty(&reports)?;
        println!("{}", serialized);
    } else {
        println!("{:<15} {:<30} {:<30} REASON", "STATUS", "SOURCE", "TARGET");
        for r in &reports {
            println!(
                "{:<15} {:<30} {:<30} {}",
                r.status.to_string(),
                format!("{}:{}", r.source_cli, r.source_id),
                format!("{}:{}", r.target_cli, r.target_session_id),
                r.reason
            );
        }
        println!(
            "\n{} live, {} target-gone, {} source-updated, {} source-gone",
            live_count, target_gone_count, source_updated_count, source_gone_count
        );
    }

    if gc {
        let removed_count = keys_to_remove.len();
        for k in keys_to_remove {
            index.entries.remove(&k);
        }
        if json {
            eprintln!("Removed {} source-gone entries.", removed_count);
        } else {
            println!("Removed {} source-gone entries.", removed_count);
        }
    }

    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_through_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("crossload-index.json");

        let mut idx = CrossloadIndex::default();
        idx.record(
            "codex",
            "abc-123",
            "claude",
            "sess-1".into(),
            "/tmp/sess-1.jsonl".into(),
            None,
        );
        idx.record(
            "pi",
            "xyz-789",
            "gemini",
            "sess-2".into(),
            "/tmp/chats/sess-2.json".into(),
            None,
        );
        save_to(&idx, &path).unwrap();

        let reloaded = load_from(Some(&path));
        assert_eq!(reloaded.entries.len(), 2);

        let e = reloaded.lookup("codex", "abc-123", "claude").unwrap();
        assert_eq!(e.target_session_id, "sess-1");
        assert_eq!(e.target_path, "/tmp/sess-1.jsonl");

        // Absent entries return None.
        assert!(reloaded.lookup("codex", "abc-123", "gemini").is_none());
        assert!(reloaded.lookup("claude", "abc-123", "claude").is_none());
    }

    #[test]
    fn missing_file_yields_empty_index() {
        let idx = load_from(Some(Path::new("/nonexistent/path/xyz")));
        assert!(idx.entries.is_empty());
    }

    #[test]
    fn malformed_file_yields_empty_index() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.json");
        std::fs::write(&path, "this is not JSON").unwrap();
        let idx = load_from(Some(&path));
        assert!(idx.entries.is_empty());
    }

    #[test]
    fn test_remove() {
        let mut idx = CrossloadIndex::default();
        idx.record("a", "b", "c", "d".into(), String::new(), None);
        assert!(idx.lookup("a", "b", "c").is_some());
        idx.remove("a", "b", "c");
        assert!(idx.lookup("a", "b", "c").is_none());
    }

    #[test]
    fn test_record_overwrites() {
        let mut idx = CrossloadIndex::default();
        idx.record("a", "b", "c", "first".into(), "/tmp/one".into(), None);
        idx.record("a", "b", "c", "second".into(), "/tmp/two".into(), None);
        let entry = idx.lookup("a", "b", "c").unwrap();
        assert_eq!(entry.target_session_id, "second");
        assert_eq!(entry.target_path, "/tmp/two");
    }

    #[test]
    fn entry_is_live_file_existence() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real.jsonl");
        std::fs::write(&real, b"x").unwrap();

        let live = Entry {
            target_session_id: "s".into(),
            target_path: real.to_string_lossy().into(),
            source_updated_at: None,
        };
        let dead = Entry {
            target_session_id: "s".into(),
            target_path: "/nonexistent/ghost.jsonl".into(),
            source_updated_at: None,
        };
        let db = Entry {
            target_session_id: "s".into(),
            target_path: String::new(),
            source_updated_at: None,
        };

        assert!(entry_is_live(&live));
        assert!(!entry_is_live(&dead));
        assert!(entry_is_live(&db));
    }

    #[test]
    fn test_doctor_classification_and_gc() {
        use crate::interchange::sessions::SessionInfo;

        let tmp = tempfile::tempdir().unwrap();
        let target_file_real = tmp.path().join("real_target.jsonl");
        std::fs::write(&target_file_real, b"x").unwrap();

        let mut idx = CrossloadIndex::default();

        // 1. Live entry (file-backed target exists, source exists, updated_at matches)
        idx.record(
            "claude",
            "source-live",
            "pi",
            "target-live".into(),
            target_file_real.to_string_lossy().to_string(),
            Some("2026-05-27T12:00:00Z".into()),
        );

        // 2. Target-gone entry (file-backed target missing, source exists, updated_at matches)
        idx.record(
            "claude",
            "source-tgone",
            "pi",
            "target-tgone".into(),
            "/nonexistent/target.jsonl".into(),
            Some("2026-05-27T12:00:00Z".into()),
        );

        // 3. Source-updated entry (file-backed target exists, source exists, but updated_at drifted)
        idx.record(
            "claude",
            "source-supd",
            "pi",
            "target-supd".into(),
            target_file_real.to_string_lossy().to_string(),
            Some("2026-05-27T11:00:00Z".into()), // older timestamp
        );

        // 4. Source-gone entry (source does not exist)
        idx.record(
            "claude",
            "source-sgone",
            "pi",
            "target-sgone".into(),
            target_file_real.to_string_lossy().to_string(),
            Some("2026-05-27T12:00:00Z".into()),
        );

        // Mock resolver
        let mock_sessions = [
            SessionInfo {
                cli: "claude".into(),
                id: "source-live".into(),
                name: None,
                title: None,
                directory: "/tmp".into(),
                path: std::path::PathBuf::from("/tmp/nope1.jsonl"),
                updated_at: "2026-05-27T12:00:00Z".into(),
                message_count: None,
            },
            SessionInfo {
                cli: "claude".into(),
                id: "source-tgone".into(),
                name: None,
                title: None,
                directory: "/tmp".into(),
                path: std::path::PathBuf::from("/tmp/nope2.jsonl"),
                updated_at: "2026-05-27T12:00:00Z".into(),
                message_count: None,
            },
            SessionInfo {
                cli: "claude".into(),
                id: "source-supd".into(),
                name: None,
                title: None,
                directory: "/tmp".into(),
                path: std::path::PathBuf::from("/tmp/nope3.jsonl"),
                updated_at: "2026-05-27T12:00:00Z".into(), // source has newer timestamp
                message_count: None,
            },
        ];

        let find_src = |cli: &str, id: &str| {
            mock_sessions.iter().find(|s| s.cli == cli && s.id == id).cloned()
        };

        // Run doctor report (without gc)
        let mut idx_clone = idx.clone();
        let reports = run_doctor_impl(false, false, &mut idx_clone, find_src).unwrap();

        assert_eq!(reports.len(), 4);

        let r_live = reports.iter().find(|r| r.source_id == "source-live").unwrap();
        assert_eq!(r_live.status, DoctorStatus::Live);
        assert_eq!(r_live.reason, "");

        let r_tgone = reports.iter().find(|r| r.source_id == "source-tgone").unwrap();
        assert_eq!(r_tgone.status, DoctorStatus::TargetGone);
        assert_eq!(r_tgone.reason, "target file missing");

        let r_supd = reports.iter().find(|r| r.source_id == "source-supd").unwrap();
        assert_eq!(r_supd.status, DoctorStatus::SourceUpdated);
        assert_eq!(r_supd.reason, "source session modified");

        let r_sgone = reports.iter().find(|r| r.source_id == "source-sgone").unwrap();
        assert_eq!(r_sgone.status, DoctorStatus::SourceGone);
        assert_eq!(r_sgone.reason, "source session not found");

        // Confirm no entries were removed during dry run
        assert_eq!(idx_clone.entries.len(), 4);

        // Run doctor report (with gc)
        let reports_gc = run_doctor_impl(false, true, &mut idx, find_src).unwrap();
        assert_eq!(reports_gc.len(), 4);

        // Confirm only source-gone entries were removed (1 entry removed, 3 left)
        assert_eq!(idx.entries.len(), 3);
        assert!(idx.lookup("claude", "source-live", "pi").is_some());
        assert!(idx.lookup("claude", "source-tgone", "pi").is_some());
        assert!(idx.lookup("claude", "source-supd", "pi").is_some());
        assert!(idx.lookup("claude", "source-sgone", "pi").is_none());
    }
}
