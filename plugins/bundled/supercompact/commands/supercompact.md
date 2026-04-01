---
description: EITF entity-preservation compaction (~400x faster than /compact, 2x better entity retention)
argument-hint: "[budget] [--method eitf|setcover|dedup]"
allowed-tools: Bash(*/compact-session.sh*), Bash(restart-claude*)
---

# Supercompact — Entity-Preservation Compaction

Run the compaction script. It will find the session JSONL automatically, compact it, and report results.

```bash
SCRIPT="${CLAUDE_PLUGIN_ROOT:-${HOME}/.local/share/supercompact/claude-code/plugin}/scripts/compact-session.sh"
"$SCRIPT" $ARGUMENTS
```

If the script succeeds and reports compaction was performed (not "already within budget"), restart to load the compacted context:

```bash
restart-claude "Session compacted with supercompact. Restarting to load compacted context."
```

If `restart-claude` is not available, tell the user: "Run `/quit` then `claude --resume` to load the compacted context."

If the script reports "already within budget", tell the user and do NOT restart.

If the script fails, show the error output to the user and do not restart.
