# Claude Code Conversation Storage Format

> **Internal developer reference for unleash contributors.**
> Last verified: 2026-03-31, verified against source

---

## 1. Storage Location

All Claude Code persistent data lives under `~/.claude/`.

```
~/.claude/
├── projects/                          # Conversation logs, per-project
│   └── <PATH_ENCODED>/               # e.g. -home-me-ht-unleash/
│       ├── <SESSION_UUID>.jsonl       # Main conversation transcript
│       ├── <SESSION_UUID>/
│       │   └── subagents/
│       │       └── agent-<ID>.jsonl   # Subagent (tool-spawned) transcripts
│       │       └── agent-<ID>.meta.json  # Subagent metadata
│       └── memory/
│           └── MEMORY.md              # Auto-persisted project memory
├── sessions/                          # Active session metadata
│   └── <PID>.json                     # One file per running process
├── history.jsonl                      # Prompt history for search/recall
├── paste-cache/                       # Content-addressed storage for large pastes
├── image-cache/                       # Per-session image storage
│   └── <SESSION_ID>/
├── tool-results/                      # Externalized tool result storage
├── remote-agents/                     # Remote agent connection metadata
├── workflows/                         # Workflow run grouping
│   └── <RUN_ID>/
├── settings.json                      # User-level settings
└── CLAUDE.md                          # User-level global instructions
```

### Path encoding

Project directories use the absolute path with **all non-alphanumeric characters** replaced by `-` (regex `[^a-zA-Z0-9]` -> `-`). Paths exceeding 200 characters are truncated and suffixed with a hash to avoid collisions.

Examples:

| Actual path | Encoded directory name |
|---|---|
| `/home/me/ht/unleash` | `-home-me-ht-unleash` |
| `/home/me/projects/foo` | `-home-me-projects-foo` |
| `/home/me/projects/foo.bar` | `-home-me-projects-foo-bar` |

---

## 2. File Format

### Conversation files

Each conversation is a single **JSONL** (JSON Lines) file named `<UUID>.jsonl`. Every line is a self-contained JSON object representing one message or event.

```
~/.claude/projects/-home-me-ht-unleash/a1b2c3d4-e5f6-7890-abcd-ef1234567890.jsonl
```

UUIDs are v4 format. File size grows unbounded for the lifetime of a session; there is no rotation or splitting.

**Write batching**: Writes are queued in memory and flushed at 100ms intervals. The JSONL file is not immediately consistent with in-memory state; there is a brief window where the most recent entries have not yet been written to disk.

### History index

`~/.claude/history.jsonl` is a JSONL file used for **prompt history search** (the fuzzy-search session picker). Each line records a past conversation. This file is NOT used by `--continue` to find the most recent session (see Section 7).

| Field | Type | Description |
|---|---|---|
| `display` | string | First user message (truncated), shown in picker UI |
| `pastedContents` | `Record<number, StoredPastedContent>` | Pasted content from that first message (see below) |
| `timestamp` | number | Epoch milliseconds |
| `project` | string | Encoded project path |
| `sessionId` | string | UUID linking to the `.jsonl` transcript |

**`StoredPastedContent` structure**: Each entry in `pastedContents` is keyed by a numeric ID and contains:

| Field | Type | Description |
|---|---|---|
| `id` | number | Paste identifier |
| `type` | string | Paste type |
| `content` / `contentHash` | string | Inline content for small pastes, or a content-addressed hash for large pastes stored in `~/.claude/paste-cache/` |
| `mediaType` | string | MIME type of the pasted content |
| `filename` | string | Original filename if applicable |

### Session metadata

`~/.claude/sessions/<PID>.json` tracks each running Claude Code process:

| Field | Type | Description |
|---|---|---|
| `pid` | number | OS process ID |
| `sessionId` | string | UUID of the active conversation |
| `cwd` | string | Working directory |
| `startedAt` | number | Epoch milliseconds |
| `kind` | string | Session type (e.g. `"interactive"`, `"headless"`) |
| `entrypoint` | string | How Claude Code was launched (e.g. `"cli"`, `"sdk-ts"`, `"sdk-py"`) |
| `messagingSocketPath` | string | Unix socket path for IPC messaging |
| `name` | string | Session display name |
| `logPath` | string | Path to session log file |
| `agent` | boolean | Whether this is an agent/subagent session |
| `status` | string | Current status: `"busy"`, `"idle"`, or `"waiting"` |
| `waitingFor` | string | What the session is waiting for (when status is `"waiting"`) |
| `updatedAt` | number | Epoch milliseconds of last status update |
| `bridgeSessionId` | string | Session ID of the bridge connection (if applicable) |

These files are cleaned up when the process exits normally. Stale files from crashed processes may linger.

---

## 3. Message Schema

Every line in a `.jsonl` transcript shares these **universal top-level fields**:

```json
{
  "type": "user",
  "uuid": "msg-uuid-here",
  "parentUuid": "previous-msg-uuid",
  "timestamp": "2026-03-29T10:15:30.123Z",
  "sessionId": "session-uuid-here",
  "version": "2.1.87",
  "cwd": "/home/me/ht/unleash",
  "gitBranch": "main",
  "isSidechain": false,
  "userType": "external"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | string | yes | Message type discriminator (see Section 4) |
| `uuid` | string | yes | Unique ID for this message |
| `parentUuid` | string | yes | UUID of the preceding message in the conversation chain |
| `logicalParentUuid` | string | no | UUID of the logical parent (may differ from `parentUuid` in branched conversations) |
| `timestamp` | string | yes | ISO 8601 with milliseconds |
| `sessionId` | string | yes | Session UUID this message belongs to |
| `version` | string | yes | Claude Code version that wrote this line |
| `cwd` | string | yes | Working directory at time of message |
| `gitBranch` | string | no | Current git branch (absent outside git repos) |
| `isSidechain` | boolean | yes | `true` if this message is part of a branched/sidechain conversation |
| `userType` | string | yes | `"external"` for human, `"internal"` for system-generated |
| `promptId` | string | no | Links related request/response pairs in the same exchange |
| `agentId` | string | no | Identifier of the agent that produced this message |
| `teamName` | string | no | Team name in multi-agent contexts |
| `agentName` | string | no | Display name of the agent |
| `agentColor` | string | no | UI color assigned to the agent |
| `entrypoint` | string | no | How the session was launched: `"cli"`, `"sdk-ts"`, `"sdk-py"`, etc. |
| `slug` | string | no | Short identifier slug |
| `sourceToolAssistantUUID` | string | no | UUID of the assistant message whose tool call produced this message |
| `isMeta` | boolean | no | `true` on user messages that are system-generated metadata, not actual human input |

---

## 4. Message Types

### Transcript message types

Only four message types are persisted to the conversation transcript: `user`, `assistant`, `attachment`, and `system`. Other entry types (metadata, state) are also stored in the JSONL file but are not part of the conversational transcript.

### `user`

Human input. `role` is always `"user"`.

```json
{
  "type": "user",
  "role": "user",
  "content": "Show me the config file",
  ...universal fields
}
```

`content` is either:
- A **string** for plain text input
- An **array** of content blocks when delivering tool results back to the model (see Section 5)

The `isMeta` field, when `true`, indicates this is a system-generated metadata message rather than actual human input.

### `assistant`

Model response. The API response fields (`role`, `content`, `model`, `usage`, `stop_reason`, `id`) are nested inside a `message` sub-object:

```json
{
  "type": "assistant",
  "message": {
    "role": "assistant",
    "model": "claude-opus-4-6-20250327",
    "id": "msg_01ABC...",
    "content": [
      {
        "type": "thinking",
        "thinking": "Let me examine the config...",
        "signature": "ErUBCkYI..."
      },
      {
        "type": "text",
        "text": "Here is the config file contents:"
      },
      {
        "type": "tool_use",
        "id": "toolu_01XYZ...",
        "name": "Read",
        "input": { "file_path": "/home/me/.config/app/config.json" }
      }
    ],
    "usage": { ... },
    "stop_reason": "tool_use"
  },
  ...universal fields
}
```

Key fields inside `message`:

| Field | Type | Description |
|---|---|---|
| `model` | string | Full model identifier |
| `id` | string | Anthropic API message ID (`msg_...`) |
| `content` | array | Ordered content blocks (thinking, text, tool_use) |
| `usage` | object | Token counts (see Section 6) |
| `stop_reason` | string | `"end_turn"`, `"tool_use"`, `"max_tokens"`, `"stop_sequence"` |

### `attachment`

File attachments and hook outputs. This is the fourth transcript message type, used to carry file content, hook output, and other attached data into the conversation.

```json
{
  "type": "attachment",
  "content": "...",
  ...universal fields
}
```

### `system`

Internal system events. These are persisted to the transcript.

```json
{
  "type": "system",
  "subtype": "local_command",
  "content": "Executed: git status",
  "level": "info",
  ...universal fields
}
```

| Field | Type | Description |
|---|---|---|
| `subtype` | string | Event category (see below) |
| `content` | string | Human-readable description |
| `level` | string | Severity: `"info"`, `"warning"`, `"error"` |

**System message subtypes**:

| Subtype | Description |
|---|---|
| `informational` | General informational message |
| `permission_retry` | Permission was denied, retrying |
| `bridge_status` | Bridge connection status change |
| `scheduled_task_fire` | A scheduled task was triggered |
| `stop_hook_summary` | Summary from a Stop hook execution |
| `turn_duration` | Turn timing information |
| `away_summary` | Summary of activity while user was away |
| `memory_saved` | Memory was persisted |
| `agents_killed` | Subagents were terminated |
| `api_metrics` | API performance metrics |
| `local_command` | Local shell command was executed |
| `compact_boundary` | Marks a compaction point in the transcript |
| `microcompact_boundary` | Lighter compaction that clears specific tool results |
| `api_error` | API error encountered |

### `tombstone`

Marks a message as deleted or invalidated. Used for message deletion and content replacement workflows.

```json
{
  "type": "tombstone",
  "targetUuid": "uuid-of-deleted-message",
  ...universal fields
}
```

### `progress`

Hook lifecycle events emitted by the plugin/hook system. **Note**: `progress` messages are NOT persisted to the transcript. They are transient events used during execution. Old transcripts may contain `progress` entries from earlier versions, but these are bridged/skipped on load.

```json
{
  "type": "progress",
  "hookEvent": "PreToolUse",
  "toolName": "Bash",
  "hookResult": { "decision": "approve" }
}
```

Hook event types:
- `SessionStart` - Session initialization
- `PreToolUse` - Before a tool executes (can block/approve)
- `PostToolUse` - After a tool completes
- `Stop` - End of assistant turn (used by auto-mode)

### `queue-operation`

Tracks queue state changes (relevant to auto-mode and message queuing).

```json
{
  "type": "queue-operation",
  "operation": "enqueue",
  "message": "Continue with the next step",
  ...universal fields
}
```

### `file-history-snapshot`

Captures file state references for undo/restore functionality. Stores backup references, not inline content.

```json
{
  "type": "file-history-snapshot",
  "messageId": "uuid-of-associated-message",
  "trackedFileBackups": [ ... ],
  "timestamp": "2026-03-29T10:15:30.123Z",
  ...universal fields
}
```

| Field | Type | Description |
|---|---|---|
| `messageId` | string | UUID of the message this snapshot is associated with |
| `trackedFileBackups` | array | List of backup file references |
| `timestamp` | string | When the snapshot was taken |

### Metadata-only types

These carry no conversational content. They store UI/session state inline in the JSONL file:

| Type | Purpose | Key field(s) |
|---|---|---|
| `pr-link` | GitHub PR URL associated with session | `url` |
| `custom-title` | User-set conversation title | `title` |
| `agent-name` | Display name for the agent | `name` |
| `agent-color` | UI color preference | `color` |
| `last-prompt` | Caches the most recent user prompt | `prompt` |
| `ai-title` | AI-generated conversation title | `title` |
| `task-summary` | AI-generated summary of the task | `summary` |
| `tag` | Tags/labels applied to the session | `tag` |
| `agent-setting` | Agent configuration change | setting key/value |
| `attribution-snapshot` | Snapshot of file attribution state | attribution data |
| `speculation-accept` | Speculative execution acceptance | speculation data |
| `mode` | Mode change record (e.g. auto-mode toggle) | `mode` |
| `worktree-state` | Git worktree state snapshot | worktree data |
| `content-replacement` | Content replacement/substitution record | replacement data |
| `marble-origami-commit` | Commit event in marble-origami workflow | commit data |
| `marble-origami-snapshot` | Snapshot in marble-origami workflow | snapshot data |

---

## 5. Tool Call Format

Tool interactions span two messages: an `assistant` message containing `tool_use` blocks, followed by a `user` message containing `tool_result` blocks.

### Tool Use (assistant -> tool)

Appears as a content block inside an `assistant` message's `message.content` array:

```json
{
  "type": "tool_use",
  "id": "toolu_01ABC123...",
  "name": "Bash",
  "input": {
    "command": "cargo test",
    "timeout": 30000
  }
}
```

The `id` field is the correlation key linking the request to its result.

### Tool Result (tool -> user)

The subsequent `user` message delivers results back. `content` is an array:

```json
{
  "type": "user",
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_01ABC123...",
      "content": "running 12 tests\ntest result: ok. 12 passed; 0 failed"
    }
  ],
  ...universal fields
}
```

`tool_result.content` can be:
- A **string** for text output
- An **array** of content blocks (e.g. text + image for screenshot tools)

### toolUseResult metadata

Some tool result messages include a `toolUseResult` object with execution metadata:

```json
{
  "type": "user",
  "content": [ ... ],
  "toolUseResult": {
    "tool_use_id": "toolu_01ABC123...",
    "name": "Bash",
    "success": true,
    "interrupted": false,
    "duration_ms": 2340
  },
  ...universal fields
}
```

| Field | Type | Description |
|---|---|---|
| `tool_use_id` | string | Correlates with the `tool_use` block |
| `name` | string | Tool name |
| `success` | boolean | Whether the tool completed without error |
| `interrupted` | boolean | `true` if user interrupted execution (Ctrl+C, Escape) |
| `duration_ms` | number | Wall-clock execution time |

---

## 6. Token Tracking

Every `assistant` message includes a `usage` object nested inside `message`:

```json
{
  "type": "assistant",
  "message": {
    "usage": {
      "input_tokens": 15234,
      "output_tokens": 892,
      "cache_creation_input_tokens": 4096,
      "cache_read_input_tokens": 11138,
      "server_tool_use": {
        "web_search_requests": 2,
        "web_fetch_requests": 1
      },
      "service_tier": "standard",
      "cache_creation": {
        "ephemeral_1h_input_tokens": 1024,
        "ephemeral_5m_input_tokens": 512
      },
      "inference_geo": "us"
    },
    ...
  }
}
```

| Field | Type | Description |
|---|---|---|
| `input_tokens` | number | Non-cached input tokens consumed |
| `output_tokens` | number | Tokens generated in the response |
| `cache_creation_input_tokens` | number | Tokens written to prompt cache this turn |
| `cache_read_input_tokens` | number | Tokens served from prompt cache |
| `server_tool_use` | object | Server-side tool usage counts (`web_search_requests`, `web_fetch_requests`) |
| `service_tier` | string | API service tier used for the request |
| `cache_creation` | object | Granular cache creation breakdown (`ephemeral_1h_input_tokens`, `ephemeral_5m_input_tokens`) |
| `inference_geo` | string | Geographic region where inference ran |

Additional session-level tracking fields may appear on assistant messages:

| Field | Type | Description |
|---|---|---|
| `iterations` | number | Number of agentic loop iterations in this turn |
| `speed` | number | Tokens per second |

**Cost calculation**: Total input = `input_tokens + cache_creation_input_tokens + cache_read_input_tokens`. Cache reads are billed at reduced rate. Cache creation tokens are billed at a premium over standard input.

Summing `usage` objects across all `assistant` messages in a transcript gives total session consumption.

---

## 7. Session Management

### Session identification

Each conversation is identified by a v4 UUID, stored as `sessionId` in every message. The same UUID names the `.jsonl` file.

### Continuing sessions

The `--continue` flag (or `-c`) resumes the most recent session. Claude Code:
1. Scans the project directory (`~/.claude/projects/<path>/`) for `.jsonl` files
2. Selects the most recently modified file by **filesystem modification time**
3. Loads the corresponding `.jsonl` file
4. Replays the conversation into context
5. Appends new messages to the same file

**Note**: `history.jsonl` is NOT used by `--continue`. It serves only as the prompt history index for the fuzzy session picker. Session resumption is based on filesystem mtime of `.jsonl` transcript files.

A specific session can be resumed by `--session <UUID>`.

### Subagent sessions

When Claude Code spawns a subagent (e.g. via the Agent tool), the subagent's transcript is stored in a subdirectory:

```
<SESSION_UUID>/subagents/agent-<ID>.jsonl
<SESSION_UUID>/subagents/agent-<ID>.meta.json
```

The `<ID>` is an incrementing integer. Subagent transcripts follow the same JSONL schema. The `.meta.json` file contains agent metadata (name, role, configuration). The parent transcript references subagent results through tool_result messages.

### Session lifecycle

```
Process starts
  -> Creates ~/.claude/sessions/<PID>.json
  -> Creates or appends to ~/.claude/projects/<path>/<UUID>.jsonl
  -> Appends to ~/.claude/history.jsonl (for prompt search)

During session
  -> Updates ~/.claude/sessions/<PID>.json (status, updatedAt)
  -> Writes are batched in memory, flushed every 100ms

Process exits
  -> Removes ~/.claude/sessions/<PID>.json
  -> Transcript file remains permanently
```

---

## 8. Project Organization

Claude Code scopes conversations and configuration by project path.

```
~/.claude/projects/-home-me-ht-unleash/
├── *.jsonl                    # Conversation transcripts
├── */subagents/               # Subagent transcripts + meta files
├── memory/
│   └── MEMORY.md             # Auto-persisted learnings
└── settings.json             # Project-level settings (optional)
```

### Project-level settings

If `~/.claude/projects/<path>/settings.json` exists, it is merged over the global `~/.claude/settings.json`. This allows per-project tool permissions, hook configurations, and MCP server definitions.

### Project-level CLAUDE.md

The `CLAUDE.md` file in the project root (and any parent directories) is automatically loaded as system context. Additionally, `~/.claude/projects/<path>/CLAUDE.md` provides project-scoped instructions that are not checked into the repo.

---

## 9. Configuration and Retention

### Configuration hierarchy (highest priority first)

1. `~/.claude/projects/<path>/settings.json` - Project-level overrides
2. `~/.claude/settings.json` - User-level settings
3. `~/.claude.json` - Legacy/alternative user config location
4. Defaults built into Claude Code

### Key settings affecting storage

```json
{
  "permissions": {
    "allow": ["Bash(git *)"],
    "deny": []
  },
  "hooks": {
    "PreToolUse": [...],
    "PostToolUse": [...],
    "Stop": [...]
  }
}
```

### Retention policy

**There is no automatic cleanup.** Transcript files accumulate indefinitely. unleash or external tooling must handle pruning if disk usage becomes a concern.

Typical transcript sizes:
- Short conversation: 50-200 KB
- Long coding session: 5-50 MB
- Sessions with image content: 50-500 MB (base64 encoded)

---

## 10. Multimodal Content

Images appear as content blocks within message arrays, typically in `tool_result` content for screenshot tools.

### Image content block

```json
{
  "type": "image",
  "source": {
    "type": "base64",
    "media_type": "image/png",
    "data": "iVBORw0KGgoAAAANSUhEUgAA..."
  }
}
```

### In context: screenshot tool result

```json
{
  "type": "user",
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_01SCREENSHOT...",
      "content": [
        {
          "type": "text",
          "text": "Screenshot captured (1920x1080)"
        },
        {
          "type": "image",
          "source": {
            "type": "base64",
            "media_type": "image/png",
            "data": "iVBORw0KGgoAAAANSUhEUgAA..."
          }
        }
      ]
    }
  ],
  ...universal fields
}
```

### Storage implications

Images are stored inline as base64 in the JSONL transcript, with additional caching in `~/.claude/image-cache/<session-id>/`. A single screenshot can add 1-5 MB to the transcript. This is the primary driver of large transcript files.

When reading transcripts programmatically, be prepared for very long lines containing base64 image data.

---

## 11. Error and Interrupted States

### Interrupted tool execution

When the user interrupts a running tool (Ctrl+C, Escape, or timeout), the `toolUseResult` metadata flags it:

```json
{
  "toolUseResult": {
    "tool_use_id": "toolu_01ABC...",
    "name": "Bash",
    "success": false,
    "interrupted": true,
    "duration_ms": 5000
  }
}
```

The `tool_result` content block will contain whatever partial output was captured before interruption.

### Truncated content

Long tool outputs may be truncated by Claude Code before storage. When this happens, the text content ends with a truncation marker (typically indicating bytes omitted). The full output is not recoverable from the transcript.

### Incomplete transcripts

If Claude Code crashes or is killed (SIGKILL), the last line of the JSONL file may be:
- Truncated (invalid JSON) - the line was being written when the process died
- Missing entirely - the message was buffered but not flushed (writes are batched at 100ms intervals)

Parsers should handle malformed trailing lines gracefully. All preceding lines will be valid JSON.

### Error tool results

Failed tool executions produce an `is_error` field in the tool_result:

```json
{
  "type": "tool_result",
  "tool_use_id": "toolu_01ABC...",
  "content": "Error: ENOENT: no such file or directory",
  "is_error": true
}
```

### Stop reasons

The `stop_reason` field on assistant messages (inside `message`) indicates why generation stopped:

| Value | Meaning |
|---|---|
| `"end_turn"` | Model finished its response naturally |
| `"tool_use"` | Model wants to call a tool |
| `"max_tokens"` | Hit output token limit |
| `"stop_sequence"` | Hit a stop sequence |

---

## Appendix: Parsing Tips for unleash Developers

1. **Line-by-line processing**: Each JSONL line is independent. Use streaming parsers for large files.
2. **Type dispatch**: Switch on the `type` field first, then handle type-specific fields. Only `user`, `assistant`, `attachment`, and `system` are transcript messages; other types are metadata/state entries.
3. **Reconstruct conversations**: Follow `parentUuid` chains to build the message tree. `isSidechain` marks branched explorations. Use `logicalParentUuid` when available for logical ordering.
4. **Correlate tool calls**: Match `tool_use.id` in assistant messages to `tool_result.tool_use_id` in the next user message.
5. **Calculate costs**: Sum `usage` objects (inside `message`) across all assistant messages. Apply per-model pricing.
6. **Handle images**: Skip or truncate base64 `data` fields when you don't need image content. They dominate file size.
7. **Detect active sessions**: Cross-reference `~/.claude/sessions/*.json` with transcript `sessionId` values to find live conversations. Check the `status` field for busy/idle/waiting state.
8. **promptId grouping**: Messages sharing the same `promptId` belong to the same request-response exchange (user prompt + assistant reply + tool calls within that turn).
9. **Handle tombstones**: When processing transcripts, check for `tombstone` entries that invalidate earlier messages by `targetUuid`.
10. **Write consistency**: Due to 100ms write batching, the on-disk file may lag slightly behind the in-memory state. Account for this when reading transcripts of active sessions.
11. **Progress entries**: Skip `progress` type entries when loading transcripts. They are not part of the conversation and are only present in older transcripts.
12. **Assistant message nesting**: Remember that `role`, `content`, `model`, `usage`, and `stop_reason` are inside `message`, not at the top level of assistant entries.
