# Internal Developer Documentation

Reference documentation for unleash developers. Not user-facing.

## Contents

### Claude Code (`claude-code/`)

- [CLI Format](claude-code/CLI_FORMAT.md) — JSONL transcripts, 12+ message types, path encoding, source-verified
- [XML Message Signatures](claude-code/XML_MESSAGE_SIGNATURES.md) — Native XML tags for custom rendering and agent messages
- [Hooks](claude-code/HOOKS.md) — All 27 hook events, 4 types, input/output schemas, asyncRewake, once flag
- [Telemetry](claude-code/TELEMETRY.md) — All telemetry systems, env vars to block them, Statsig + OTEL details
- [Plugins](claude-code/PLUGINS.md) — `--plugin-dir` internals, manifest schema, hooks.json format, `${CLAUDE_PLUGIN_ROOT}`
- [Tools](claude-code/TOOLS.md) — Built-in tool names, input schemas, hook matcher patterns, allowed-tools
- [Plugin Development Guide](claude-code/plugin-development.md) — Developer workflow: scaffolding, testing, PR process

### Codex (`codex/`)

- [CLI Format](codex/CLI_FORMAT.md) — JSONL + SQLite hybrid, event stream model

### Gemini CLI (`gemini/`)

- [CLI Format](gemini/CLI_FORMAT.md) — JSON sessions, thoughts, project hashing

### OpenCode (`opencode/`)

- [CLI Format](opencode/CLI_FORMAT.md) — SQLite + Drizzle ORM, message/part separation

---

## CLI Format Comparison

Cross-CLI comparison of conversation storage formats. See individual docs for full details.

### Format Summary

| CLI | Primary Format | Storage Path | Session ID | File Naming |
|-----|---------------|-------------|------------|-------------|
| Claude Code | JSONL | `~/.claude/projects/<path>/` | UUID | `<session-uuid>.jsonl` |
| Codex | JSONL + SQLite | `~/.codex/sessions/YYYY/MM/DD/` | UUID | `rollout-<timestamp>-<uuid>.jsonl` |
| Gemini CLI | JSON | `~/.gemini/tmp/<hash>/chats/` | UUID | `session-<timestamp>-<uuid6>.json` |
| OpenCode | SQLite + JSON | `~/.local/share/opencode/` | `ses_` + base36 | SQLite rows + `storage/` files |

### Feature Comparison Matrix

| Feature | Claude Code | Codex | Gemini CLI | OpenCode |
|---------|-------------|-------|------------|----------|
| **Primary format** | JSONL | JSONL + SQLite | JSON | SQLite + JSON |
| **Message ID format** | UUID | N/A (event stream) | Sequential (`msg-NNN`) | `msg_` + base36 |
| **Thinking/reasoning** | thinking blocks (signed) | event_msg reasoning type | thoughts array | reasoning part type |
| **Tool calls** | content block (tool_use) | response_item event | toolCalls array | tool part type |
| **Tool results** | content block (tool_result) | response_item (role:tool) | nested in toolCalls | tool part (state.output) |
| **Token tracking** | usage object per response | token_count events | tokens per message | tokens in message data |
| **Cache tracking** | cache_creation + cache_read | cached_input_tokens | cached field | cache.read + cache.write |
| **Git metadata** | gitBranch per message | git_sha, branch, origin_url | No | No |
| **File history** | file-history-snapshot type | No | No | patch part type |
| **Sidechain/branching** | isSidechain flag | No | No | parentID |
| **Project scoping** | path-encoded directory | date hierarchy | SHA256 project hash | SHA1 project hash |
| **Retention config** | No auto-cleanup | max_bytes compaction | 30-day default | No auto-cleanup |
| **Export support** | No native | No native | /chat share | No native |
| **History index** | history.jsonl | history.jsonl + session_index.jsonl | logs.json | SQLite queries |
| **Hook integration** | progress events | No | No | No |
| **Image support** | image content blocks (base64) | input_image type | inlineData | TBD |
| **Interruption markers** | toolUseResult.interrupted | task_complete events | No | step-finish reason |
| **Cost tracking** | No | No | No | cost field per message |

### Portable Features (present in all CLIs)

- User/assistant message roles
- Text content
- Tool calls with name + arguments
- Tool results with output
- Timestamps (ISO 8601 or Unix ms)
- Session identification
- Token usage (input + output)
- Model identification

### CLI-Specific Features (may be lost or approximated)

| Feature | CLI | Conversion Note |
|---------|-----|----------------|
| Signed thinking blocks | Claude Code | Signature is verification-only, content is portable |
| Plaintext thoughts | Gemini CLI | Directly portable as thinking content |
| Encrypted reasoning | OpenCode | Provider-dependent, may not be decryptable |
| File history snapshots | Claude Code | No equivalent in other CLIs |
| Sidechain conversations | Claude Code | Flatten to linear or drop |
| Hook progress events | Claude Code | Metadata-only, not needed for conversation replay |
| Queue operations | Claude Code | Internal scheduling, not portable |
| PR links | Claude Code | Could map to metadata in other formats |
| Turn context | Codex | Approval/sandbox policy has no equivalent |
| Git SHA + origin | Codex | Claude has branch only, others have none |
| Cost tracking | OpenCode | No equivalent (could add as metadata) |
| Session hierarchy | OpenCode | parent_id tree, no equivalent in others |
| 30-day auto-cleanup | Gemini CLI | Data may be missing in old sessions |

### Message Type Mapping

| Concept | Claude Code | Codex | Gemini CLI | OpenCode |
|---------|-------------|-------|------------|----------|
| User input | type:user, role:user | event_msg:user_message | type:user | role:user |
| AI response | type:assistant, role:assistant | event_msg:agent_message + response_item:assistant | type:gemini | role:assistant |
| System info | type:system | session_meta, turn_context | type:info | N/A |
| Tool call | assistant content: tool_use | response_item: tool_use | gemini: toolCalls[] | part type:tool |
| Tool result | user content: tool_result | response_item: tool_result | toolCalls[].result | part type:tool (state) |
| Reasoning | assistant content: thinking | event_msg: reasoning | gemini: thoughts[] | part type:reasoning |

---

## Maintenance

These docs should be updated when:
- A CLI updates its storage format (check after major version bumps)
- A new CLI is added to unleash
- Research reveals new details about a CLI's internals

Each document includes a "Last verified" date and CLI version.
