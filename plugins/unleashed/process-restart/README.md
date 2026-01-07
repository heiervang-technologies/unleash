# Process Restart Plugin

Restart Claude Code while preserving your session state, conversation history, and working context.

## Requirements

Self-restart requires **one of** the following:

1. **Wrapper method** (recommended) - No dependencies
2. **tmux method** - Requires tmux

## Quick Start

### Option 1: Wrapper Method (Recommended)

```bash
# Use the wrapper script instead of 'claude' directly
~/claude-unleashed/plugins/unleashed/process-restart/scripts/claude-wrapper.sh

# Or create an alias
alias cw='~/claude-unleashed/plugins/unleashed/process-restart/scripts/claude-wrapper.sh'
cw
```

### Option 2: tmux Method

```bash
# Start Claude inside tmux
tmux new-session -s claude
claude
```

## Overview

This plugin provides:
- Self-restart capability (Claude can restart itself)
- Two restart methods (wrapper and tmux)
- Session state preservation across restarts
- Automatic method detection

## How It Works

Claude cannot spawn its own replacement directly. Previous approaches (nohup, setsid, Stop hooks) **do not work**.

The solution requires an **external coordinator** that:
1. Survives Claude's death
2. Holds the TTY
3. Can start a new Claude

### Method 1: Wrapper (Recommended)

The wrapper script is a while loop that:
1. Runs Claude as a child process
2. When Claude exits, checks for a trigger file
3. If trigger exists, restarts Claude with `--continue`

```bash
# claude-wrapper.sh (simplified)
while true; do
    claude "$@"
    if [[ -f ~/.cache/claude-unleashed/process-restart/restart-trigger ]]; then
        rm ~/.cache/claude-unleashed/process-restart/restart-trigger
        set -- --continue
        continue
    fi
    break
done
```

**To restart**, Claude:
1. Creates the trigger file
2. Kills itself
3. Wrapper detects exit, sees trigger, restarts

```bash
touch ~/.cache/claude-unleashed/process-restart/restart-trigger
kill -INT $(pgrep -f "^claude" | head -1)
```

### Method 2: tmux

A background watcher monitors Claude's PID and sends the restart command to tmux after Claude dies.

```bash
CLAUDE_PID=$(pgrep -f "^claude" | head -1)
TMUX_TARGET=$(tmux display-message -p '#{session_name}:#{window_index}.#{pane_index}')

# Watcher waits for death, then restarts
(while kill -0 $CLAUDE_PID 2>/dev/null; do sleep 0.1; done
 sleep 0.5
 tmux send-keys -t $TMUX_TARGET 'claude --continue' Enter) &

kill -INT $CLAUDE_PID
```

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    WRAPPER METHOD                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────┐                                        │
│  │ claude-wrapper  │ ← Holds TTY, survives Claude death     │
│  │   (while loop)  │                                        │
│  └────────┬────────┘                                        │
│           │                                                 │
│           ↓                                                 │
│  ┌─────────────────┐      ┌──────────────────┐              │
│  │  Claude Code    │ ──→  │ /restart command │              │
│  │   (running)     │      └────────┬─────────┘              │
│  └─────────────────┘               │                        │
│           ↑                        ↓                        │
│           │               ┌──────────────────┐              │
│           │               │ Create trigger   │              │
│           │               │ file + kill self │              │
│           │               └────────┬─────────┘              │
│           │                        │                        │
│           │                        ↓                        │
│           │               ┌──────────────────┐              │
│           │               │ Claude exits     │              │
│           │               └────────┬─────────┘              │
│           │                        │                        │
│           │                        ↓                        │
│           │               ┌──────────────────┐              │
│           │               │ Wrapper checks   │              │
│           │               │ trigger file     │              │
│           │               └────────┬─────────┘              │
│           │                        │                        │
│           └────────────────────────┘                        │
│                    (loop continues)                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Commands

### `/restart`

Restart Claude Code while preserving your session.

**Usage:**
```
/restart              # Standard restart with session resume
/restart --force      # Skip any confirmation prompts
/restart --clean      # Restart without preserving state (fresh session)
```

## What Gets Preserved

When you restart:

### Preserved via `--continue`/`--resume`
- Session ID (conversation history)
- Message history
- Working directory context

### Reloaded Fresh
- MCP server connections (run `/mcp` to reconnect)
- Plugin configurations
- Any runtime state

## What Does NOT Persist

- Active tool executions (interrupted)
- Streaming responses (cut off)
- MCP connections (must run `/mcp` after restart)
- Background processes spawned by tools

## Post-Restart Steps

After restart completes:

1. **Reconnect MCP servers**: Run `/mcp` in Claude Code
2. **Verify session**: Check conversation history is accessible
3. **Continue working**: Resume where you left off

## Configuration

Configure in `.claude/settings.json`:

```json
{
  "plugins": {
    "process-restart": {
      "restartCommand": "claude --continue",
      "tmuxAutoDetect": true
    }
  }
}
```

### Settings

- **`restartCommand`** (string, default: `"claude --continue"`)
  - Command to restart Claude with session resume
  - Can be an alias like `cy --continue`

- **`tmuxAutoDetect`** (boolean, default: `true`)
  - Automatically detect current tmux pane
  - Set to false to manually specify pane

## Troubleshooting

### "Not running in tmux"

**Problem**: Restart fails because Claude isn't in a tmux session

**Solution**:
```bash
# Exit Claude, start tmux, then restart Claude
exit
tmux new-session -s claude
claude --continue  # or your session resume command
```

### Restart doesn't preserve session

**Problem**: New session starts fresh

**Solutions**:
1. Ensure using `--continue` or `--resume` flag
2. Check session ID exists: `ls ~/.claude/projects/`
3. Verify Claude finds correct session

### MCP servers disconnected after restart

**Problem**: MCP tools not available after restart

**Solution**: Run `/mcp` in Claude Code to reconnect

### Tmux pane not detected

**Problem**: Script can't find correct tmux pane

**Solution**:
```bash
# Manually check your pane
tmux display-message -p '#{session_name}:#{window_index}.#{pane_index}'
# Update restartCommand with explicit pane if needed
```

## Technical Details

### File Locations

| File | Purpose |
|------|---------|
| `scripts/trigger-restart.sh` | Main restart script with tmux method |
| `commands/restart.md` | Skill documentation |
| `hooks-handlers/session-restore.sh` | Optional: restore state on session start |

### Why tmux?

tmux provides:
1. **Process isolation** - Watcher survives Claude's death
2. **Input injection** - Can type commands into the shell
3. **Session persistence** - Shell stays alive between Claude instances

No other terminal multiplexer or method has been found to work reliably.

## Version History

- **1.1.0** (2026-01-06) - Working tmux-based self-restart
  - Discovered working restart method using tmux PID monitoring
  - Removed non-functional nohup/setsid approaches
  - Added tmux as requirement
  - Updated documentation to reflect actual behavior

- **1.0.0** (2026-01-01) - Initial release (self-restart non-functional)
  - Session state preservation
  - Hook integration
  - State file management

## License

Same as Claude Unleashed parent repository.

## Author

Heiervang Technologies
