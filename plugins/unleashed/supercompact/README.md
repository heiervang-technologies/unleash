# Supercompact Plugin

Entity-preservation conversation compaction for Claude Code. Replaces the default `/compact` with a method that is ~400x faster and preserves 2x more entities.

## How It Works

The plugin scores conversation turns to determine which contain the most irreplaceable information. Turns with rare, high-weight entities (file paths, function names, error messages, URLs) score highest and are preserved during compaction.

### Available Methods

| Method | Description | Speed |
|--------|-------------|-------|
| `eitf` | Entity-frequency Inverse Turn Frequency — scores by weighted entity importance × rarity | ~0.2s |
| `setcover` | EITF + adaptive normalization + entity exclusivity bonus | ~0.2s |
| `dedup` | Suffix automaton dedup — penalizes turns with repeated content | ~0.3s |

### Algorithm

1. **Parse** the JSONL conversation into user/system turns
2. **Extract entities** (paths, functions, classes, URLs, errors, etc.) from all turns
3. **Score** each system turn using the configured method
4. **Select** turns greedily by adjusted score until the token budget is filled
5. **Write** the compacted JSONL back to the session file

## Components

### PreCompact Hook

Fires when Claude Code is about to compact the conversation. The hook **cannot block or replace** the built-in compaction (it's notification-only), so it:

1. Backs up the full transcript (`.pre-compact-full`) before Claude's summarization loses detail
2. Runs compaction (using configured method) to produce a superior alternative (`.supercompact`)
3. Claude's built-in compaction still runs on the in-memory conversation
4. To use the supercompact version instead: `cp transcript.jsonl.supercompact transcript.jsonl` then resume

### `/supercompact` Command

Manual slash command for on-demand compaction:

```
/supercompact                          # Default method and budget
/supercompact 120000                   # Custom budget
/supercompact --method setcover        # Custom method
/supercompact 120000 --method dedup    # Both
```

### cli.js Patch

The `patch-compaction.sh` script patches Claude Code's cli.js to replace the LLM summarization call with supercompact. This makes compaction automatic — no need to manually invoke `/supercompact`.

The patch reads configuration from environment variables at runtime, so changing settings takes effect on the next compaction without re-patching.

## Configuration

Environment variables (set in your shell profile, `.env`, or via the launcher):

| Variable | Default | Description |
|----------|---------|-------------|
| `PLUGIN_SETTING_METHOD` | `eitf` | Scoring method (`eitf`, `setcover`, `dedup`) |
| `PLUGIN_SETTING_BUDGET` | `80000` | Target token budget |
| `PLUGIN_SETTING_FALLBACK_TO_BUILTIN` | `true` | Fall back to Claude's built-in LLM compaction if supercompact fails |

### Setting via shell

```bash
export PLUGIN_SETTING_METHOD=setcover
export PLUGIN_SETTING_BUDGET=120000
export PLUGIN_SETTING_FALLBACK_TO_BUILTIN=true
```

### Method selection guide

- **`eitf`** (default) — Best general-purpose choice. Fast, good entity preservation.
- **`setcover`** — Slightly better at preserving exclusive entities (ones that only appear in one turn). Same speed as EITF.
- **`dedup`** — Best when conversations have lots of repeated content (e.g., retried commands, similar error messages). Uses suffix automaton.

## Logs

Compaction logs are written to `~/.cache/agent-unleashed/supercompact/hook.log`.

## Dependencies

- [supercompact](https://github.com/heiervang-technologies/supercompact) at `/home/me/ht/supercompact`
- `uv` for Python dependency management
