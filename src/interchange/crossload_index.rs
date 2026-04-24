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
    ) {
        self.entries.insert(
            key(source_cli, source_id, target_cli),
            Entry {
                target_session_id,
                target_path,
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
        );
        idx.record(
            "pi",
            "xyz-789",
            "gemini",
            "sess-2".into(),
            "/tmp/chats/sess-2.json".into(),
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
    fn remove_drops_entry() {
        let mut idx = CrossloadIndex::default();
        idx.record("a", "b", "c", "d".into(), String::new());
        assert!(idx.lookup("a", "b", "c").is_some());
        idx.remove("a", "b", "c");
        assert!(idx.lookup("a", "b", "c").is_none());
    }

    #[test]
    fn re_record_overwrites_in_place() {
        let mut idx = CrossloadIndex::default();
        idx.record("a", "b", "c", "first".into(), "/tmp/one".into());
        idx.record("a", "b", "c", "second".into(), "/tmp/two".into());
        let e = idx.lookup("a", "b", "c").unwrap();
        assert_eq!(e.target_session_id, "second");
        assert_eq!(e.target_path, "/tmp/two");
    }

    #[test]
    fn entry_is_live_file_existence() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real.jsonl");
        std::fs::write(&real, b"x").unwrap();

        let live = Entry {
            target_session_id: "s".into(),
            target_path: real.to_string_lossy().into(),
        };
        let dead = Entry {
            target_session_id: "s".into(),
            target_path: "/nonexistent/ghost.jsonl".into(),
        };
        let db = Entry {
            target_session_id: "s".into(),
            target_path: String::new(),
        };

        assert!(entry_is_live(&live));
        assert!(!entry_is_live(&dead));
        assert!(entry_is_live(&db));
    }
}
