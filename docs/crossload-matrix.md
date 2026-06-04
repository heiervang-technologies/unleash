# Cross-CLI Session Crossload Matrix

Status of conversation history loading between all supported agent CLIs.

**Last updated:** 2026-06-05

> Coverage below is for the four CLIs with end-to-end synthetic-fixture
> tests (`src/interchange/tests/fixtures/synthetic/*-10turn.*`). For the
> other three see [Additional CLIs](#additional-clis) below.

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
| Claude → Gemini | :green_circle: Lossless | Full history, survives --list-sessions |
| Gemini → Claude | :green_circle: Lossless | 82 messages, chain intact |
| Codex → Claude | :green_circle: Lossless | Verified end-to-end, events filtered, tool calls preserved |
| Claude → Codex | :green_circle: Lossless | Verified end-to-end with `codex resume`, state DB registered |
| OpenCode → Claude | :green_circle: Lossless | Thinking blocks converted to text, full history loads |
| Codex → Gemini | :green_circle: Lossless | Via hub, verified |
| Gemini → Codex | :green_circle: Lossless | Via hub, verified |
| Claude → OpenCode | :yellow_circle: Partial | SQLite injection works, session not visible in OpenCode UI — investigating `-s` session loading |
| Codex → OpenCode | :yellow_circle: Partial | SQLite injection implemented, same OpenCode display issue |
| Gemini → OpenCode | :yellow_circle: Partial | SQLite injection implemented, same OpenCode display issue |
| OpenCode → Gemini | :yellow_circle: Partial | Implemented, needs verification |
| OpenCode → Codex | :yellow_circle: Partial | Implemented, needs verification |

**Legend:** :green_circle: Lossless (verified end-to-end) · :yellow_circle: Partial (works with known limitations) · :red_circle: Not working · :white_circle: Untested

## Additional CLIs

| CLI | Source discovery | Target injection | Round-trip coverage |
|---|---|---|---|
| **Pi** | ✓ `sessions.rs:776` | ✓ `inject_into_pi` | Bidirectional **portable-fields** unit tests in `cross_cli_tests.rs` against claude/codex/gemini/opencode. No 10-turn synthetic fixture yet → not in main matrix. |
| **Hermes** | ✓ `sessions.rs:640` | ✓ `inject_into_hermes` | Untested — dispatch wired in, no round-trip tests yet. |
| **Antigravity (`agy`)** | via Gemini storage layout (`sessions.rs` Gemini path; see `normalize_target_cli` in `inject.rs:130`) | via Gemini path | Inherits Gemini's matrix entries. |

## How It Works

1. **Session discovery** (`unleash sessions`) scans the configured CLI session stores (claude/codex/gemini/opencode/pi/hermes; agy shares the Gemini path)
2. **Hub conversion** transforms the source format to the Unleash Conversation Format (.ucf.jsonl)
3. **Target injection** converts from hub to the target CLI format and writes to its session directory
4. **Resume** launches the target CLI with `--resume <session-id>`

### Architecture

```
Claude JSONL  <-->  Hub (.ucf.jsonl)  <-->  Codex JSONL
                         |
                         |---->  Gemini JSON  (agy shares this path)
                         |---->  OpenCode SQLite
                         |---->  Pi JSON
                         |---->  Hermes JSON
```

Hub-and-spoke model: O(N) converters, not O(N²) direct pairs.

## Known Limitations

- **Claude injection**: non-message events (progress, hooks, token counts) are filtered out. Only user/assistant messages with real content are injected. System preamble (`<environment_context>`, `<permissions>`) is stripped.
- **Gemini injection**: requires valid `startTime`, `lastUpdated`, `sessionId`, and at least one user/gemini message. Sessions with only system messages are deleted by Gemini CLI.
- **OpenCode injection**: writes directly to SQLite database. Session, message, and part rows are created with proper ID chains. Project is auto-created if not found.
- **Thinking blocks**: Claude requires signed thinking blocks. Foreign thinking/reasoning blocks are converted to `[Reasoning]: ...` text blocks.
- **Tool calls**: preserved structurally where formats align. Codex function_call maps to tool_use, function_call_output maps to tool_result.

## Planned Improvements

### Index Sync (#303)

On crossload, automatically update the target CLI's session index:
- **Claude**: append to `~/.claude/history.jsonl`
- **Codex**: append to `~/.codex/session_index.jsonl`
- **Gemini**: write `logs.json` entries (already implemented)
- **OpenCode**: INSERT into SQLite (implemented)

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
