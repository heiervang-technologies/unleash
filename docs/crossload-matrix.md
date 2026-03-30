# Cross-CLI Session Crossload Matrix

Status of conversation history loading between all supported agent CLIs.

**Last updated:** 2026-03-30

## Usage

```bash
# List all sessions across all CLIs
unleash sessions

# Interactive picker — browse and select any session
unleash claude -x

# Direct crossload by name
unleash claude -x codex:rust-eng
unleash gemini -x claude:rice-chief

# Convert formats offline
unleash convert --from codex session.jsonl --to claude -o output.jsonl
```

## Matrix

| Source → Target | Status | Notes |
|----------------|--------|-------|
| Codex → Claude (synthetic) | :green_circle: Lossless | All 10 msgs verified in tmux |
| Codex → Claude (real) | :yellow_circle: Partial | Events filtered, tool calls as text |
| Claude → Gemini | :green_circle: Lossless | Full history, survives --list-sessions |
| Gemini → Claude | :green_circle: Lossless | 82 messages, chain intact |
| Claude → Codex | :green_circle: Lossless | Verified end-to-end with `codex resume`, state DB registered |
| OpenCode → Claude | :yellow_circle: Partial | Thinking blocks converted to text |
| Codex → Gemini | :white_circle: Untested | |
| Gemini → Codex | :white_circle: Untested | |
| OpenCode → Gemini | :white_circle: Untested | |
| Codex → OpenCode | :white_circle: Untested | SQLite injection pending |
| Claude → OpenCode | :white_circle: Untested | SQLite injection pending |
| Gemini → OpenCode | :white_circle: Untested | SQLite injection pending |

**Legend:** :green_circle: Lossless (verified end-to-end) · :yellow_circle: Partial (works with known limitations) · :red_circle: Not working · :white_circle: Untested

## How It Works

1. **Session discovery** (`unleash sessions`) scans all 4 CLI session stores
2. **Hub conversion** transforms the source format to the Unleash Conversation Format (.ucf.jsonl)
3. **Target injection** converts from hub to the target CLI format and writes to its session directory
4. **Resume** launches the target CLI with `--resume <session-id>`

### Architecture

```
Claude JSONL  <-->  Hub (.ucf.jsonl)  <-->  Codex JSONL
                         |
Gemini JSON   <---------|---------->  OpenCode SQLite
```

Hub-and-spoke model: O(N) converters, not O(N^2) direct pairs.

## Known Limitations

- **Claude injection**: non-message events (progress, hooks, token counts) are filtered out. Only user/assistant messages with real content are injected. System preamble (`<environment_context>`, `<permissions>`) is stripped.
- **Gemini injection**: requires valid `startTime`, `lastUpdated`, `sessionId`, and at least one user/gemini message. Sessions with only system messages are deleted by Gemini CLI.
- **OpenCode injection**: SQLite writes not yet implemented. Currently exports to hub format only.
- **Thinking blocks**: Claude requires signed thinking blocks. Foreign thinking/reasoning blocks are converted to `[Reasoning]: ...` text blocks.
- **Tool calls**: preserved structurally where formats align. Codex function_call maps to tool_use, function_call_output maps to tool_result.

## Planned Improvements

### Index Sync (#303)

On crossload, automatically update the target CLI's session index:
- **Claude**: append to `~/.claude/history.jsonl`
- **Codex**: append to `~/.codex/session_index.jsonl`
- **Gemini**: write `logs.json` entries (already implemented)
- **OpenCode**: INSERT into SQLite (pending)

### Session List UX (#303)

`unleash sessions` should display a clear tabular view:

```
CLI        NAME              CLUE                              DIRECTORY                    TIMESTAMP
claude     unleash-dev       fix-crossload-target-detect...    /home/me/ht/unleash          2026-03-29 19:25:02
codex      rust-eng          hey-rust-eng-welcome-to-the...    /home/me/ht/unleash          2026-03-29 18:17:14
gemini     fd431d16          are-you-familiar-with-omarchy     /home/me                     2026-02-28 05:41:00
opencode   hidden-wolf       greeting                          /home/me/ht/unleash          2026-03-14 00:00:00
```

Columns:
- **CLI**: agent CLI name
- **NAME**: session slug, thread_name, agent-name, or truncated UUID
- **CLUE**: extractive kebab-case summary of first/last user message (not too long)
- **DIRECTORY**: project working directory
- **TIMESTAMP**: full precision down to second (YYYY-MM-DD HH:MM:SS)

## Tracked Issues

- [#303](https://github.com/heiervang-technologies/unleash/issues/303) — Index sync, session list UX, real session filtering, OpenCode SQLite injection

## Test Fixtures

Synthetic 10-turn conversations with known content (foo/bar, baz/marco, polo/ping, pong/hello, world/done):

```
src/interchange/tests/fixtures/synthetic/
├── claude-10turn.jsonl    # Claude Code JSONL
├── codex-10turn.jsonl     # Codex JSONL
├── gemini-10turn.json     # Gemini CLI JSON
└── opencode-10turn.json   # OpenCode JSON
```
