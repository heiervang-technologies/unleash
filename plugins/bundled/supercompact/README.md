# supercompact

Drop-in replacement for `/compact` that scores conversation turns by
entity preservation instead of summarizing. About **400× faster** than
the default summarizer and retains roughly **2× more distinct entities**
across a single compaction.

## Scoring methods

Pick one in the Plugins tab of the unleash TUI:

| Method | What it does |
|---|---|
| **eitf** *(default)* | Entity Importance Term Frequency — ranks turns by the rarity and recency of entities they contain |
| **setcover** | Greedy set-cover: keeps the smallest set of turns that still covers every entity in the transcript |
| **dedup** | Drops near-duplicate turns; lightest-weight |

## Trigger model

Two layers, both enabled by default:

1. **Preemptive (UserPromptSubmit hook)** — fires when the transcript
   crosses a token threshold (default 200k tokens, or ~2.7 MB on disk
   when no tokenizer is available). Compacts to a *preemptive* target
   (default 50% of current) so the next turn has headroom.
2. **Manual (PreCompact hook)** — when the user runs `/compact`, runs
   scoring instead of summarization and compacts to the configured
   *budget* (default 40% of current in auto mode, fixed token count in
   manual mode).

The thresholds, target percentages, and the fixed token budget are all
configurable from the Plugins tab.

## `/supercompact-budget` slash command

For per-session overrides without touching the TUI:

```
/supercompact-budget                       # show current overrides
/supercompact-budget <N>                   # set BUDGET to N tokens (manual mode)
/supercompact-budget threshold <N>         # set the trigger THRESHOLD
/supercompact-budget threshold default     # restore default threshold (180000)
/supercompact-budget floor <N>             # set BUDGET floor (auto-mode min)
/supercompact-budget ceiling <N>           # set BUDGET ceiling (auto-mode max)
/supercompact-budget reset                 # clear all overrides
```

Overrides persist at `~/.config/unleash/plugins/supercompact/settings.env`
and apply on every subsequent compaction.

> **Rule of thumb**: keep THRESHOLD ≫ BUDGET (at least +30k headroom)
> or compaction will re-trigger immediately after running.

## Files

| Path | Purpose |
|---|---|
| `hooks/hooks.json` | Wires `UserPromptSubmit` + `PreCompact` events |
| `hooks-handlers/supercompact-userprompt.sh` | Preemptive compaction trigger |
| `hooks-handlers/supercompact-precompact.sh` | Manual `/compact` handler |
| `hooks-handlers/supercompact-compact.sh` | Shared scoring + write |
| `scripts/set-budget.sh` | Backs the `/supercompact-budget` command |
| `scripts/check-enabled.sh` | Plugin-enabled gate (called by every hook) |

## Disabling

Add `supercompact` to `~/.config/unleash/config.toml`'s
`enabled_plugins` allowlist (or remove from it) — see
[docs/extensions/configuration.md](../../../docs/extensions/configuration.md).
