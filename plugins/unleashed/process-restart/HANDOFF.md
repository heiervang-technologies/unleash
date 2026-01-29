# Process Restart Plugin - Handoff Document

**Date**: 2026-01-06
**Status**: WORKING - Two successful self-restart methods discovered
**Issue**: https://github.com/heiervang-technologies/agent-unleashed/issues/7

## BREAKTHROUGH: Working Self-Restart Methods

After extensive investigation, two working self-restart methods were discovered:

1. **Wrapper method** (recommended) - No dependencies, works anywhere
2. **tmux method** - Requires tmux, but works without wrapper

Both are the **first known methods** that actually work for Claude Code self-restart.

### Method 1: Wrapper (Recommended)

```bash
#!/bin/bash
# claude-wrapper.sh - run this instead of 'claude' directly
TRIGGER="$HOME/.cache/agent-unleashed/process-restart/restart-trigger"
mkdir -p "$(dirname "$TRIGGER")"

while true; do
    rm -f "$TRIGGER"
    claude "$@"

    if [[ -f "$TRIGGER" ]]; then
        rm "$TRIGGER"
        set -- --continue
        continue
    fi
    break
done
```

**To restart** (from within Claude):
```bash
touch ~/.cache/agent-unleashed/process-restart/restart-trigger
kill -INT $(pgrep -f "^claude" | head -1)
```

### Method 2: tmux

```bash
#!/bin/bash
# Working self-restart script (requires tmux)
CLAUDE_PID=$(pgrep -f "^claude" | head -1)
TMUX_TARGET="0:1.1"  # Adjust to your tmux pane

# Watch for Claude death, then send restart command
(while kill -0 $CLAUDE_PID 2>/dev/null; do sleep 0.1; done
 sleep 0.5
 tmux send-keys -t $TMUX_TARGET 'claude --continue' Enter) &

# Kill Claude - the watcher will restart it
kill -INT $CLAUDE_PID
```

### Why This Works (When Others Don't)

The key insight is that **Claude cannot spawn its own replacement**. Every previous approach tried:
1. Spawn new Claude process (nohup, setsid, etc.)
2. Kill current Claude

This fails because:
- The spawned process doesn't survive properly
- Even with full process detachment (setsid), something prevents the new Claude from taking over
- The new process may start but doesn't connect to the terminal

The working approach **inverts this**:
1. Set up an **external watcher** (background shell process)
2. The watcher monitors Claude's PID
3. Kill Claude
4. **After Claude dies**, the watcher sends the restart command to tmux
5. tmux types the command into the now-empty shell
6. New Claude starts fresh

The critical difference: The restart command is sent **after** Claude dies, to the **shell** (via tmux), not spawned as a child process of Claude.

## Previous Failed Approaches

### 1. nohup Spawn (Failed)
```bash
nohup claude --resume $SESSION_ID &
kill -TERM $CLAUDE_PID
```
**Result**: New process doesn't survive or doesn't connect to terminal.

### 2. setsid Spawn (Failed)
```bash
setsid claude --resume $SESSION_ID < /dev/null > /dev/null 2>&1 &
kill -TERM $CLAUDE_PID
```
**Result**: Process appears to spawn but doesn't take over.

### 3. Stop Hook (Partial)
```bash
# In Stop hook:
nohup claude --resume $SESSION_ID &
```
**Result**: Only works with graceful `/exit`, not SIGTERM. Still has spawn issues.

### 4. Double Fork Daemon (Not Tested)
Would require more complex process management. The tmux solution is simpler and works.

## Requirements for Working Method

1. **tmux** - Must be running inside a tmux session
2. **Correct tmux target** - Need to know the pane ID (e.g., `0:1.1`)
3. **Restart command** - Must resume the session (e.g., `claude --continue` or alias)

### Detecting Tmux Pane

```bash
# Get current tmux pane
tmux display-message -p '#{session_name}:#{window_index}.#{pane_index}'
# Example output: 0:1.1
```

## Implementation Notes

### Session ID Discovery (Works)

Successfully implemented - finds session ID from Claude's project files:
```bash
PROJECT_PATH=$(echo "${WORKING_DIR}" | sed 's|^/||; s|/|-|g')
PROJECT_DIR="${HOME}/.claude/projects/-${PROJECT_PATH}"
SESSION_FILE=$(find "${PROJECT_DIR}" -maxdepth 1 -name "*.jsonl" \
  ! -name "agent-*.jsonl" -type f -printf '%T@ %p\n' \
  | sort -rn | head -1 | cut -d' ' -f2-)
SESSION_ID=$(basename "${SESSION_FILE}" .jsonl)
```

### State Preservation (Works)

State file creation and restoration via SessionStart hook works correctly.

### MCP Reconnection

After restart, run `/mcp` in Claude Code to reconnect to MCP servers.

## Files to Update

| File | Status | Notes |
|------|--------|-------|
| `scripts/trigger-restart.sh` | Needs rewrite | Implement tmux method |
| `README.md` | Needs update | Document tmux requirement, remove false claims |
| `commands/restart.md` | Needs update | Correct technical details |
| `hooks-handlers/restart-handler.sh` | Optional | May not be needed with tmux approach |

## Testing the Solution

```bash
# 1. Start Claude inside tmux
tmux new-session -s claude
claude

# 2. Find your tmux pane
tmux display-message -p '#{session_name}:#{window_index}.#{pane_index}'

# 3. Run the restart (from within Claude)
# Claude will execute the script, die, and be restarted by the watcher
```

## Integration with omni-mcp

This can be integrated with omni-mcp's `restart_mcp` tool:
1. `restart_mcp` restarts the MCP server
2. User runs `/mcp` to reconnect
3. Or: create a combined tool that restarts both MCP and Claude

## Credit

Working solution discovered during omni-mcp development session, 2026-01-06.
