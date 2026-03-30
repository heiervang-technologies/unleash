# Internal CLI Knowledge Base Design

**Date:** 2026-03-29
**Status:** Draft
**Depends on:** Research from 5 agents exploring Claude Code, Codex, Gemini CLI, OpenCode formats + existing standards

## Problem

Unleash manages 4 agent CLIs but has no internal documentation about how they work. Developers adding features (session management, format conversion, polyfills) must reverse-engineer each CLI's storage format, session handling, and tool calling conventions from scratch. This slows development and leads to incorrect assumptions.

## Goal

Create a developer-facing internal knowledge base documenting the internals of all 4 supported CLIs. This serves as the foundation for the chat log interchange format (separate spec).

## Audience

Unleash developers and contributors. Not end users.

## File Structure

```
docs/internal/
├── README.md                    # What's here, why, how to maintain
└── cli-formats/
    ├── overview.md              # Feature comparison matrix + format summary
    ├── claude-code.md           # Claude Code storage internals
    ├── codex.md                 # Codex CLI storage internals
    ├── gemini-cli.md            # Gemini CLI storage internals
    └── opencode.md              # OpenCode storage internals
```

## Content Per CLI Document

Each document follows the same structure:

### 1. Storage Location
- Paths on disk (config, sessions, history, databases)
- Directory hierarchy and naming conventions

### 2. File Format
- Primary format (JSONL, JSON, SQLite, hybrid)
- File naming patterns (UUIDs, timestamps, hashes)

### 3. Message Schema
- Top-level record structure with annotated JSON examples
- All fields documented with types and descriptions

### 4. Message Types
- User messages
- Assistant messages
- System/info messages
- Tool call and tool result messages
- Thinking/reasoning messages
- Metadata messages (titles, names, PR links, etc.)

### 5. Tool Call Format
- How tool invocations are represented
- How tool results (stdout, stderr, exit codes) are stored
- Tool name conventions

### 6. Token Tracking
- Token usage fields (input, output, cached, reasoning)
- Where token data lives (per-message, per-turn, cumulative)

### 7. Session Management
- Session identification (UUID format, naming)
- Session resumption mechanism
- Session metadata (timestamps, git info, model, cwd)
- Session lifecycle (creation, update, archival)

### 8. Project Organization
- How projects/workspaces are scoped
- Project identification (path encoding, hashing)

### 9. Configuration and Retention
- Relevant config files and their impact on storage
- Retention policies (Codex: max_bytes compaction, Gemini: 30-day cleanup)
- History limits and automatic cleanup behavior
- What data may be missing due to retention (important for interchange)

### 10. Multimodal Content
- Image handling (base64 inline, file references, content blocks)
- PDF and other media types
- Encoding format and size limits
- How non-text content appears in the message schema

### 11. Error and Interrupted States
- How crashed/interrupted sessions are represented on disk
- Partial tool results (truncated output, timeouts)
- Rate limit pauses and retry markers
- Whether session files can be incomplete/truncated
- Flags indicating interruption (e.g., Claude's `interrupted` field in toolUseResult)

## overview.md — Feature Comparison Matrix

The overview document contains a cross-CLI comparison table:

| Feature | Claude Code | Codex | Gemini CLI | OpenCode |
|---------|-------------|-------|------------|----------|
| **Primary format** | JSONL | JSONL + SQLite | JSON | SQLite + JSON |
| **Storage path** | ~/.claude/projects/ | ~/.codex/sessions/ | ~/.gemini/tmp/ | ~/.local/share/opencode/ |
| **Session ID format** | UUID | UUID | UUID | `ses_` + base36 |
| **Message ID format** | UUID | N/A (event stream) | UUID | `msg_` + base36 |
| **Thinking/reasoning** | thinking blocks (signed) | reasoning_output_tokens | thoughts array | reasoning part type |
| **Tool calls** | content block (tool_use) | response_item event | toolCalls array | tool part type |
| **Tool results** | content block (tool_result) | event_msg | nested in toolCalls | tool part (state.output) |
| **Token tracking** | usage object per response | token_count events | tokens per message | tokens in message data |
| **Cache tracking** | cache_creation + cache_read | cached_input_tokens | cached field | cache.read + cache.write |
| **Git metadata** | gitBranch per message | git_sha, git_branch, git_origin_url | No | No |
| **File history** | file-history-snapshot type | No | No | patch part type |
| **Sidechain/branching** | isSidechain flag | No | No | parentID |
| **Project scoping** | path-encoded directory | date hierarchy | project hash (SHA256) | project hash (SHA1) |
| **Retention config** | No auto-cleanup | max_bytes compaction | 30-day default | No auto-cleanup |
| **Export support** | No native export | No native export | /chat share file.json | No native export |
| **History index** | history.jsonl | history.jsonl + session_index.jsonl | logs.json | SQLite queries |
| **Hook integration** | progress events (SessionStart, PreToolUse, etc.) | No | No | No |
| **Image support** | image content blocks (base64) | input_image content type | inlineData | TBD |
| **Interruption markers** | toolUseResult.interrupted | task_complete events | No | step-finish reason |
| **Cost tracking** | No | No | No | cost field per message |

### Portable vs CLI-Specific Features

Features that exist across all CLIs (portable):
- User/assistant message roles
- Text content
- Tool calls with name + arguments
- Tool results with output
- Timestamps
- Session identification
- Token usage (input + output)
- Model identification

Features specific to one or two CLIs (may be lost or approximated in conversion):
- Thinking/reasoning blocks (Claude signed, Gemini plaintext, OpenCode encrypted)
- File history snapshots (Claude only)
- Sidechain conversations (Claude only)
- Hook progress events (Claude only)
- Queue operations (Claude only)
- PR links (Claude only)
- Extended turn context (Codex: approval policy, sandbox policy, personality)
- Git metadata detail (Codex: SHA + branch + origin; Claude: branch only)
- Cost tracking (OpenCode only)

## docs/internal/README.md Content

```markdown
# Internal Developer Documentation

Reference documentation for Unleash developers. Not user-facing.

## Contents

### CLI Formats (`cli-formats/`)

How each supported agent CLI stores conversations, sessions, and metadata.
Used as the foundation for the chat log interchange format.

- [Overview & Comparison](cli-formats/overview.md)
- [Claude Code](cli-formats/claude-code.md)
- [Codex](cli-formats/codex.md)
- [Gemini CLI](cli-formats/gemini-cli.md)
- [OpenCode](cli-formats/opencode.md)

## Maintenance

These docs should be updated when:
- A CLI updates its storage format (check after major version bumps)
- A new CLI is added to Unleash
- Research reveals new details about a CLI's internals

Each document includes a "Last verified" date and CLI version.
```

## Implementation

This is documentation only — no code changes. Each document is written from the research findings, verified against actual files on disk where possible, and tagged with the CLI version it was verified against.

### Implementation Order

1. Create `docs/internal/README.md`
2. Create `docs/internal/cli-formats/overview.md` with comparison matrix
3. Create `docs/internal/cli-formats/claude-code.md`
4. Create `docs/internal/cli-formats/codex.md`
5. Create `docs/internal/cli-formats/gemini-cli.md`
6. Create `docs/internal/cli-formats/opencode.md`
7. Update `docs/DOCUMENTATION_MAP.md` with internal docs section
8. Commit and PR

### Source Material

Research findings from 5 agents (stored in this conversation):
- Claude Code: JSONL format, 12+ message types, rich metadata
- Codex: JSONL + SQLite hybrid, event-stream model, thread management
- Gemini CLI: JSON sessions + logs index, thoughts, project hashing
- OpenCode: SQLite + Drizzle ORM, message/part separation, 6 part types
- Standards survey: No existing interchange standard; cass is the closest tool
