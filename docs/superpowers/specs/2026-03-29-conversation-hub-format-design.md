# Unleash Conversation Hub Format Design

**Date:** 2026-03-29
**Status:** Draft (rev 2 — QA review incorporated)
**Depends on:** Internal CLI Knowledge Base (Spec 1, PR #274)

## Problem

There is no standard format for portable AI conversation logs. Each of the 4 CLIs Unleash supports uses a different format (JSONL, JSON, SQLite+JSON). Users cannot move conversation history between CLIs. The industry lacks an interchange standard (confirmed by research — no RFC, no de facto disk format exists).

## Goal

Define an open conversation interchange format (the "Hub Format") and build converters for lossless round-trip conversion between all 4 supported CLIs. Specifically:

- `Claude JSONL -> Hub -> Claude JSONL` must be semantically identical
- `Codex JSONL -> Hub -> Codex JSONL` must be semantically identical
- `Gemini JSON -> Hub -> Gemini JSON` must be semantically identical
- `OpenCode SQLite -> Hub -> OpenCode SQLite` must be semantically identical
- Cross-CLI conversion: `Claude -> Hub -> Codex` preserves all portable fields and stores CLI-specific fields as metadata

### Semantic Equality Definition

Two conversation files are **semantically equal** when:
1. Every JSON key-value pair in the original exists in the result with the same value
2. Array ordering is preserved
3. Null vs missing key is treated as equivalent
4. JSON key ordering and whitespace may differ
5. Floating-point numbers match to 6 decimal places
6. For SQLite: row content matches per-column, ignoring rowid/autoincrement and WAL state

Byte-level equality is NOT required. Tests use structured JSON comparison (parsed, not string-diff).

## Design Principles

1. **Lossless round-trip**: Converting to Hub and back must not lose any data
2. **Superset schema**: Hub must represent every field from every CLI
3. **CLI-specific preservation**: Fields unique to one CLI are stored in an `extensions` object keyed by CLI name
4. **Universal core**: Common fields (role, content, timestamps, tool calls) are normalized into a shared schema
5. **JSONL on disk**: One record per line, streaming-friendly, same as Claude Code and Codex
6. **Human-readable**: JSON, not binary. No compression. Easy to grep and debug
7. **Versioned**: Schema version in every file header for forward compatibility

## Hub Format Schema

### File Format

JSONL (JSON Lines). One JSON object per line. First line is always a session header.

File extension: `.ucf.jsonl` (Unleash Conversation Format)

### Session Header (first line)

```json
{
  "ucf_version": "1.0.0",
  "type": "session",
  "session_id": "uuid-string",
  "created_at": "2026-03-29T14:00:00.000Z",
  "updated_at": "2026-03-29T15:30:00.000Z",
  "source_cli": "claude-code",
  "source_version": "2.1.87",
  "project": {
    "directory": "/home/user/project",
    "root": null,
    "hash": null,
    "vcs": "git",
    "branch": "main",
    "sha": null,
    "origin_url": null
  },
  "model": "claude-opus-4-6",
  "title": "Session title",
  "slug": null,
  "parent_session_id": null,
  "extensions": {}
}
```

Session extensions by CLI:

```json
{
  "extensions": {
    "claude-code": {
      "sessionId": "c18ef90c-...",
      "permissionMode": "bypassPermissions",
      "slug": "zazzy-percolating-eagle"
    },
    "codex": {
      "rollout_path": "sessions/2026/03/29/rollout-...",
      "source": "cli",
      "model_provider": "openai",
      "sandbox_policy": "danger-full-access",
      "approval_mode": "on-request",
      "reasoning_effort": "medium",
      "agent_nickname": null,
      "agent_role": null,
      "memory_mode": null
    },
    "gemini": {
      "projectHash": "sha256-hash",
      "installationId": "uuid"
    },
    "opencode": {
      "slug": "hidden-wolf",
      "parent_id": null,
      "version": "1.3.5",
      "permission": null,
      "workspace_id": null,
      "summary": {
        "additions": 0,
        "deletions": 0,
        "files": 0
      }
    }
  }
}
```

### Message Records

Each subsequent line is a message:

```json
{
  "type": "message",
  "id": "uuid-string",
  "api_message_id": "msg_01ABC...",
  "parent_id": "uuid-string-or-null",
  "timestamp": "2026-03-29T14:05:00.000Z",
  "completed_at": "2026-03-29T14:05:03.500Z",
  "role": "user|assistant|system",
  "content": [],
  "metadata": {},
  "extensions": {}
}
```

Key fields addressing QA findings:
- `api_message_id`: Anthropic API message ID (`msg_01ABC...`), null for other CLIs
- `completed_at`: When the message finished generating (OpenCode's `time.completed`), null if same as timestamp

### Content Blocks

The `content` array contains typed blocks. This is the universal representation of all content types across all CLIs.

#### Text

```json
{
  "type": "text",
  "text": "Hello, can you help me?"
}
```

#### Tool Use (invocation)

```json
{
  "type": "tool_use",
  "id": "tool-call-id",
  "name": "bash",
  "display_name": null,
  "description": null,
  "input": {
    "command": "ls -la",
    "description": "List directory"
  }
}
```

Fields `display_name` and `description` come from Gemini's `toolCalls[].displayName` and `toolCalls[].description`.

#### Tool Result

Output is an **array of content blocks**, not a string. This supports Claude's compound tool results (text + image arrays).

```json
{
  "type": "tool_result",
  "tool_use_id": "tool-call-id",
  "content": [
    {"type": "text", "text": "total 42\ndrwxr-xr-x ..."},
    {"type": "image", "media_type": "image/png", "encoding": "base64", "data": "..."}
  ],
  "exit_code": 0,
  "is_error": false,
  "interrupted": false,
  "status": "completed",
  "duration_ms": 150,
  "title": null,
  "truncated": false
}
```

- `content`: Array of content blocks (text, image). For simple string output, a single text block.
- `status`: "completed", "error", "timeout", "cancelled" (covers Gemini's extended status enum)
- `title`: From OpenCode's `state.title`
- `truncated`: From OpenCode's `state.metadata.truncated`

#### Thinking/Reasoning

```json
{
  "type": "thinking",
  "text": "Let me analyze this...",
  "subject": null,
  "description": null,
  "signature": "base64-signature-or-null",
  "encrypted": false,
  "encryption_format": null,
  "encrypted_data": null,
  "timestamp": null
}
```

- `subject` + `description`: From Gemini's `thoughts[].subject` and `thoughts[].description`
- `signature`: From Claude's signed thinking blocks
- `encrypted` + `encryption_format` + `encrypted_data`: From OpenCode's encrypted reasoning
- `timestamp`: From Gemini's per-thought timestamps

#### Image

```json
{
  "type": "image",
  "media_type": "image/png",
  "encoding": "base64",
  "data": "base64-data...",
  "source_url": null
}
```

#### Step Boundary

Captures OpenCode's `step-start` and `step-finish` part types:

```json
{
  "type": "step_boundary",
  "boundary": "start|finish",
  "snapshot": "git-hash-or-null",
  "finish_reason": null,
  "cost": null,
  "tokens": null
}
```

#### Patch

Captures OpenCode's file modification tracking. One patch block per file (matching OpenCode's 1:1 cardinality):

```json
{
  "type": "patch",
  "path": "/path/to/modified/file.rs",
  "hash_before": "aaa111...",
  "hash_after": "bbb222..."
}
```

### Metadata Object

Normalized metadata present on most messages:

```json
{
  "metadata": {
    "model": "claude-opus-4-6",
    "tokens": {
      "input": 1500,
      "output": 300,
      "cache_creation": 500,
      "cache_read": 200,
      "reasoning": 0,
      "tool": 0,
      "total": 2500
    },
    "tokens_cumulative": false,
    "cost": null,
    "stop_reason": "end_turn",
    "cwd": "/home/user/project",
    "root": null,
    "git_branch": "main",
    "mode": null,
    "agent": null
  }
}
```

Token tracking changes (addressing QA critical #2):
- `cache_creation` and `cache_read` are **separate fields** (not collapsed). Maps directly to Claude's `cache_creation_input_tokens` and `cache_read_input_tokens`.
- `tool`: From Gemini's `tokens.tool`
- `tokens_cumulative`: **Boolean flag**. `true` for Codex (tokens are running session totals), `false` for all others (per-message). Converters must handle delta calculation when converting cumulative to per-message.
- `cost`: From OpenCode's per-message cost tracking
- `root`: From OpenCode's `path.root`
- `mode` + `agent`: From OpenCode's `message.mode` and `message.agent`

### Extensions Object

CLI-specific fields preserved for lossless round-trip. **Every CLI has documented extensions.**

#### Claude Code Extensions

```json
{
  "extensions": {
    "claude-code": {
      "isSidechain": false,
      "promptId": "da1529c2-...",
      "userType": "external",
      "version": "2.1.87",
      "requestId": "req_011CZ...",
      "toolUseResult": {
        "name": "Bash",
        "success": true,
        "stdout": "",
        "stderr": ""
      },
      "sourceToolAssistantUUID": null,
      "usage": {
        "service_tier": "standard",
        "inference_geo": "not_available",
        "speed": "standard"
      }
    }
  }
}
```

#### Codex Extensions

```json
{
  "extensions": {
    "codex": {
      "turn_context": {
        "turn_id": "uuid",
        "approval_policy": "on-request",
        "sandbox_policy": "danger-full-access",
        "personality": null,
        "collaboration_mode": "default",
        "effort": null,
        "realtime_active": false,
        "user_instructions": null,
        "truncation_policy": null
      },
      "item_id": "item-id-string",
      "stream": false
    }
  }
}
```

#### Gemini Extensions

```json
{
  "extensions": {
    "gemini": {
      "projectHash": "sha256-hash",
      "renderOutputAsMarkdown": true
    }
  }
}
```

#### OpenCode Extensions

```json
{
  "extensions": {
    "opencode": {
      "part_id": "prt_base36...",
      "reasoning_metadata": {
        "openrouter": {
          "reasoning_details": []
        }
      }
    }
  }
}
```

### Event Records

Non-message events (metadata, lifecycle, hooks):

```json
{
  "type": "event",
  "event_type": "file_snapshot|pr_link|title_change|hook_progress|session_start|session_end|turn_context|task_started|task_complete|agent_name|agent_color|queue_operation",
  "timestamp": "2026-03-29T14:05:00.000Z",
  "data": {},
  "extensions": {}
}
```

Expanded `event_type` enum to include Codex's `turn_context`, `task_started`, `task_complete` and Claude's `agent_name`, `agent_color`, `queue_operation`.

## Converter Architecture

### Hub-and-Spoke Model

```
Claude JSONL  <-->  Hub (.ucf.jsonl)  <-->  Codex JSONL
                         |
Gemini JSON   <---------|---------->  OpenCode SQLite
```

Each CLI gets one converter module with two functions:
- `to_hub(cli_data) -> Hub JSONL`
- `from_hub(hub_data) -> CLI format`

Total: 4 converters (8 functions). O(N) complexity, not O(N^2).

### Codex Token Delta Handling

Codex stores cumulative session token totals, not per-message deltas. The converter must:
- `to_hub`: Calculate deltas by subtracting previous cumulative total. Set `tokens_cumulative: false` on Hub records.
- `from_hub`: Reconstruct cumulative totals by running sum. Set running totals on Codex `token_count` events.

This is tested explicitly with fixtures containing multiple turns.

### Round-Trip Guarantee

For each CLI, the following must hold:

```
semantic_equal(original_data, from_hub(to_hub(original_data)))
```

See "Semantic Equality Definition" above for the precise definition.

### Cross-CLI Conversion

When converting between different CLIs (e.g., Claude -> Codex):

1. All **portable fields** map directly (role, content, tool calls, timestamps, tokens)
2. **Source CLI extensions** are preserved in the Hub but not written to the target format
3. **Target CLI extensions** are populated with sensible defaults where required
4. A `_conversion` extension is added noting source CLI, conversion timestamp, and any fields that were approximated

```json
{
  "extensions": {
    "_conversion": {
      "source_cli": "claude-code",
      "source_version": "2.1.87",
      "converted_at": "2026-03-29T16:00:00.000Z",
      "approximated_fields": ["isSidechain (dropped)", "promptId (dropped)"]
    }
  }
}
```

## Implementation

### Language: Rust

The converters are implemented in Rust as part of the Unleash binary. This gives us:
- Type safety for the Hub schema (serde)
- Fast conversion (streaming JSONL processing)
- Integration with existing Unleash infrastructure
- Single binary distribution

### Module Structure

```
src/
├── interchange/
│   ├── mod.rs              # Hub format types and traits
│   ├── hub.rs              # Hub schema (serde structs)
│   ├── semantic_eq.rs      # Semantic equality comparison
│   ├── claude.rs           # Claude Code <-> Hub converter
│   ├── codex.rs            # Codex <-> Hub converter
│   ├── gemini.rs           # Gemini CLI <-> Hub converter
│   ├── opencode.rs         # OpenCode <-> Hub converter
│   └── tests/
│       ├── fixtures/       # Real conversation samples from each CLI
│       ├── round_trip.rs   # Round-trip lossless tests
│       ├── cross_cli.rs    # Cross-CLI conversion tests
│       ├── edge_cases.rs   # Interrupted sessions, multimodal, etc.
│       └── negative.rs     # Missing extensions, unknown types, malformed input
```

### CLI Command

```bash
# Convert Claude session to Hub format
unleash convert --from claude ~/.claude/projects/-home-me/session.jsonl -o session.ucf.jsonl

# Convert Hub to Codex format
unleash convert --from hub session.ucf.jsonl --to codex -o ~/.codex/sessions/2026/03/29/rollout.jsonl

# Direct cross-CLI (shorthand for from->hub->to)
unleash convert --from claude session.jsonl --to codex -o rollout.jsonl

# Verify round-trip lossless
unleash convert --verify session.jsonl --format claude
```

### Test Strategy

**Comparison framework:** All tests use `semantic_eq.rs` which parses JSON, compares structurally (ignoring key order and whitespace), handles null-vs-missing equivalence, and produces detailed diffs on failure showing exactly which field mismatched.

**Round-trip tests (critical):**
- For each CLI, take a real conversation file (fixture)
- Convert to Hub, convert back
- Assert semantic equality using the structured comparison framework
- SQLite comparison: column-level equality per row, ignoring rowid/autoincrement and WAL state
- Test with: empty sessions, single message, large sessions, multimodal, tool-heavy, thinking-heavy

**Field-specific round-trip tests:**
- Token cache split: `cache_creation` + `cache_read` survive round-trip (Claude)
- Compound tool results: text + image array in tool_result survives round-trip (Claude)
- Dual timestamps: `timestamp` + `completed_at` survive round-trip (OpenCode)
- API message ID: `api_message_id` survives round-trip (Claude)
- Step boundaries: `step_boundary` start/finish pairs survive round-trip (OpenCode)
- Cumulative tokens: Codex cumulative totals -> Hub deltas -> Codex cumulative totals
- Encrypted reasoning: encrypted_data preserved through round-trip (OpenCode)
- Signed thinking: signature preserved through round-trip (Claude)

**Cross-CLI tests:**
- Convert Claude -> Hub -> Codex, verify all portable fields present
- Convert Codex -> Hub -> Claude, verify portable fields + sensible defaults
- Test all 12 cross-CLI pairs (4 x 3)

**Edge case tests:**
- Interrupted/truncated sessions
- Sessions with images (including compound tool results with images)
- Sessions with encrypted reasoning (OpenCode)
- Sessions with sidechains (Claude)
- Sessions with 0 messages (empty)
- Very large sessions (100K+ lines)
- Malformed/partial JSONL lines

**Negative tests:**
- Missing extensions object (should not crash, use defaults)
- Unknown content block types (preserve as-is or warn)
- Unknown event types (preserve as-is or warn)
- Corrupt JSON lines (skip with warning, don't abort)
- Future ucf_version (refuse major version mismatch)

**Property-based / fuzz tests (recommended):**
- Use `proptest` crate to generate random Hub records and verify round-trip invariant
- Targets: unusual Unicode, extreme nesting, empty arrays, null-heavy records, very long strings
- Catches edge cases that hand-written fixtures miss

**Fixture collection:**
- Extract representative samples from each CLI's actual conversation files
- Sanitize (remove API keys, tokens, personal data)
- Include at least: simple chat, tool-heavy session, thinking-heavy session, multimodal session, interrupted session
- Store in `src/interchange/tests/fixtures/`

## File Naming Convention

Hub files use `.ucf.jsonl` extension:
- `session-2026-03-29-c18ef90c.ucf.jsonl`
- Pattern: `session-<date>-<uuid8>.ucf.jsonl`

## Schema Versioning

The `ucf_version` field in the session header uses semver:
- **Patch**: new optional fields, backward compatible
- **Minor**: new content block types, backward compatible
- **Major**: breaking changes to existing fields

Converters must check version and refuse to process unsupported major versions.

## Implementation Order

1. Define Hub schema as Rust structs with serde (`src/interchange/hub.rs`)
2. Implement semantic equality framework (`src/interchange/semantic_eq.rs`)
3. Implement Claude Code converter (most complex, most fields)
4. Write round-trip tests for Claude Code with real fixtures
5. Implement Codex converter (including cumulative token delta handling)
6. Write round-trip tests for Codex
7. Implement Gemini CLI converter
8. Write round-trip tests for Gemini
9. Implement OpenCode converter (SQLite handling)
10. Write round-trip tests for OpenCode
11. Write cross-CLI conversion tests (all 12 pairs)
12. Add `unleash convert` CLI command
13. Negative and edge case tests
14. Documentation

## Success Criteria

1. All 4 round-trip tests pass with real conversation fixtures (semantic equality)
2. All field-specific round-trip tests pass (cache split, compound tool results, dual timestamps, etc.)
3. All 12 cross-CLI conversion tests pass (portable fields preserved)
4. All negative tests pass (graceful handling of bad input)
5. `unleash convert --verify` command confirms lossless round-trip
6. Hub format is documented and versioned
7. Test coverage > 90% for converter modules
