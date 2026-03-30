# OpenCode Conversation Storage Format

> Last verified: 2026-03-29, OpenCode v1.3.5

Internal developer reference for unleash. Describes how OpenCode persists conversations, sessions, and tool interactions on disk.

---

## 1. Storage Location

All persistent state lives under the XDG data directory:

```
~/.local/share/opencode/
```

Primary files and directories:

| Path | Purpose |
|------|---------|
| `opencode.db` | Main SQLite database (schema, indexes, relations) |
| `opencode.db-wal` | WAL journal (write-ahead log) |
| `opencode.db-shm` | Shared memory file for WAL |
| `storage/` | File-based storage for large/structured data |
| `storage/message/` | Per-message JSON files |
| `storage/part/` | Per-part JSON files (tool calls, text, patches, etc.) |
| `storage/session/` | Per-session JSON files |
| `storage/session_diff/` | Session diff snapshots |
| `storage/project/` | Project metadata files |

The database acts as the index and relational backbone; bulky JSON payloads are stored as individual files in `storage/` subdirectories, keyed by their respective IDs.

---

## 2. File Format

### Database

- **Engine**: SQLite 3
- **Journal mode**: WAL (write-ahead logging) -- enables concurrent reads during writes
- **ORM**: Drizzle ORM (TypeScript/JS schema definitions compiled to SQL)
- **Encoding**: UTF-8

### Hybrid storage model

OpenCode uses a **SQLite + filesystem hybrid**. The SQLite tables hold structured metadata (IDs, timestamps, foreign keys, lightweight fields), while the `data` column in several tables contains JSON, and larger payloads are written as standalone `.json` files under `storage/`.

Each entity type has a corresponding subdirectory in `storage/`:
- Files are named by entity ID (e.g., `storage/message/msg_abc123.json`)
- The database row references the entity by ID; the file contains the full JSON payload

### Tables

The database contains these tables:

| Table | Purpose |
|-------|---------|
| `project` | Registered projects (by worktree path) |
| `session` | Conversation sessions |
| `message` | Individual messages within sessions |
| `part` | Sub-message parts (text, tool calls, patches, reasoning, etc.) |
| `todo` | Task/todo tracking |
| `session_share` | Shared session references |
| `workspace` | Workspace definitions |
| `account` | Provider account metadata |

---

## 3. Message Schema

### `message` table columns

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT PK | Prefixed ID, format: `msg_` + random base36 |
| `session_id` | TEXT FK | References `session.id` |
| `time_created` | TEXT | ISO 8601 timestamp |
| `time_updated` | TEXT | ISO 8601 timestamp |
| `data` | TEXT (JSON) | Full message payload |

### `data` JSON fields

The `data` column (and corresponding `storage/message/<id>.json` file) contains:

```json
{
  "id": "msg_xxxxxxxx",
  "sessionID": "ses_xxxxxxxx",
  "role": "user | assistant",
  "time": {
    "created": 1711700000.000,
    "completed": 1711700005.000
  },
  "parentID": "msg_yyyyyyyy",
  "modelID": "claude-sonnet-4-20250514",
  "providerID": "anthropic",
  "mode": "normal",
  "agent": "default",
  "path": {
    "cwd": "/home/user/project",
    "root": "/home/user/project"
  },
  "cost": 0.003456,
  "tokens": {
    "input": 1500,
    "output": 800,
    "reasoning": 0,
    "cache": {
      "read": 500,
      "write": 1000
    }
  },
  "finish": "end_turn"
}
```

| Field | Type | Notes |
|-------|------|-------|
| `id` | string | Same as table PK |
| `sessionID` | string | Parent session |
| `role` | string | `"user"` or `"assistant"` |
| `time.created` | float | Unix epoch seconds |
| `time.completed` | float | Unix epoch seconds; null if incomplete |
| `parentID` | string | ID of preceding message in conversation tree; enables branching |
| `modelID` | string | Model identifier used for this response |
| `providerID` | string | Provider name (e.g., `"anthropic"`, `"openai"`, `"bedrock"`) |
| `mode` | string | Interaction mode (e.g., `"normal"`) |
| `agent` | string | Agent identity within OpenCode |
| `path.cwd` | string | Working directory at message time |
| `path.root` | string | Project root directory |
| `cost` | float | Estimated cost in USD |
| `tokens` | object | Token counts (see Section 6) |
| `finish` | string | Finish reason (see Section 11) |

---

## 4. Message Types

### Role: `user`

User messages contain the prompt text. Parts are stored in the `part` table and typically consist of a single `text` part.

### Role: `assistant`

Assistant messages are decomposed into multiple **parts** (stored in the `part` table, each with a `prt_` prefixed ID). The six part types are:

| Part Type | Description |
|-----------|-------------|
| `text` | Plain text content from the model |
| `step-start` | Marks the beginning of an agentic step/turn |
| `step-finish` | Marks the end of a step, includes finish reason |
| `reasoning` | Model reasoning/thinking content (may be encrypted) |
| `tool` | Tool invocation with input, output, and metadata |
| `patch` | File modification record with git hash tracking |

A typical assistant message contains parts in this order:

```
step-start -> reasoning? -> text? -> tool* -> text? -> step-finish
```

Multiple tool calls can occur within a single step. Steps can repeat within one message for multi-step agentic flows.

---

## 5. Tool Call Format

Tool invocations are stored as parts with `type: "tool"`. The part's data structure:

```json
{
  "type": "tool",
  "callID": "call_xxxxxxxx",
  "tool": "bash",
  "state": {
    "status": "completed | error | pending | running",
    "input": {
      "command": "ls -la"
    },
    "output": "total 42\ndrwxr-xr-x ...",
    "title": "List directory contents",
    "metadata": {
      "output": "total 42\ndrwxr-xr-x ...",
      "exit": 0,
      "description": "Ran bash command",
      "truncated": false
    },
    "time": {
      "start": 1711700001.000,
      "end": 1711700002.500
    }
  }
}
```

| Field | Type | Notes |
|-------|------|-------|
| `callID` | string | Unique tool call identifier |
| `tool` | string | Tool name (e.g., `"bash"`, `"read"`, `"write"`, `"glob"`, `"grep"`) |
| `state.status` | string | Execution status: `"completed"`, `"error"`, `"pending"`, `"running"` |
| `state.input` | object | Tool-specific input parameters (varies by tool) |
| `state.output` | string | Primary output content |
| `state.title` | string | Human-readable summary of the action |
| `state.metadata.output` | string | Full output (may duplicate `state.output`) |
| `state.metadata.exit` | int | Exit code for shell commands; null for non-shell tools |
| `state.metadata.description` | string | Describes what the tool did |
| `state.metadata.truncated` | bool | Whether output was truncated |
| `state.time.start` | float | Unix epoch seconds |
| `state.time.end` | float | Unix epoch seconds |

### Patch parts

Patch parts track file modifications:

```json
{
  "type": "patch",
  "path": "/home/user/project/src/main.rs",
  "hash": {
    "before": "a1b2c3d4...",
    "after": "e5f6g7h8..."
  }
}
```

These record the git object hashes before and after modification, enabling diff reconstruction.

---

## 6. Token Tracking

Token usage is tracked per-message in the `data.tokens` object:

```json
{
  "tokens": {
    "input": 1500,
    "output": 800,
    "reasoning": 200,
    "cache": {
      "read": 500,
      "write": 1000
    }
  }
}
```

| Field | Description |
|-------|-------------|
| `input` | Total input tokens sent to the model |
| `output` | Tokens generated by the model |
| `reasoning` | Tokens used for chain-of-thought / extended thinking |
| `cache.read` | Tokens served from prompt cache (cache hits) |
| `cache.write` | Tokens written to prompt cache (cache misses stored) |

The `cost` field at message level is the estimated USD cost computed from these token counts and the model's pricing.

Token values are integers. All fields may be `0` but are always present on assistant messages. User messages may omit the tokens object entirely.

---

## 7. Session Management

### Session IDs

Format: `ses_` + base36 random string (e.g., `ses_k7m2x9p4`).

### `session` table columns

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT PK | `ses_` prefixed ID |
| `slug` | TEXT | Human-readable name, e.g., `"hidden-wolf"`, `"amber-fox"` |
| `version` | TEXT | Schema/format version |
| `title` | TEXT | Auto-generated or user-set session title |
| `summary` | TEXT (JSON) | Session summary object |
| `parent_id` | TEXT FK | References parent session for branching/hierarchy |
| `permission` | TEXT | Permission level for the session |
| `workspace_id` | TEXT FK | References `workspace.id` |
| `time_created` | TEXT | ISO 8601 |
| `time_updated` | TEXT | ISO 8601 |

### Session hierarchy

Sessions support parent-child relationships via `parent_id`. This enables:
- Branching conversations from a specific point
- Hierarchical session organization
- Forking a session to explore alternative approaches

### Session summary

The `summary` field contains structured data about the session's content:

```json
{
  "additions": 42,
  "deletions": 15,
  "files": ["src/main.rs", "Cargo.toml"]
}
```

This tracks aggregate code changes (lines added/deleted) and files touched across the session.

### Slugs

Each session receives a two-word slug (adjective-noun pattern) for human-friendly identification. Examples: `"hidden-wolf"`, `"amber-fox"`, `"quiet-river"`. These are unique within a project scope.

---

## 8. Project Organization

### Project identification

Projects are identified by a **SHA1 hash** of the worktree path. This hash serves as the primary key and links all sessions/messages to a specific project.

### `project` table columns

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT PK | SHA1 hash of the worktree path |
| `path` | TEXT | Absolute path to project worktree |
| `vcs` | TEXT | Version control type (e.g., `"git"`) |
| `time_created` | TEXT | ISO 8601 |
| `time_updated` | TEXT | ISO 8601 |

### Worktree path

The `path` field stores the canonical absolute path to the project root (typically the git worktree root). This is used to:
- Associate sessions with the correct project when OpenCode is launched
- Resolve relative paths in tool calls and patches
- Detect project identity across different working directories within the same repo

### VCS detection

OpenCode detects the version control system in use. Currently `"git"` is the primary supported VCS type. This informs patch tracking and diff generation.

---

## 9. Configuration and Retention

### Configuration file

```
~/.config/opencode/opencode.json
```

This is the primary user configuration file. It controls provider settings, model selection, keybindings, and other preferences. It does **not** control storage format or retention.

### Data retention

- **No automatic cleanup**: OpenCode does not auto-purge old sessions or messages
- **No TTL**: There is no time-to-live mechanism on stored data
- **No size limits**: The database and storage directory grow unbounded
- **Manual cleanup**: Users must manually delete sessions or remove the database/storage files

### Database maintenance

Since WAL mode is used, the `-wal` and `-shm` files may grow. SQLite handles checkpointing automatically, but manual `PRAGMA wal_checkpoint(TRUNCATE)` can reclaim space if needed.

---

## 10. Multimodal Content

OpenCode's storage schema supports multimodal content, but specifics are still being mapped:

- **Images**: Can be included in user message parts. Storage mechanism (inline base64 vs. file reference in `storage/`) is TBD pending further investigation.
- **File attachments**: Referenced by path in tool call inputs/outputs rather than stored inline.
- **Audio/video**: No evidence of direct support in the current schema.

The `part` table's flexible JSON `data` column can accommodate additional content types as they are added. The `type` field on parts would be extended with new values for new modalities.

> **TODO**: Verify multimodal storage paths with actual image-containing conversations once available.

---

## 11. Error and Interrupted States

### Step finish reasons

The `step-finish` part type includes a `reason` field indicating how the step ended:

| Reason | Description |
|--------|-------------|
| `end_turn` | Model completed its response normally |
| `max_tokens` | Response hit the output token limit |
| `stop_sequence` | A stop sequence was encountered |
| `tool_use` | Model is yielding to execute a tool (step continues after tool result) |
| `error` | An error occurred during generation |
| `cancelled` | User cancelled the response |

### Tool state statuses

The `state.status` field on tool parts tracks execution lifecycle:

| Status | Description |
|--------|-------------|
| `pending` | Tool call queued but not yet started |
| `running` | Tool is currently executing |
| `completed` | Tool finished successfully |
| `error` | Tool execution failed |

When `status` is `"error"`, the `state.output` and `state.metadata.output` fields contain the error message or stack trace.

### Interrupted sessions

If OpenCode exits mid-conversation:
- The last message may have `time.completed` as `null`
- Tool parts may remain in `"pending"` or `"running"` status
- The `step-finish` part may be absent from the final step
- The session's `time_updated` reflects the last successful write

On resume, OpenCode does **not** attempt to replay or complete interrupted tool calls. The conversation continues from the last fully committed state.

### Reasoning part encryption

Reasoning parts (`type: "reasoning"`) may contain encrypted content depending on the provider. For example, Anthropic's extended thinking responses may include a `signature` field and the reasoning text may be provider-encrypted. This is a provider-side behavior, not an OpenCode storage decision. The encrypted payload is stored as-is in the part data.
