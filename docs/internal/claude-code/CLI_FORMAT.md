# Claude Code Conversation Storage Format

> **Internal developer reference for unleash contributors.**
> Last verified: 2026-03-29, Claude Code v2.1.87

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
│       └── memory/
│           └── MEMORY.md              # Auto-persisted project memory
├── sessions/                          # Active session metadata
│   └── <PID>.json                     # One file per running process
├── history.jsonl                      # Global conversation index
├── settings.json                      # User-level settings
└── CLAUDE.md                          # User-level global instructions
```

### Path encoding

Project directories use the absolute path with `/` replaced by `-` and the leading slash dropped. Examples:

| Actual path | Encoded directory name |
|---|---|
| `/home/me/ht/unleash` | `-home-me-ht-unleash` |
| `/home/me/projects/foo` | `-home-me-projects-foo` |

---

## 2. File Format

### Conversation files

Each conversation is a single **JSONL** (JSON Lines) file named `<UUID>.jsonl`. Every line is a self-contained JSON object representing one message or event.

```
~/.claude/projects/-home-me-ht-unleash/a1b2c3d4-e5f6-7890-abcd-ef1234567890.jsonl
```

UUIDs are v4 format. File size grows unbounded for the lifetime of a session; there is no rotation or splitting.

### History index

`~/.claude/history.jsonl` is a separate JSONL file where each line records a past conversation for display in the session picker. Fields:

| Field | Type | Description |
|---|---|---|
| `display` | string | First user message (truncated), shown in picker UI |
| `pastedContents` | string[] | Any pasted text from that first message |
| `timestamp` | string | ISO 8601 timestamp |
| `project` | string | Encoded project path |
| `sessionId` | string | UUID linking to the `.jsonl` transcript |

### Session metadata

`~/.claude/sessions/<PID>.json` tracks each running Claude Code process:

| Field | Type | Description |
|---|---|---|
| `pid` | number | OS process ID |
| `sessionId` | string | UUID of the active conversation |
| `cwd` | string | Working directory |
| `startedAt` | string | ISO 8601 start time |
| `kind` | string | Session type (e.g. `"interactive"`, `"headless"`) |
| `entrypoint` | string | How Claude Code was launched (e.g. `"cli"`, `"sdk"`) |

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
| `timestamp` | string | yes | ISO 8601 with milliseconds |
| `sessionId` | string | yes | Session UUID this message belongs to |
| `version` | string | yes | Claude Code version that wrote this line |
| `cwd` | string | yes | Working directory at time of message |
| `gitBranch` | string | no | Current git branch (absent outside git repos) |
| `isSidechain` | boolean | yes | `true` if this message is part of a branched/sidechain conversation |
| `userType` | string | yes | `"external"` for human, `"internal"` for system-generated |
| `promptId` | string | no | Links related request/response pairs in the same exchange |

---

## 4. Message Types

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

### `assistant`

Model response. Contains the full API response envelope.

```json
{
  "type": "assistant",
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
  "stop_reason": "tool_use",
  ...universal fields
}
```

Key fields:

| Field | Type | Description |
|---|---|---|
| `model` | string | Full model identifier |
| `id` | string | Anthropic API message ID (`msg_...`) |
| `content` | array | Ordered content blocks (thinking, text, tool_use) |
| `usage` | object | Token counts (see Section 6) |
| `stop_reason` | string | `"end_turn"`, `"tool_use"`, `"max_tokens"`, `"stop_sequence"` |

### `system`

Internal system events, not shown to users.

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
| `subtype` | string | Event category (e.g. `"local_command"`) |
| `content` | string | Human-readable description |
| `level` | string | Severity: `"info"`, `"warning"`, `"error"` |

### `progress`

Hook lifecycle events emitted by the plugin/hook system.

```json
{
  "type": "progress",
  "hookEvent": "PreToolUse",
  "toolName": "Bash",
  "hookResult": { "decision": "approve" },
  ...universal fields
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

Captures file state for undo/restore functionality.

```json
{
  "type": "file-history-snapshot",
  "files": {
    "/home/me/ht/unleash/src/lib.rs": {
      "content": "...",
      "hash": "abc123..."
    }
  },
  ...universal fields
}
```

### Metadata-only types

These carry no conversational content. They store UI/session state inline in the transcript:

| Type | Purpose | Key field |
|---|---|---|
| `pr-link` | GitHub PR URL associated with session | `url` |
| `custom-title` | User-set conversation title | `title` |
| `agent-name` | Display name for the agent | `name` |
| `agent-color` | UI color preference | `color` |
| `last-prompt` | Caches the most recent user prompt | `prompt` |

---

## 5. Tool Call Format

Tool interactions span two messages: an `assistant` message containing `tool_use` blocks, followed by a `user` message containing `tool_result` blocks.

### Tool Use (assistant -> tool)

Appears as a content block inside an `assistant` message:

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

Every `assistant` message includes a `usage` object:

```json
{
  "usage": {
    "input_tokens": 15234,
    "output_tokens": 892,
    "cache_creation_input_tokens": 4096,
    "cache_read_input_tokens": 11138
  }
}
```

| Field | Type | Description |
|---|---|---|
| `input_tokens` | number | Non-cached input tokens consumed |
| `output_tokens` | number | Tokens generated in the response |
| `cache_creation_input_tokens` | number | Tokens written to prompt cache this turn |
| `cache_read_input_tokens` | number | Tokens served from prompt cache |

**Cost calculation**: Total input = `input_tokens + cache_creation_input_tokens + cache_read_input_tokens`. Cache reads are billed at reduced rate. Cache creation tokens are billed at a premium over standard input.

Summing `usage` across all `assistant` messages in a transcript gives total session consumption.

---

## 7. Session Management

### Session identification

Each conversation is identified by a v4 UUID, stored as `sessionId` in every message. The same UUID names the `.jsonl` file.

### Continuing sessions

The `--continue` flag (or `-c`) resumes the most recent session. Claude Code:
1. Reads `~/.claude/history.jsonl` to find the latest `sessionId`
2. Loads the corresponding `.jsonl` file
3. Replays the conversation into context
4. Appends new messages to the same file

A specific session can be resumed by `--session <UUID>`.

### Subagent sessions

When Claude Code spawns a subagent (e.g. via the Agent tool), the subagent's transcript is stored in a subdirectory:

```
<SESSION_UUID>/subagents/agent-<ID>.jsonl
```

The `<ID>` is an incrementing integer. Subagent transcripts follow the same JSONL schema. The parent transcript references subagent results through tool_result messages.

### Session lifecycle

```
Process starts
  → Creates ~/.claude/sessions/<PID>.json
  → Creates or appends to ~/.claude/projects/<path>/<UUID>.jsonl
  → Appends to ~/.claude/history.jsonl

Process exits
  → Removes ~/.claude/sessions/<PID>.json
  → Transcript file remains permanently
```

---

## 8. Project Organization

Claude Code scopes conversations and configuration by project path.

```
~/.claude/projects/-home-me-ht-unleash/
├── *.jsonl                    # Conversation transcripts
├── */subagents/               # Subagent transcripts
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

Images are stored inline as base64 in the JSONL transcript. There is no external blob storage or deduplication. A single screenshot can add 1-5 MB to the transcript. This is the primary driver of large transcript files.

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
- Missing entirely - the message was buffered but not flushed

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

The `stop_reason` field on assistant messages indicates why generation stopped:

| Value | Meaning |
|---|---|
| `"end_turn"` | Model finished its response naturally |
| `"tool_use"` | Model wants to call a tool |
| `"max_tokens"` | Hit output token limit |
| `"stop_sequence"` | Hit a stop sequence |

---

## Appendix: Parsing Tips for unleash Developers

1. **Line-by-line processing**: Each JSONL line is independent. Use streaming parsers for large files.
2. **Type dispatch**: Switch on the `type` field first, then handle type-specific fields.
3. **Reconstruct conversations**: Follow `parentUuid` chains to build the message tree. `isSidechain` marks branched explorations.
4. **Correlate tool calls**: Match `tool_use.id` in assistant messages to `tool_result.tool_use_id` in the next user message.
5. **Calculate costs**: Sum `usage` objects across all assistant messages. Apply per-model pricing.
6. **Handle images**: Skip or truncate base64 `data` fields when you don't need image content. They dominate file size.
7. **Detect active sessions**: Cross-reference `~/.claude/sessions/*.json` with transcript `sessionId` values to find live conversations.
8. **promptId grouping**: Messages sharing the same `promptId` belong to the same request-response exchange (user prompt + assistant reply + tool calls within that turn).
