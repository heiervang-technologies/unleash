---
name: supercompact-budget
description: Set default supercompact token budget, floor, or ceiling
---

# Supercompact Budget

Configure the token budget supercompact uses for future `/compact` runs. Overrides persist in `~/.config/unleash/plugins/supercompact/settings.env` and apply on every subsequent compaction.

## Usage

- `/supercompact-budget` — show current overrides
- `/supercompact-budget <N>` — fixed budget of N tokens (switches to manual mode)
- `/supercompact-budget auto` — return to auto (percentage-of-current) mode
- `/supercompact-budget floor <N>` — set minimum retained tokens
- `/supercompact-budget ceiling <N>` — set maximum retained tokens
- `/supercompact-budget reset` — clear all overrides

## Action

Run the configuration script with the provided arguments and report its output verbatim:

```bash
"${CLAUDE_PLUGIN_ROOT}/scripts/set-budget.sh" $ARGUMENTS
```
