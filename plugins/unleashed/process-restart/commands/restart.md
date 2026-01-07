---
name: restarting
description: Restart Claude Code while preserving your session
---

# Restart Claude Code Process

Restarts the Claude Code process while preserving your current session and conversation history.

**Requirement**: Must use one of:
- **Wrapper method**: Start Claude via `claude-wrapper.sh` (recommended)
- **tmux method**: Run Claude inside tmux

## Usage

- `/restart` - Restart with session resume
- `/restart --force` - Skip confirmation prompts
- `/restart --clean` - Fresh restart without session preservation

## How It Works

The script auto-detects which method is available and uses it.

### Method 1: Wrapper (Recommended)

If Claude was started via `claude-wrapper.sh`:

1. Create trigger file
2. Kill Claude
3. Wrapper detects exit, sees trigger file
4. Wrapper restarts Claude with `--continue`

```
claude-wrapper.sh (while loop)
        │
        ↓ runs
    Claude Code
        │
        ↓ /restart
    Create trigger file + kill self
        │
        ↓
    Claude exits
        │
        ↓
    Wrapper checks trigger → found
        │
        ↓
    Wrapper restarts Claude --continue
```

### Method 2: tmux (Fallback)

If running inside tmux:

1. Spawn background watcher (monitors PID)
2. Kill Claude
3. Watcher detects death
4. Watcher sends restart command via `tmux send-keys`
5. Shell receives command, starts new Claude

### Why These Methods?

Standard approaches **do not work**:
- `nohup claude &` - Process doesn't survive
- `setsid claude &` - Process spawns but doesn't take over
- Stop hooks - Only fire on graceful `/exit`, not SIGTERM

Both methods work because they provide an **external coordinator** that:
- Survives Claude's death
- Holds/accesses the TTY
- Can start a new Claude

## What Gets Preserved

### Via `--continue`/`--resume`
- Session ID (conversation history)
- Message history accessible
- Working directory context

### Must Be Manually Restored
- MCP connections - run `/mcp` after restart

## What Does NOT Persist

- Active tool executions (interrupted)
- Streaming responses (cut off)
- MCP server connections
- Background processes

## Post-Restart

After restart completes:

1. Run `/mcp` to reconnect MCP servers
2. Verify conversation history is accessible
3. Continue where you left off

## Requirements

### tmux

Claude must be running inside tmux:

```bash
# If not in tmux, exit and restart in tmux
tmux new-session -s claude
claude
```

Check if you're in tmux:
```bash
echo $TMUX  # Should show tmux socket path
```

### Restart Command

The plugin uses `claude --continue` by default. Configure a different command in settings:

```json
{
  "plugins": {
    "process-restart": {
      "restartCommand": "cy --continue"
    }
  }
}
```

## Troubleshooting

### "Not running in tmux"

Exit Claude, start tmux, resume session:
```bash
# Exit current Claude
/exit

# Start tmux and resume
tmux new-session -s claude
claude --continue
```

### MCP tools unavailable after restart

Run `/mcp` in Claude Code to reconnect to MCP servers.

### Session not preserved

1. Ensure restart command includes `--continue` or `--resume`
2. Check session exists: `ls ~/.claude/projects/`

### Restart hangs or doesn't complete

1. Check tmux pane: `tmux display-message -p '#{session_name}:#{window_index}.#{pane_index}'`
2. Manually kill Claude if stuck: `pkill -9 claude`
3. Restart manually: `claude --continue`

## Technical Details

### Script Location

`plugins/unleashed/process-restart/scripts/trigger-restart.sh`

### Core Logic

```bash
#!/bin/bash
CLAUDE_PID=$(pgrep -f "^claude" | head -1)
TMUX_TARGET=$(tmux display-message -p '#{session_name}:#{window_index}.#{pane_index}')
RESTART_CMD="${RESTART_COMMAND:-claude --continue}"

# Watcher process
(while kill -0 $CLAUDE_PID 2>/dev/null; do sleep 0.1; done
 sleep 0.5
 tmux send-keys -t $TMUX_TARGET "$RESTART_CMD" Enter) &

# Kill Claude
kill -INT $CLAUDE_PID
```

## Version

- **1.1.0** - Working tmux-based restart (2026-01-06)
- **1.0.0** - Initial release (self-restart non-functional)
