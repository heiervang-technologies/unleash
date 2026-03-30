# Google Gemini CLI - Conversation Storage Format Reference

> Last verified: 2026-03-29, Gemini CLI v0.35.3

Internal developer reference for unleash. Documents the on-disk storage format used by Google's Gemini CLI for conversation history, session management, and related metadata.

---

## 1. Storage Location

Gemini CLI stores all conversation data under the user's home directory:

```
~/.gemini/
├── tmp/
│   └── <project_hash>/
│       ├── chats/
│       │   ├── session-2026-03-29T14-22-abc123.json
│       │   └── session-2026-03-28T09-11-def456.json
│       └── logs.json
├── settings.json
├── projects.json
├── oauth_creds.json
└── installation_id
```

**Primary paths:**

| Path | Purpose |
|------|---------|
| `~/.gemini/tmp/<project_hash>/chats/` | Session files (one per conversation) |
| `~/.gemini/tmp/<project_hash>/logs.json` | Aggregated log index for the project |
| `~/.gemini/settings.json` | Global user settings |
| `~/.gemini/projects.json` | Project-level configuration overrides |
| `~/.gemini/oauth_creds.json` | OAuth credentials for Google auth |
| `~/.gemini/installation_id` | Unique installation identifier |

The `<project_hash>` is a SHA-256 hash derived from the project root path (see Section 8).

---

## 2. File Format

Session files are **standard JSON** (not JSONL). Each file contains a single JSON object representing one complete conversation session.

The logs index (`logs.json`) is also standard JSON, containing an array of log entry objects.

There is no streaming/append format -- the entire session file is rewritten on each update. This means partial reads during writes could yield invalid JSON (no fsync guarantees documented).

**Encoding:** UTF-8, no BOM.

**Pretty-printing:** Session files are typically written with 2-space indentation.

---

## 3. Message Schema

Each session file has the following top-level structure:

```json
{
  "sessionId": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "projectHash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "startTime": "2026-03-29T14:22:33.456Z",
  "lastUpdated": "2026-03-29T14:35:12.789Z",
  "messages": [
    {
      "id": "msg-001",
      "timestamp": "2026-03-29T14:22:33.456Z",
      "type": "user",
      "content": "Explain the project structure",
      "thoughts": [],
      "tokens": {
        "input": 0,
        "output": 0,
        "cached": 0,
        "thoughts": 0,
        "tool": 0,
        "total": 0
      },
      "model": null,
      "toolCalls": []
    },
    {
      "id": "msg-002",
      "timestamp": "2026-03-29T14:22:35.123Z",
      "type": "gemini",
      "content": "This project uses a standard Node.js layout...",
      "thoughts": [
        {
          "subject": "project_analysis",
          "description": "Examining directory structure and package.json",
          "timestamp": "2026-03-29T14:22:34.500Z"
        }
      ],
      "tokens": {
        "input": 1245,
        "output": 387,
        "cached": 800,
        "thoughts": 52,
        "tool": 0,
        "total": 2484
      },
      "model": "gemini-2.5-pro",
      "toolCalls": []
    }
  ]
}
```

**Top-level fields:**

| Field | Type | Description |
|-------|------|-------------|
| `sessionId` | `string` (UUID v4) | Unique session identifier |
| `projectHash` | `string` (SHA-256 hex) | Hash of the project root path |
| `startTime` | `string` (ISO 8601) | When the session was created |
| `lastUpdated` | `string` (ISO 8601) | Timestamp of the most recent message or state change |
| `messages` | `array` | Ordered array of message objects |

**Per-message fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | `string` | Yes | Unique message identifier within the session |
| `timestamp` | `string` (ISO 8601) | Yes | When the message was created |
| `type` | `string` | Yes | One of: `"user"`, `"gemini"`, `"info"` |
| `content` | `string` | Yes | The message text content |
| `thoughts` | `array` | Yes | Model thinking steps (see below); empty array if none |
| `tokens` | `object` | Yes | Token usage breakdown (see Section 6) |
| `model` | `string \| null` | Yes | Model identifier (e.g. `"gemini-2.5-pro"`); `null` for user/info messages |
| `toolCalls` | `array` | Yes | Tool invocations and results (see Section 5); empty array if none |

---

## 4. Message Types

Three message types exist:

### `"user"`
Human input. The `content` field holds the user's text. `model` is `null`. `thoughts` and `toolCalls` are empty arrays. `tokens` fields are typically all zero.

### `"gemini"`
Model response. Contains the assistant's reply in `content`. May include populated `thoughts`, `toolCalls`, and `tokens`. The `model` field identifies which Gemini model generated the response.

### `"info"`
System-generated informational messages. Used for status updates, session metadata notes, or CLI-injected context. `model` is `null`. Examples include tool execution status summaries and session lifecycle events.

---

## 5. Tool Call Format

When the model invokes tools, the `toolCalls` array on the `"gemini"` message is populated:

```json
{
  "toolCalls": [
    {
      "id": "tc-abc123",
      "name": "read_file",
      "args": {
        "path": "src/index.ts",
        "offset": 0,
        "limit": 100
      },
      "result": [
        {
          "functionResponse": {
            "id": "tc-abc123",
            "name": "read_file",
            "response": {
              "output": "import express from 'express';\n...",
              "error": null
            }
          }
        }
      ],
      "status": "success",
      "timestamp": "2026-03-29T14:22:36.789Z",
      "displayName": "Read File",
      "description": "Read the contents of src/index.ts"
    }
  ]
}
```

**Tool call fields:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique tool call identifier, used to correlate request and response |
| `name` | `string` | Internal tool function name (e.g. `"read_file"`, `"run_command"`, `"write_file"`, `"search"`) |
| `args` | `object` | Arguments passed to the tool, schema varies per tool |
| `result` | `array` | Array of result objects, each containing a `functionResponse` |
| `status` | `string` | Execution status: `"success"`, `"error"`, `"timeout"`, `"cancelled"` |
| `timestamp` | `string` (ISO 8601) | When the tool call was initiated |
| `displayName` | `string` | Human-readable tool name for UI display |
| `description` | `string` | Human-readable summary of what the tool call does |

**`functionResponse` structure:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Matches the parent tool call `id` |
| `name` | `string` | Matches the parent tool call `name` |
| `response.output` | `string \| null` | Tool output on success |
| `response.error` | `string \| null` | Error message on failure |

When a tool errors, `response.output` is `null` and `response.error` contains the error string. Conversely, on success `response.error` is `null`.

Multiple tool calls in a single turn produce multiple entries in the `toolCalls` array.

---

## 6. Token Tracking

Every message includes a `tokens` object with fine-grained usage breakdown:

```json
{
  "tokens": {
    "input": 1245,
    "output": 387,
    "cached": 800,
    "thoughts": 52,
    "tool": 0,
    "total": 2484
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `input` | `number` | Input/prompt tokens consumed |
| `output` | `number` | Output/completion tokens generated |
| `cached` | `number` | Tokens served from context cache (subset of input) |
| `thoughts` | `number` | Tokens used for internal reasoning/thinking steps |
| `tool` | `number` | Tokens consumed by tool call arguments and results |
| `total` | `number` | Sum of all token categories |

For `"user"` and `"info"` messages, all token fields are typically `0`. Token counts are only meaningful on `"gemini"` messages.

---

## 7. Session Management

### Session ID

Each session receives a UUID v4 identifier stored in the top-level `sessionId` field.

### Filename Convention

Session files follow this naming pattern:

```
session-<ISO_TIMESTAMP>-<uuid6>.json
```

Where:
- `<ISO_TIMESTAMP>` is the session start time formatted as `YYYY-MM-DDTHH-mm` (colons replaced with hyphens for filesystem compatibility)
- `<uuid6>` is the first 6 characters of the session UUID

Example: `session-2026-03-29T14-22-a1b2c3.json`

### Timestamps

- `startTime`: Set once when the session is created, never modified
- `lastUpdated`: Updated each time a new message is appended or session state changes

Both use ISO 8601 format with millisecond precision and UTC timezone (`Z` suffix).

### Logs Index

The `logs.json` file in each project directory provides a flat index across all sessions:

```json
[
  {
    "sessionId": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "messageId": "msg-001",
    "type": "user",
    "message": "Explain the project structure",
    "timestamp": "2026-03-29T14:22:33.456Z"
  },
  {
    "sessionId": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "messageId": "msg-002",
    "type": "gemini",
    "message": "This project uses a standard Node.js layout...",
    "timestamp": "2026-03-29T14:22:35.123Z"
  }
]
```

This index enables cross-session search without parsing every session file. It contains a subset of fields (no tokens, thoughts, or toolCalls).

---

## 8. Project Organization

### Project Hash

Gemini CLI identifies projects by computing a SHA-256 hash of the absolute project root path:

```
SHA256("/home/user/projects/my-app") -> "e3b0c44298fc1c14..."
```

This hash becomes the directory name under `~/.gemini/tmp/`. The hashing ensures:
- No filesystem-unsafe characters in directory names
- Consistent mapping from path to storage location
- Privacy (project paths not exposed in directory listings)

### Directory Structure per Project

```
~/.gemini/tmp/<project_hash>/
├── chats/                    # All session files for this project
│   ├── session-*.json
│   └── ...
└── logs.json                 # Aggregated log index
```

Each project gets its own isolated directory. There is no cross-project index file; discovering all projects requires listing `~/.gemini/tmp/` and reverse-mapping hashes (which is not straightforward without a lookup table -- `projects.json` in the root may serve this purpose).

---

## 9. Configuration and Retention

### Global Settings

`~/.gemini/settings.json` holds user-level configuration:

```json
{
  "theme": "dark",
  "model": "gemini-2.5-pro",
  "sessionRetention": 30,
  "sandbox": true
}
```

Key configuration fields relevant to storage:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `sessionRetention` | `number` | `30` | Days to retain session files before automatic cleanup |
| `model` | `string` | varies | Default model for new sessions |

### Retention Behavior

Session files older than `sessionRetention` days (based on `lastUpdated`) are eligible for automatic deletion. Cleanup appears to run on CLI startup. The `logs.json` index entries for deleted sessions are also pruned.

### Export

The `/chat share` command exports a session to a standalone file:

```bash
/chat share output.json    # Full JSON export
/chat share output.md      # Markdown-formatted export
```

The JSON export produces a self-contained file with the same schema as the session file. The Markdown export renders messages as a human-readable conversation transcript.

### Other Config Files

| File | Purpose |
|------|---------|
| `~/.gemini/projects.json` | Per-project configuration overrides (model, settings) |
| `~/.gemini/oauth_creds.json` | Google OAuth tokens for authentication |
| `~/.gemini/installation_id` | Plain text file with a unique installation UUID |

---

## 10. Multimodal Content

When images or other media are included in messages, they appear as inline data within the `content` field or as structured content parts:

```json
{
  "id": "msg-003",
  "type": "user",
  "content": [
    {
      "type": "text",
      "text": "What does this screenshot show?"
    },
    {
      "type": "inlineData",
      "mimeType": "image/png",
      "data": "<base64-encoded-image-data>"
    }
  ],
  "timestamp": "2026-03-29T14:30:00.000Z",
  "thoughts": [],
  "tokens": {},
  "model": null,
  "toolCalls": []
}
```

**Key observations:**

- When multimodal content is present, `content` becomes an **array of content parts** rather than a plain string. Consumers must handle both `string` and `array` types for the `content` field.
- Each content part has a `type` discriminator: `"text"` for text, `"inlineData"` for embedded media.
- `inlineData` parts include `mimeType` (MIME type string) and `data` (base64-encoded binary).
- Images from tool results (e.g. screenshots) may also appear as `inlineData` within tool call results.
- Large media payloads inflate session file sizes significantly. There does not appear to be external blob storage -- everything is inline.

---

## 11. Error and Interrupted States

### Partial Sessions

If the CLI is interrupted mid-conversation (e.g. Ctrl+C, crash, network failure), the session file reflects the state at the last successful write:

- Messages already written are preserved
- A partially streamed model response may be absent entirely (if the write hadn't occurred yet) or present with truncated content
- The `lastUpdated` timestamp reflects the last successful write, not the interruption time
- Tool calls in progress at interruption time may appear with `status: "cancelled"` or may be absent from the session file

### Error States in Tool Calls

Tool calls that fail during execution are recorded with:
- `status` set to `"error"` or `"timeout"`
- `result[].functionResponse.response.error` containing the error message
- `result[].functionResponse.response.output` set to `null`

### Session Recovery

There is no explicit crash recovery or WAL (write-ahead log) mechanism. The CLI does not attempt to resume interrupted sessions automatically. The user can continue from the last saved state by resuming the session, but any unsaved messages are lost.

### Empty Sessions

Sessions where the user exits before sending any messages may still produce a session file with an empty `messages` array. These are subject to normal retention cleanup.

---

## Appendix: Comparison Notes for unleash Developers

When building format adapters or migration tooling, note these key differences from Claude Code's format:

| Aspect | Claude Code | Gemini CLI |
|--------|-------------|------------|
| File format | JSONL (one object per line) | JSON (single object per file) |
| Storage path | `~/.claude/projects/<path-encoded>/` | `~/.gemini/tmp/<sha256-hash>/chats/` |
| Message types | `user`, `assistant`, `system`, `tool_result` | `user`, `gemini`, `info` |
| Tool results | Separate `tool_result` messages | Nested inside `toolCalls[].result` on the model message |
| Token tracking | Per-conversation summary | Per-message granular breakdown |
| Logs index | None (single JSONL file is the index) | Separate `logs.json` aggregation file |
| Multimodal | External file references | Inline base64 in content array |
| Thinking | `thinking` content blocks | `thoughts` array with subject/description |

---

*This document is for internal unleash development use. Not intended for end-user distribution.*
