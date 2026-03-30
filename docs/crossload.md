# Cross-CLI Session Crossload

Load conversation history from one agent CLI into another. Start a session in
Codex, continue it in Claude, hand it to Gemini -- without losing context.

## How It Works

1. **Discovery** -- `unleash sessions` scans session stores for all installed CLIs
2. **Hub conversion** -- source format is converted to Universal Chat Format (`.ucf.jsonl`)
3. **Target injection** -- hub format is converted to the target CLI's native format
4. **Resume** -- target CLI launches with the injected session

### Hub-and-Spoke Architecture

```
Claude JSONL  <-->  Hub (.ucf.jsonl)  <-->  Codex JSONL
                         |
Gemini JSON   <---------|---------->  OpenCode SQLite
```

O(N) converters instead of O(N^2) direct pairs. The hub format is JSONL for
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

# Offline format conversion
unleash convert input.jsonl output.ucf.jsonl
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

## Detailed Matrix

See [crossload-matrix.md](crossload-matrix.md) for per-pair notes, test
fixtures, and planned improvements.
