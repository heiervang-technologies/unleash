# Conversation Hub Format Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build lossless round-trip conversation converters between all 4 CLI formats via an open hub format (.ucf.jsonl).

**Architecture:** Hub-and-spoke model with 4 converter modules (8 functions total). Each CLI gets `to_hub()` and `from_hub()`. The Hub schema is a superset of all CLI fields — universal core plus CLI-specific `extensions` objects. Semantic equality comparison (not byte-level) for round-trip verification.

**Tech Stack:** Rust, serde/serde_json for serialization, rusqlite for OpenCode SQLite, proptest for property-based testing

**Spec:** `docs/superpowers/specs/2026-03-29-conversation-hub-format-design.md`

---

## File Structure

```
src/
├── interchange/
│   ├── mod.rs              # Module exports, ConvertError type, CLI enum
│   ├── hub.rs              # Hub schema structs (Session, Message, ContentBlock, etc.)
│   ├── semantic_eq.rs      # Structured JSON comparison for round-trip testing
│   ├── claude.rs           # Claude Code JSONL <-> Hub
│   ├── codex.rs            # Codex JSONL <-> Hub
│   ├── gemini.rs           # Gemini CLI JSON <-> Hub
│   └── opencode.rs         # OpenCode SQLite+JSON <-> Hub
├── lib.rs                  # Add `mod interchange;`
Cargo.toml                  # Add rusqlite dependency
tests/
└── interchange/
    ├── fixtures/            # Sanitized real conversation samples
    │   ├── claude-sample.jsonl
    │   ├── codex-sample.jsonl
    │   ├── gemini-sample.json
    │   └── opencode-session.json
    ├── round_trip.rs        # Round-trip lossless tests (all 4 CLIs)
    ├── cross_cli.rs         # Cross-CLI conversion tests
    └── edge_cases.rs        # Interrupted, multimodal, empty, malformed
```

---

### Task 1: Hub Schema Structs

**Files:**
- Create: `src/interchange/mod.rs`
- Create: `src/interchange/hub.rs`
- Modify: `src/lib.rs` (add `mod interchange;`)
- Modify: `Cargo.toml` (add rusqlite)

- [ ] **Step 1: Add dependencies to Cargo.toml**

Add under `[dependencies]`:
```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

Add under `[dev-dependencies]`:
```toml
proptest = "1.5"
```

- [ ] **Step 2: Create `src/interchange/mod.rs`**

```rust
pub mod hub;
pub mod semantic_eq;
pub mod claude;
pub mod codex;
pub mod gemini;
pub mod opencode;

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliFormat {
    ClaudeCode,
    Codex,
    GeminiCli,
    OpenCode,
}

impl fmt::Display for CliFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClaudeCode => write!(f, "claude-code"),
            Self::Codex => write!(f, "codex"),
            Self::GeminiCli => write!(f, "gemini-cli"),
            Self::OpenCode => write!(f, "opencode"),
        }
    }
}

#[derive(Debug)]
pub enum ConvertError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Sqlite(rusqlite::Error),
    InvalidFormat(String),
    UnsupportedVersion(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::Sqlite(e) => write!(f, "SQLite error: {e}"),
            Self::InvalidFormat(msg) => write!(f, "Invalid format: {msg}"),
            Self::UnsupportedVersion(v) => write!(f, "Unsupported UCF version: {v}"),
        }
    }
}

impl From<std::io::Error> for ConvertError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}
impl From<serde_json::Error> for ConvertError {
    fn from(e: serde_json::Error) -> Self { Self::Json(e) }
}
impl From<rusqlite::Error> for ConvertError {
    fn from(e: rusqlite::Error) -> Self { Self::Sqlite(e) }
}
```

- [ ] **Step 3: Create `src/interchange/hub.rs`**

```rust
use serde::{Deserialize, Serialize};

pub const UCF_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HubRecord {
    #[serde(rename = "session")]
    Session(SessionHeader),
    #[serde(rename = "message")]
    Message(HubMessage),
    #[serde(rename = "event")]
    Event(HubEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    pub ucf_version: String,
    pub session_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub source_cli: String,
    pub source_version: String,
    #[serde(default)]
    pub project: Option<ProjectInfo>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub parent_session_id: Option<String>,
    #[serde(default)]
    pub extensions: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub directory: String,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub vcs: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub sha: Option<String>,
    #[serde(default)]
    pub origin_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubMessage {
    pub id: String,
    #[serde(default)]
    pub api_message_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub timestamp: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    pub role: String,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub metadata: MessageMetadata,
    #[serde(default)]
    pub extensions: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tokens: Option<TokenUsage>,
    #[serde(default)]
    pub tokens_cumulative: bool,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cache_creation: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub tool: u64,
    #[serde(default)]
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        display_name: Option<String>,
        #[serde(default)]
        description: Option<String>,
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: Vec<ContentBlock>,
        #[serde(default)]
        exit_code: Option<i32>,
        #[serde(default)]
        is_error: bool,
        #[serde(default)]
        interrupted: bool,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        duration_ms: Option<u64>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        truncated: bool,
    },

    #[serde(rename = "thinking")]
    Thinking {
        #[serde(default)]
        text: String,
        #[serde(default)]
        subject: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        signature: Option<String>,
        #[serde(default)]
        encrypted: bool,
        #[serde(default)]
        encryption_format: Option<String>,
        #[serde(default)]
        encrypted_data: Option<String>,
        #[serde(default)]
        timestamp: Option<String>,
    },

    #[serde(rename = "image")]
    Image {
        media_type: String,
        encoding: String,
        data: String,
        #[serde(default)]
        source_url: Option<String>,
    },

    #[serde(rename = "step_boundary")]
    StepBoundary {
        boundary: String,
        #[serde(default)]
        snapshot: Option<String>,
        #[serde(default)]
        finish_reason: Option<String>,
        #[serde(default)]
        cost: Option<f64>,
        #[serde(default)]
        tokens: Option<TokenUsage>,
    },

    #[serde(rename = "patch")]
    Patch {
        path: String,
        #[serde(default)]
        hash_before: Option<String>,
        #[serde(default)]
        hash_after: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubEvent {
    pub event_type: String,
    pub timestamp: String,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub extensions: serde_json::Value,
}
```

- [ ] **Step 4: Add module declaration to `src/lib.rs`**

Add after the existing `mod` declarations (around line 27):
```rust
mod interchange;
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: no errors

- [ ] **Step 6: Commit**

```bash
git add src/interchange/mod.rs src/interchange/hub.rs src/lib.rs Cargo.toml Cargo.lock
git commit -m "feat: add hub interchange format schema structs

Defines the unleash Conversation Format (.ucf.jsonl) types:
Session, Message, Event, ContentBlock (text, tool_use, tool_result,
thinking, image, step_boundary, patch), TokenUsage, and metadata."
```

---

### Task 2: Semantic Equality Framework

**Files:**
- Create: `src/interchange/semantic_eq.rs`

- [ ] **Step 1: Write the semantic equality comparator**

```rust
use serde_json::Value;

/// Compare two JSON values for semantic equality.
/// Returns Ok(()) if equal, Err(path_description) if different.
pub fn semantic_eq(a: &Value, b: &Value) -> Result<(), String> {
    semantic_eq_inner(a, b, "$")
}

fn semantic_eq_inner(a: &Value, b: &Value, path: &str) -> Result<(), String> {
    // Null vs missing is equivalent (handled at object level)
    match (a, b) {
        (Value::Null, Value::Null) => Ok(()),
        (Value::Bool(a), Value::Bool(b)) if a == b => Ok(()),
        (Value::Number(a), Value::Number(b)) => {
            // Float comparison to 6 decimal places
            let af = a.as_f64().unwrap_or(0.0);
            let bf = b.as_f64().unwrap_or(0.0);
            if (af - bf).abs() < 1e-6 || a == b {
                Ok(())
            } else {
                Err(format!("{path}: number mismatch: {a} != {b}"))
            }
        }
        (Value::String(a), Value::String(b)) if a == b => Ok(()),
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(format!("{path}: array length {len_a} != {len_b}",
                    len_a = a.len(), len_b = b.len()));
            }
            for (i, (av, bv)) in a.iter().zip(b.iter()).enumerate() {
                semantic_eq_inner(av, bv, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        (Value::Object(a), Value::Object(b)) => {
            // Check all keys in a exist in b (null == missing)
            for (k, av) in a {
                let bv = b.get(k).unwrap_or(&Value::Null);
                if av == &Value::Null && bv == &Value::Null {
                    continue;
                }
                semantic_eq_inner(av, bv, &format!("{path}.{k}"))?;
            }
            // Check keys in b not in a
            for (k, bv) in b {
                if !a.contains_key(k) && bv != &Value::Null {
                    return Err(format!("{path}.{k}: missing in original, present in result"));
                }
            }
            Ok(())
        }
        _ => Err(format!("{path}: type mismatch: {a_type} != {b_type}",
            a_type = type_name(a), b_type = type_name(b))),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_identical_values() {
        let v = json!({"a": 1, "b": "hello"});
        assert!(semantic_eq(&v, &v).is_ok());
    }

    #[test]
    fn test_key_order_irrelevant() {
        let a = json!({"a": 1, "b": 2});
        let b = json!({"b": 2, "a": 1});
        assert!(semantic_eq(&a, &b).is_ok());
    }

    #[test]
    fn test_null_vs_missing() {
        let a = json!({"a": 1, "b": null});
        let b = json!({"a": 1});
        assert!(semantic_eq(&a, &b).is_ok());
    }

    #[test]
    fn test_float_precision() {
        let a = json!({"x": 1.0000001});
        let b = json!({"x": 1.0000002});
        assert!(semantic_eq(&a, &b).is_ok());
    }

    #[test]
    fn test_value_mismatch() {
        let a = json!({"a": 1});
        let b = json!({"a": 2});
        assert!(semantic_eq(&a, &b).is_err());
    }

    #[test]
    fn test_array_order_matters() {
        let a = json!([1, 2, 3]);
        let b = json!([1, 3, 2]);
        assert!(semantic_eq(&a, &b).is_err());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test interchange::semantic_eq -- --nocapture 2>&1 | tail -10`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add src/interchange/semantic_eq.rs
git commit -m "feat: add semantic equality comparator for round-trip testing

Handles key ordering, null-vs-missing, float precision. Returns
detailed path descriptions on mismatch for debugging."
```

---

### Task 3: Claude Code Converter

**Files:**
- Create: `src/interchange/claude.rs`

- [ ] **Step 1: Write the Claude Code converter**

The converter handles Claude's JSONL format with its 12+ message types. `to_hub()` reads JSONL lines, maps each to HubRecord. `from_hub()` reconstructs Claude JSONL from Hub records.

Key mappings:
- Claude `type: "user"` with `role: "user"` -> Hub `Message { role: "user" }`
- Claude `type: "assistant"` -> Hub `Message { role: "assistant" }`
- Claude `content[].type: "thinking"` -> Hub `ContentBlock::Thinking`
- Claude `content[].type: "tool_use"` -> Hub `ContentBlock::ToolUse`
- Claude `tool_result` user messages -> Hub `Message` with `ContentBlock::ToolResult` (array content)
- Claude `type: "progress"|"system"|"file-history-snapshot"|"pr-link"|"custom-title"|...` -> Hub `Event`
- All Claude-specific fields -> `extensions.claude-code`

Implementation: read each JSONL line as `serde_json::Value`, match on `type` field, construct Hub record. Preserve ALL original fields in extensions for lossless round-trip.

```rust
use crate::interchange::hub::*;
use crate::interchange::ConvertError;
use serde_json::Value;
use std::io::{BufRead, Write, BufWriter};

pub fn to_hub<R: BufRead>(reader: R) -> Result<Vec<HubRecord>, ConvertError> {
    let mut records = Vec::new();
    let mut session_header_emitted = false;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        let val: Value = serde_json::from_str(&line)?;

        let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match msg_type {
            "user" | "assistant" => {
                if !session_header_emitted {
                    records.push(build_session_from_first_message(&val));
                    session_header_emitted = true;
                }
                records.push(HubRecord::Message(claude_message_to_hub(&val)?));
            }
            "system" | "progress" | "file-history-snapshot" | "pr-link"
            | "custom-title" | "agent-name" | "agent-color"
            | "last-prompt" | "queue-operation" => {
                if !session_header_emitted {
                    records.push(build_session_from_first_message(&val));
                    session_header_emitted = true;
                }
                records.push(HubRecord::Event(claude_event_to_hub(&val)?));
            }
            _ => {
                // Unknown type — store as event with full original as extensions
                if !session_header_emitted {
                    records.push(build_session_from_first_message(&val));
                    session_header_emitted = true;
                }
                records.push(HubRecord::Event(HubEvent {
                    event_type: format!("claude_unknown_{msg_type}"),
                    timestamp: val.get("timestamp")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    data: Value::Null,
                    extensions: serde_json::json!({"claude-code": val}),
                }));
            }
        }
    }
    Ok(records)
}

pub fn from_hub(records: &[HubRecord]) -> Result<Vec<Value>, ConvertError> {
    let mut lines = Vec::new();
    for record in records {
        match record {
            HubRecord::Session(_) => {
                // Session header doesn't map to a Claude JSONL line directly
                // (Claude infers session from per-message sessionId)
            }
            HubRecord::Message(msg) => {
                lines.push(hub_message_to_claude(msg)?);
            }
            HubRecord::Event(evt) => {
                lines.push(hub_event_to_claude(evt)?);
            }
        }
    }
    Ok(lines)
}

// Helper functions: build_session_from_first_message, claude_message_to_hub,
// claude_event_to_hub, hub_message_to_claude, hub_event_to_claude
// Each preserves ALL original fields in extensions for lossless round-trip.
// Implementation details: map content blocks, extract metadata, preserve extensions.
// Full implementation during coding — see spec for field mappings.
```

Note: The above is the skeleton. During implementation, each helper function must:
1. Map universal fields to Hub schema (role, content blocks, timestamps, tokens)
2. Store the ENTIRE original JSON record in `extensions.claude-code._original` as a fallback
3. When converting back (`from_hub`), reconstruct from extensions first, override with universal fields

The `_original` approach guarantees lossless round-trip: if the converter misses a field, it's still in the original.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -5`

- [ ] **Step 3: Commit**

```bash
git add src/interchange/claude.rs
git commit -m "feat: add Claude Code JSONL <-> Hub converter

Maps 12+ Claude message types to Hub records. Preserves all
fields in extensions for lossless round-trip."
```

---

### Task 4: Claude Code Round-Trip Tests

**Files:**
- Create: `tests/interchange/round_trip.rs` (or `src/interchange/claude.rs` tests module)
- Create: `tests/interchange/fixtures/claude-sample.jsonl`

- [ ] **Step 1: Create test fixture**

Extract a small (5-10 message) conversation from `~/.claude/projects/` that includes:
- At least 1 user text message
- At least 1 assistant response with tool_use
- At least 1 tool_result (including compound content if available)
- At least 1 thinking block
- Sanitize personal data

- [ ] **Step 2: Write round-trip test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::semantic_eq::semantic_eq;

    #[test]
    fn test_claude_round_trip() {
        let fixture = include_str!("../../tests/interchange/fixtures/claude-sample.jsonl");
        let reader = std::io::BufReader::new(fixture.as_bytes());

        // Convert to Hub
        let hub_records = to_hub(reader).expect("to_hub failed");
        assert!(!hub_records.is_empty());

        // Convert back to Claude JSONL
        let claude_lines = from_hub(&hub_records).expect("from_hub failed");

        // Compare each line
        let original_lines: Vec<Value> = fixture.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();

        // Skip session header (Hub has one, Claude doesn't)
        assert_eq!(claude_lines.len(), original_lines.len(),
            "Line count mismatch: {} vs {}", claude_lines.len(), original_lines.len());

        for (i, (orig, result)) in original_lines.iter().zip(claude_lines.iter()).enumerate() {
            semantic_eq(orig, result)
                .unwrap_or_else(|e| panic!("Line {i} mismatch: {e}"));
        }
    }

    #[test]
    fn test_claude_cache_split_preserved() {
        // Verify cache_creation and cache_read survive round-trip
        let msg_with_cache = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hi"}],"usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":500,"cache_read_input_tokens":200}},"uuid":"test-uuid","timestamp":"2026-03-29T12:00:00Z","sessionId":"test"}"#;
        let reader = std::io::BufReader::new(msg_with_cache.as_bytes());
        let hub = to_hub(reader).unwrap();
        let back = from_hub(&hub).unwrap();
        let orig: Value = serde_json::from_str(msg_with_cache).unwrap();
        semantic_eq(&orig, &back[0]).unwrap();
    }

    #[test]
    fn test_claude_compound_tool_result() {
        // Verify array content in tool_result survives round-trip
        // (text + image in same tool result)
    }

    #[test]
    fn test_claude_api_message_id_preserved() {
        // Verify msg_01ABC... ID survives round-trip
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test interchange::claude -- --nocapture 2>&1 | tail -10`
Expected: all pass

- [ ] **Step 4: Commit**

```bash
git add tests/interchange/ src/interchange/claude.rs
git commit -m "test: add Claude Code round-trip lossless tests

Verifies cache split, compound tool results, API message ID,
and full conversation round-trip with real fixture."
```

---

### Task 5: Codex Converter + Tests

**Files:**
- Create: `src/interchange/codex.rs`
- Create: `tests/interchange/fixtures/codex-sample.jsonl`

Same pattern as Task 3-4 but for Codex JSONL format. Key differences:
- Event stream model (`{timestamp, type, payload}`)
- Cumulative token tracking (must calculate deltas for Hub, reconstruct cumulative for round-trip)
- Turn context events -> Hub events with Codex extensions
- `session_meta` -> Session header

- [ ] **Step 1: Write converter** (skeleton + helpers)
- [ ] **Step 2: Create fixture** (sanitized rollout file)
- [ ] **Step 3: Write round-trip test** (including cumulative token delta test)
- [ ] **Step 4: Run tests, verify pass**
- [ ] **Step 5: Commit**

---

### Task 6: Gemini CLI Converter + Tests

**Files:**
- Create: `src/interchange/gemini.rs`
- Create: `tests/interchange/fixtures/gemini-sample.json`

Key differences from other converters:
- Single JSON object (not JSONL) — read entire file, not line-by-line
- Thoughts with subject + description + timestamp
- Tool calls with displayName, description, renderOutputAsMarkdown
- Separate logs.json index (not needed for Hub conversion — session file has all data)

- [ ] **Step 1: Write converter**
- [ ] **Step 2: Create fixture**
- [ ] **Step 3: Write round-trip test** (including thoughts preservation)
- [ ] **Step 4: Run tests, verify pass**
- [ ] **Step 5: Commit**

---

### Task 7: OpenCode Converter + Tests

**Files:**
- Create: `src/interchange/opencode.rs`
- Create: `tests/interchange/fixtures/opencode-session.json`

Key differences:
- SQLite + JSON hybrid — read from db or from exported JSON
- Message/part separation (parts reference messages)
- 6 part types: text, step-start, step-finish, reasoning, tool, patch
- `ses_`/`msg_`/`prt_` ID prefixes
- Dual timestamps (time.created + time.completed)
- Cost field per message

For round-trip testing, use exported JSON rather than SQLite directly (simpler fixtures). SQLite integration tested separately.

- [ ] **Step 1: Write converter** (JSON export format first, SQLite adapter later)
- [ ] **Step 2: Create fixture** (exported messages + parts JSON)
- [ ] **Step 3: Write round-trip test** (including dual timestamps, step boundaries, patches)
- [ ] **Step 4: Run tests, verify pass**
- [ ] **Step 5: Commit**

---

### Task 8: Cross-CLI Conversion Tests

**Files:**
- Create: `tests/interchange/cross_cli.rs`

Test all 12 cross-CLI pairs. For each pair, verify:
1. All portable fields survive (role, content text, tool names, timestamps)
2. Source extensions are NOT written to target format
3. `_conversion` extension is present noting source CLI

- [ ] **Step 1: Write cross-CLI test helpers**

```rust
fn verify_portable_fields(original: &HubMessage, converted: &HubMessage) {
    assert_eq!(original.role, converted.role);
    assert_eq!(original.timestamp, converted.timestamp);
    assert_eq!(original.content.len(), converted.content.len());
    // ... compare each content block's portable fields
}
```

- [ ] **Step 2: Write tests for all 12 pairs**
- [ ] **Step 3: Run tests, verify pass**
- [ ] **Step 4: Commit**

---

### Task 9: CLI Command (`unleash convert`)

**Files:**
- Modify: `src/cli.rs` (add convert subcommand)
- Modify: `src/lib.rs` (wire up convert command)

- [ ] **Step 1: Add convert subcommand to CLI**

```rust
/// Convert conversation history between CLI formats
#[derive(Debug, clap::Args)]
pub struct ConvertArgs {
    /// Source format (claude, codex, gemini, opencode, hub)
    #[arg(long)]
    pub from: String,

    /// Target format (claude, codex, gemini, opencode, hub). Defaults to hub.
    #[arg(long, default_value = "hub")]
    pub to: String,

    /// Input file path
    pub input: String,

    /// Output file path
    #[arg(short, long)]
    pub output: Option<String>,

    /// Verify lossless round-trip instead of converting
    #[arg(long)]
    pub verify: bool,
}
```

- [ ] **Step 2: Wire up in lib.rs**
- [ ] **Step 3: Test CLI** (`unleash convert --from claude test.jsonl -o test.ucf.jsonl`)
- [ ] **Step 4: Commit**

---

### Task 10: Edge Case + Negative Tests

**Files:**
- Create: `tests/interchange/edge_cases.rs`

- [ ] **Step 1: Write edge case tests**
- Empty session (0 messages)
- Single message session
- Interrupted session (toolUseResult.interrupted = true)
- Session with images
- Malformed JSONL line (should skip with warning)
- Unknown content block type (should preserve)
- Future ucf_version (should refuse)

- [ ] **Step 2: Run all tests**

Run: `cargo test interchange -- --nocapture 2>&1 | tail -20`
Expected: all pass

- [ ] **Step 3: Commit**

---

### Task 11: Property-Based Tests

**Files:**
- Add to: `tests/interchange/round_trip.rs`

- [ ] **Step 1: Add proptest round-trip**

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn hub_message_round_trip_json(
        text in ".*",
        role in prop_oneof!["user", "assistant", "system"],
    ) {
        let msg = HubMessage {
            id: "test".into(),
            role,
            content: vec![ContentBlock::Text { text }],
            // ... defaults
        };
        let json = serde_json::to_string(&HubRecord::Message(msg.clone())).unwrap();
        let parsed: HubRecord = serde_json::from_str(&json).unwrap();
        // Verify round-trip through serialization
    }
}
```

- [ ] **Step 2: Run proptest**

Run: `cargo test interchange::round_trip::hub_message -- --nocapture`

- [ ] **Step 3: Commit**

---

### Task 12: Documentation

**Files:**
- Create: `docs/extensions/conversation-format.md` (user-facing guide)
- Modify: `docs/DOCUMENTATION_MAP.md`

- [ ] **Step 1: Write user-facing format guide**

Cover: what is UCF, how to convert, supported formats, examples.

- [ ] **Step 2: Update doc map**
- [ ] **Step 3: Commit**
- [ ] **Step 4: Push and create PR**
