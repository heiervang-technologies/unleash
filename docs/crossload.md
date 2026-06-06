# Cross-CLI Session Crossload

Load conversation history from one agent CLI into another. Start a session in
Codex, continue it in Claude, hand it to Gemini, finish it in Pi — without
losing context.

## How It Works

1. **Discovery** -- `unleash sessions` scans session stores for all installed CLIs (claude/codex/gemini/opencode/pi/hermes; agy shares the Gemini path)
2. **Hub conversion** -- source format is converted to Universal Chat Format (`.ucf.jsonl`)
3. **Target injection** -- hub format is converted to the target CLI's native format
4. **Resume** -- target CLI launches with the injected session

### Hub-and-Spoke Architecture

```
Claude JSONL  <-->  Hub (.ucf.jsonl)  <-->  Codex JSONL
                         |
                         |---->  Gemini JSON  (agy shares this path)
                         |---->  OpenCode SQLite
                         |---->  Pi JSON
                         |---->  Hermes JSON
```

O(N) converters instead of O(N²) direct pairs. The hub format is JSONL for
corruption recovery, with minimal extensions (~10% overhead).

## Usage

```bash
# List all sessions across all CLIs
unleash sessions

# Interactive picker -- browse and select any session
unleash claude -x

# Crossload a specific session by name
unleash claude -x codex:rust-eng
unleash gemini -x claude:rice-chief

# Offline format conversion (--from is required)
unleash convert --from claude session.jsonl                           # hub format → stdout
unleash convert --from claude --to codex session.jsonl -o out.jsonl  # Claude → Codex
```

## Compatibility Matrix

| Source -> Target | Status | Notes |
|------------------|--------|-------|
| Codex -> Claude | Lossless | Verified end-to-end |
| Claude -> Gemini | Lossless | Full history preserved |
| Gemini -> Claude | Lossless | Chain intact |
| Claude -> Codex | Lossless | State DB registered |
| OpenCode -> Claude | Partial | Thinking blocks converted to text |
| -> OpenCode (all) | Pending | SQLite injection not yet implemented |

## Passthrough Fallback

Some target CLIs reject session-level injection. Antigravity (`agy`), for
example, validates every cascade_id against a server-fetched executor from
Google's CodeAssist API, so a locally-written conversation always fails with
`cascade ID mismatch` at send time (see issue #307).

When session-level injection refuses, unleash automatically falls back to
**passthrough mode**: the source session is rendered as a markdown transcript
and prepended as a single initial prompt. Tool calls, tool results, thinking
blocks, and images are summarised rather than reproduced verbatim, so the
result fits in one prompt without escape-sequence or signature issues.

```bash
# Crossload a Claude session into agy — falls back automatically
unleash agy -x claude:heierchat
```

For agents that expose a "load prompt then drop into REPL" flag
(currently just `agy -i` / `--prompt-interactive`), passthrough uses that
instead of the one-shot `-p` so the user keeps an interactive session.

### Tuning the fallback

| Variable | Effect |
|---|---|
| `UNLEASH_CROSSLOAD_MAX_TOKENS` | Trim the oldest messages so the rendered transcript stays under this token budget. Also applies to the inject path. Unset/0 = no limit. |
| `UNLEASH_CROSSLOAD_NO_FALLBACK` | Refuse to fall back — surface the original injection error instead. |

```bash
# Tighter budget for a very large source session (avoids ARG_MAX overflow)
UNLEASH_CROSSLOAD_MAX_TOKENS=20000 unleash agy -x claude:huge-session

# Hard-error if injection refuses (don't render as passthrough prompt)
UNLEASH_CROSSLOAD_NO_FALLBACK=1 unleash agy -x claude:huge-session
```

Manual passthrough is also available via `unleash convert --to passthrough`
— see [cli-reference.md](cli-reference.md#unleash-convert).

## Known Limitations

- **Thinking blocks**: Claude requires signed thinking blocks. Foreign
  reasoning blocks are converted to `[Reasoning]: ...` text.
- **Tool calls**: Preserved structurally where formats align. Codex
  `function_call` maps to Claude `tool_use`. Where no structural mapping
  exists, tool calls are rendered as descriptive text.
- **System preamble**: Claude-specific system events (permissions, environment
  context, progress) are stripped during extraction.
- **OpenCode**: SQLite writes not yet implemented. Currently exports to hub
  format only.
- **Passthrough lossiness**: When the fallback fires, the entire source
  conversation becomes one user-turn from the target's perspective — tool-call
  structure, model attribution, and thinking blocks are not preserved.

## Detailed Matrix

See [crossload-matrix.md](crossload-matrix.md) for per-pair notes, test
fixtures, and planned improvements.
