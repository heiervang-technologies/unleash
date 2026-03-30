# OpenAI Codex CLI — Conversation Storage Format

> **Last verified: 2026-03-29, Codex CLI v0.117.0**
>
> Internal developer reference for unleash. Not for end-user distribution.

---

## 1. Storage Location

All persistent state lives under `~/.codex/`:

```
~/.codex/
├── config.toml                      # User configuration
├── history.jsonl                    # Global history index
├── session_index.jsonl              # Session metadata index
├── state_5.sqlite                   # Thread/session state database
├── logs_1.sqlite                    # Diagnostic log database
└── sessions/                        # Conversation transcripts
    └── YYYY/
        └── MM/
            └── DD/
                └── rollout-<ISO8601>-<UUID>.jsonl
```

Key paths:

| Path | Purpose |
|------|---------|
| `~/.codex/sessions/` | Date-partitioned JSONL conversation logs |
| `~/.codex/state_5.sqlite` | Authoritative thread metadata (SQLite WAL mode) |
| `~/.codex/logs_1.sqlite` | Internal telemetry and error logs |
| `~/.codex/history.jsonl` | Append-only history for search/recall |
| `~/.codex/session_index.jsonl` | Lightweight session summary for fast listing |
| `~/.codex/config.toml` | User preferences including retention policy |

The `_5` and `_1` suffixes on SQLite files are schema version numbers. Codex migrates automatically on version bumps by creating a new file.

---

## 2. File Format

### JSONL Transcript Files

Each conversation is stored as a single **newline-delimited JSON** (JSONL) file. One JSON object per line, strictly ordered by emission time.

**Naming convention:**

```
rollout-YYYY-MM-DDTHH-mm-<UUID>.jsonl
```

Example:

```
rollout-2026-03-29T14-22-a3b1c9d0-7e4f-4a8b-9c12-def456789abc.jsonl
```

- The ISO 8601 prefix encodes session start time (hours and minutes only, no seconds).
- The UUID is a v4 random identifier ensuring uniqueness.
- Files are append-only during the session; no in-place edits.

### SQLite Databases

`state_5.sqlite` and `logs_1.sqlite` use standard SQLite3 with WAL journaling. The state database holds structured metadata; the JSONL files hold the full conversation stream. This is a **hybrid design** — SQLite for queryable indexes, JSONL for the authoritative event log.

---

## 3. Message Schema

Every line in a rollout JSONL file is a top-level event object with this structure:

```jsonc
{
  "timestamp": "2026-03-29T14:22:31.456Z",   // ISO 8601 UTC
  "type": "<event_type>",                     // Discriminator string
  "payload": { /* type-specific data */ }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | `string` (ISO 8601) | UTC time the event was emitted |
| `type` | `string` | Event type discriminator (see Section 4) |
| `payload` | `object` | Type-specific data; schema varies per `type` |

Events are emitted in strict chronological order. The first event in any rollout is always `session_meta`.

---

## 4. Message Types

### `session_meta`

Always the first event. Records session-level configuration.

```jsonc
{
  "type": "session_meta",
  "timestamp": "2026-03-29T14:22:30.100Z",
  "payload": {
    "session_id": "a3b1c9d0-7e4f-4a8b-9c12-def456789abc",
    "cli_version": "0.117.0",
    "model": "o4-mini",
    "model_provider": "openai",
    "cwd": "/home/user/project",
    "git_sha": "abc1234",
    "git_branch": "main",
    "git_origin_url": "https://github.com/user/repo.git",
    "agent_nickname": null
  }
}
```

### `turn_context`

Emitted at the start of each agent turn. Captures the policy and configuration snapshot for that turn.

```jsonc
{
  "type": "turn_context",
  "timestamp": "2026-03-29T14:22:31.200Z",
  "payload": {
    "model": "o4-mini",
    "approval_policy": "auto-edit",
    "sandbox_policy": "lenient",
    "personality": "default",
    "collaboration_mode": "solo",
    "effort": "high",
    "reasoning": true
  }
}
```

Fields in `turn_context.payload`:

| Field | Description |
|-------|-------------|
| `model` | Model name for this turn |
| `approval_policy` | One of `suggest`, `auto-edit`, `full-auto` |
| `sandbox_policy` | Sandbox restriction level |
| `personality` | System prompt variant |
| `collaboration_mode` | `solo` or `pair` |
| `effort` | Reasoning effort level (`low`, `medium`, `high`) |
| `reasoning` | Whether extended reasoning is enabled |

### `response_item`

Wraps a message from any participant (user or assistant). Follows an OpenAI-compatible item schema.

**User message example:**

```jsonc
{
  "type": "response_item",
  "timestamp": "2026-03-29T14:22:31.456Z",
  "payload": {
    "id": "item_abc123",
    "role": "user",
    "content": [
      {
        "type": "input_text",
        "text": "Fix the failing test in auth.ts"
      }
    ]
  }
}
```

**Assistant message example:**

```jsonc
{
  "type": "response_item",
  "timestamp": "2026-03-29T14:22:35.789Z",
  "payload": {
    "id": "item_def456",
    "role": "assistant",
    "content": [
      {
        "type": "output_text",
        "text": "I'll fix the auth test. Let me read the file first."
      }
    ]
  }
}
```

### `event_msg`

A polymorphic event wrapper for various signal types. The `payload.type` sub-field discriminates:

| `payload.type` | Description |
|-----------------|-------------|
| `agent_message` | Agent-generated text (streaming chunks or complete) |
| `user_message` | User input captured outside the response_item flow |
| `token_count` | Token usage snapshot (see Section 6) |
| `task_started` | Marks the beginning of an agentic task |
| `task_complete` | Marks task completion with status and summary |
| `reasoning` | Extended reasoning / chain-of-thought trace |

**`agent_message` example:**

```jsonc
{
  "type": "event_msg",
  "timestamp": "2026-03-29T14:22:36.000Z",
  "payload": {
    "type": "agent_message",
    "text": "Reading auth.ts to understand the test failure...",
    "stream": false
  }
}
```

**`task_started` example:**

```jsonc
{
  "type": "event_msg",
  "timestamp": "2026-03-29T14:22:36.100Z",
  "payload": {
    "type": "task_started",
    "task_id": "task_001",
    "description": "Fix failing test"
  }
}
```

**`reasoning` example:**

```jsonc
{
  "type": "event_msg",
  "timestamp": "2026-03-29T14:22:36.200Z",
  "payload": {
    "type": "reasoning",
    "text": "The test expects a 401 status but the handler returns 403..."
  }
}
```

---

## 5. Tool Call Format

Tool invocations appear as `response_item` events with role `assistant` and a content array containing `tool_use` items, followed by a corresponding `tool_result` item.

**Tool invocation:**

```jsonc
{
  "type": "response_item",
  "timestamp": "2026-03-29T14:22:37.000Z",
  "payload": {
    "id": "item_tool_001",
    "role": "assistant",
    "content": [
      {
        "type": "tool_use",
        "id": "call_abc123",
        "name": "shell",
        "input": {
          "command": ["cat", "src/auth.ts"]
        }
      }
    ]
  }
}
```

**Tool result:**

```jsonc
{
  "type": "response_item",
  "timestamp": "2026-03-29T14:22:38.500Z",
  "payload": {
    "id": "item_result_001",
    "role": "tool",
    "tool_call_id": "call_abc123",
    "content": [
      {
        "type": "output_text",
        "text": "export function authenticate(req, res) {\n  // ...\n}"
      }
    ]
  }
}
```

Common tool names in Codex CLI: `shell` (command execution), `file_edit` (apply edits), `file_read` (read file contents). The `input` schema varies per tool.

For `file_edit`:

```jsonc
{
  "type": "tool_use",
  "id": "call_edit_001",
  "name": "file_edit",
  "input": {
    "path": "src/auth.ts",
    "old_string": "return res.status(403)",
    "new_string": "return res.status(401)"
  }
}
```

Tool calls that require user approval have an `approval_status` field in the surrounding event (`pending`, `approved`, `denied`).

---

## 6. Token Tracking

Token usage is reported via `event_msg` events with `payload.type: "token_count"`. These appear after each model response.

```jsonc
{
  "type": "event_msg",
  "timestamp": "2026-03-29T14:22:39.000Z",
  "payload": {
    "type": "token_count",
    "total_token_usage": {
      "input_tokens": 4521,
      "cached_input_tokens": 3200,
      "output_tokens": 387,
      "reasoning_output_tokens": 128
    }
  }
}
```

| Field | Description |
|-------|-------------|
| `input_tokens` | Total input tokens sent to the model (includes cached) |
| `cached_input_tokens` | Portion of input tokens served from prompt cache |
| `output_tokens` | Tokens generated by the model (visible output) |
| `reasoning_output_tokens` | Tokens used for extended reasoning (chain-of-thought); 0 if reasoning disabled |

These are **cumulative session totals**, not per-turn deltas. To compute per-turn usage, diff consecutive `token_count` events. The `tokens_used` column in the SQLite threads table stores the final cumulative value.

---

## 7. Session Management

### Thread UUID

Each session is identified by a v4 UUID. This ID appears in:

- The JSONL filename (`rollout-...-<UUID>.jsonl`)
- The `session_meta` event payload
- The `threads` table primary key in `state_5.sqlite`
- The `session_index.jsonl` entries
- The `history.jsonl` entries

### SQLite `threads` Table

`~/.codex/state_5.sqlite` contains the `threads` table as the authoritative session registry:

```sql
CREATE TABLE threads (
    id                TEXT PRIMARY KEY,     -- UUID
    rollout_path      TEXT NOT NULL,        -- Relative path to JSONL file
    created_at        TEXT NOT NULL,        -- ISO 8601
    updated_at        TEXT NOT NULL,        -- ISO 8601
    source            TEXT,                 -- e.g. "cli", "vscode"
    model_provider    TEXT,                 -- e.g. "openai"
    cwd               TEXT,                 -- Working directory at session start
    title             TEXT,                 -- Auto-generated or user-set title
    tokens_used       INTEGER DEFAULT 0,   -- Final cumulative token count
    git_sha           TEXT,                 -- HEAD commit at session start
    git_branch        TEXT,                 -- Branch name
    git_origin_url    TEXT,                 -- Remote URL
    cli_version       TEXT,                 -- Codex CLI version string
    agent_nickname    TEXT,                 -- Optional agent display name
    model             TEXT,                 -- Model name (e.g. "o4-mini")
    reasoning_effort  TEXT                  -- "low", "medium", "high"
);
```

The `rollout_path` is relative to `~/.codex/`, e.g. `sessions/2026/03/29/rollout-2026-03-29T14-22-<UUID>.jsonl`.

`updated_at` is refreshed on every new event write. `tokens_used` is updated from the last `token_count` event when the session ends.

---

## 8. Project Organization

### Date-Based Hierarchy

Sessions are partitioned by date into `sessions/YYYY/MM/DD/` directories. This keeps any single directory from accumulating thousands of files and enables efficient date-range queries.

```
~/.codex/sessions/
├── 2026/
│   ├── 03/
│   │   ├── 28/
│   │   │   ├── rollout-2026-03-28T09-15-<UUID1>.jsonl
│   │   │   └── rollout-2026-03-28T16-42-<UUID2>.jsonl
│   │   └── 29/
│   │       └── rollout-2026-03-29T14-22-<UUID3>.jsonl
```

### `session_index.jsonl`

A lightweight append-only index at `~/.codex/session_index.jsonl` for fast session listing without scanning the directory tree or opening SQLite:

```json
{"id": "a3b1c9d0-...", "thread_name": "Fix auth tests", "updated_at": "2026-03-29T14:45:00Z"}
{"id": "b7c2d1e3-...", "thread_name": "Refactor database layer", "updated_at": "2026-03-29T15:10:00Z"}
```

| Field | Description |
|-------|-------------|
| `id` | Session UUID (matches threads table and JSONL filename) |
| `thread_name` | Human-readable session title |
| `updated_at` | Last activity timestamp |

This file may contain duplicate IDs (later entries supersede earlier ones). Codex compacts it periodically.

### `history.jsonl`

Global search/recall index at `~/.codex/history.jsonl`:

```json
{"session_id": "a3b1c9d0-...", "ts": "2026-03-29T14:22:31Z", "text": "Fix the failing test in auth.ts"}
{"session_id": "a3b1c9d0-...", "ts": "2026-03-29T14:22:36Z", "text": "Reading auth.ts to understand the test failure..."}
```

Each entry captures a single user or agent message for full-text search. The `ts` field is the original event timestamp; `session_id` links back to the thread.

---

## 9. Configuration and Retention

### `config.toml`

User preferences are stored in `~/.codex/config.toml`:

```toml
[history]
max_bytes = 104857600    # 100 MB default; triggers compaction
persistence = "normal"   # "normal" | "none"

[model]
default = "o4-mini"
reasoning_effort = "medium"
```

### Compaction

When total JSONL storage exceeds `history.max_bytes`, Codex runs compaction:

1. Oldest sessions (by `created_at`) are deleted first.
2. Corresponding rows in `state_5.sqlite` threads table are removed.
3. Entries in `session_index.jsonl` and `history.jsonl` become orphaned (cleaned on next rewrite).
4. Compaction runs at session start, not during active sessions.

### `persistence = "none"`

When set, Codex skips all JSONL writes and SQLite updates. Sessions exist only in memory. Useful for ephemeral/CI environments. The session still functions normally — only disk persistence is disabled.

---

## 10. Multimodal Content

User messages can include images via the `input_image` content type within `response_item` events:

```jsonc
{
  "type": "response_item",
  "timestamp": "2026-03-29T14:30:00.000Z",
  "payload": {
    "id": "item_img_001",
    "role": "user",
    "content": [
      {
        "type": "input_text",
        "text": "What's wrong with this UI?"
      },
      {
        "type": "input_image",
        "source": {
          "type": "base64",
          "media_type": "image/png",
          "data": "<base64-encoded-image-data>"
        }
      }
    ]
  }
}
```

The `source` object follows the same schema as the OpenAI vision API:

| `source.type` | Description |
|----------------|-------------|
| `base64` | Inline base64-encoded image data with `media_type` and `data` fields |
| `url` | Remote image URL (less common in CLI context) |

Supported media types: `image/png`, `image/jpeg`, `image/gif`, `image/webp`.

Note: base64 image data can make JSONL files very large. This is a factor in `history.max_bytes` compaction thresholds.

---

## 11. Error and Interrupted States

### `task_complete` Events

Every task lifecycle ends with a `task_complete` event, regardless of outcome:

```jsonc
{
  "type": "event_msg",
  "timestamp": "2026-03-29T14:25:00.000Z",
  "payload": {
    "type": "task_complete",
    "task_id": "task_001",
    "status": "completed",
    "summary": "Fixed auth test: changed 403 to 401 status code"
  }
}
```

Possible `status` values:

| Status | Description |
|--------|-------------|
| `completed` | Task finished successfully |
| `error` | Task failed due to an error |
| `interrupted` | User cancelled or session terminated mid-task |
| `timeout` | Task exceeded time or token budget |

### Error Payloads

When `status` is `error`, additional fields appear:

```jsonc
{
  "type": "task_complete",
  "task_id": "task_001",
  "status": "error",
  "error_code": "tool_execution_failed",
  "error_message": "Command exited with status 1: npm test",
  "summary": null
}
```

### Interrupted Sessions

If a session is killed (SIGINT, SIGTERM, terminal close):

- The JSONL file may end without a `task_complete` event. Consumers must handle truncated files.
- The `updated_at` in SQLite reflects the last successfully written event.
- On next startup, Codex does **not** attempt recovery or replay — the session is treated as closed.
- The `session_index.jsonl` entry may have a stale `updated_at` if the index write was also interrupted.

### Diagnostic Logs

`~/.codex/logs_1.sqlite` captures internal errors, API failures, and retry attempts separately from the conversation stream. This database is not part of the conversation format but is useful for debugging storage issues.

---

## Appendix: Parsing Notes for unleash Developers

1. **Always stream-parse JSONL** — files can grow large (especially with base64 images). Do not load entire files into memory.
2. **Handle missing trailing newlines** — interrupted sessions may have a partial last line. Discard incomplete JSON objects.
3. **UUID extraction from filename** — the UUID is the last hyphen-separated segment before `.jsonl`, but it is itself hyphenated (v4 format). Parse from the `session_meta` event instead of the filename when possible.
4. **SQLite is the index, JSONL is the source of truth** — if they disagree, the JSONL file is authoritative for conversation content. SQLite is authoritative for metadata like `tokens_used` and `title`.
5. **Schema versioning** — the `_5` suffix on `state_5.sqlite` will increment on breaking schema changes. Check for the highest-numbered file if multiple exist.
