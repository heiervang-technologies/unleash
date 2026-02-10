# Supercompact Plugin

EITF entity-preservation conversation compaction for Claude Code. Replaces the default `/compact` with a method that is ~400x faster and preserves 2x more entities.

## How It Works

The plugin uses **Entity-frequency Inverse Turn Frequency (EITF)** scoring to determine which conversation turns contain the most irreplaceable information. Turns with rare, high-weight entities (file paths, function names, error messages, URLs) score highest and are preserved during compaction.

### Algorithm

1. **Parse** the JSONL conversation into user/system turns
2. **Extract entities** (paths, functions, classes, URLs, errors, etc.) from all turns
3. **Score** each system turn by weighted entity importance x rarity (ITF)
4. **Select** turns greedily by adjusted score until the token budget is filled
5. **Write** the compacted JSONL back to the session file

## Components

### PreCompact Hook

Fires when Claude Code is about to compact the conversation. The hook **cannot block or replace** the built-in compaction (it's notification-only), so it:

1. Backs up the full transcript (`.pre-compact-full`) before Claude's summarization loses detail
2. Runs EITF compaction to produce a superior alternative (`.supercompact`)
3. Claude's built-in compaction still runs on the in-memory conversation
4. To use the EITF version instead: `cp transcript.jsonl.supercompact transcript.jsonl` then resume

### `/supercompact` Command

Manual slash command for on-demand compaction with optional custom budget:

```
/supercompact          # Default 80,000 token budget
/supercompact 120000   # Custom budget
```

## Configuration

Environment variables (set via plugin settings):

| Variable | Default | Description |
|----------|---------|-------------|
| `PLUGIN_SETTING_BUDGET` | `80000` | Target token budget |
| `PLUGIN_SETTING_METHOD` | `eitf` | Scoring method |

## Logs

Compaction logs are written to `~/.cache/agent-unleashed/supercompact/hook.log`.

## Dependencies

- [supercompact](https://github.com/heiervang-technologies/supercompact) at `/home/me/ht/supercompact`
- `uv` for Python dependency management
