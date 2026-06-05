---
name: restart
description: Restart Claude Code while preserving your session
allowed-tools: Bash(unleash-refresh)
---

# Restart Claude Code

To restart, simply run the unleash-refresh command:

```bash
unleash-refresh
```

Or with a custom message to receive after restart:

```bash
unleash-refresh "Continue working on the feature"
```

## Requirements

You must be running under the `unleash` wrapper (check: `echo $AGENT_UNLEASH` should return `1`).

## What Happens

1. The unleash-refresh script creates a trigger file
2. Claude process is terminated
3. The wrapper detects the trigger file
4. Wrapper restarts Claude with `--continue` flag
5. You receive "RESURRECTED." (or your custom message)

## Your Task

Run the unleash-refresh command now:

```bash
unleash-refresh
```
