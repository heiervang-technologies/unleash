---
name: supercompact-budget
description: Configure supercompact threshold (when to auto-compact) and budget (compression target)
---

# Supercompact Configuration

Two distinct knobs:

- **THRESHOLD** — token count at which preemptive auto-compaction TRIGGERS.
  Should sit just below the model's context window. Default: 180000.
- **BUDGET** — target token count compaction COMPRESSES TO.
  Should be much smaller than THRESHOLD. Default: auto (percentage of current).

These MUST be different. THRESHOLD must be substantially higher than BUDGET
(at least +30000 headroom) or compaction will re-trigger immediately after running.

Overrides persist in `~/.config/unleash/plugins/supercompact/settings.env`
and apply on every subsequent compaction.

## Usage

- `/supercompact-budget` — show current overrides
- `/supercompact-budget <N>` — set BUDGET to N tokens (switches to manual mode)
- `/supercompact-budget budget <N>` — set BUDGET (alias)
- `/supercompact-budget budget auto` — switch BUDGET to auto (percentage-of-current) mode
- `/supercompact-budget threshold <N>` — set THRESHOLD (auto-compact trigger)
- `/supercompact-budget threshold default` — restore default THRESHOLD (180000)
- `/supercompact-budget floor <N>` — set BUDGET floor (auto-mode min)
- `/supercompact-budget ceiling <N>` — set BUDGET ceiling (auto-mode max)
- `/supercompact-budget reset` — clear all overrides

## Action

Run the configuration script with the provided arguments and report its output verbatim:

```bash
"${CLAUDE_PLUGIN_ROOT}/scripts/set-budget.sh" $ARGUMENTS
```
