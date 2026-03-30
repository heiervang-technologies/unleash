# Process Restart Plugin

Enables Claude Code to restart itself while preserving session state and conversation history.

## Quick Start

```bash
# Start Claude with restart capability
unleash
# Or use the alias
unleash

# From within Claude, to restart:
restart-claude

# To exit without restarting:
exit-claude
```

## Installation

The `unleash` wrapper and commands should be symlinked to `~/bin`:

```bash
ln -sf ~/unleash/scripts/wrapper.sh ~/bin/unleash
ln -sf ~/unleash/scripts/restart-claude ~/bin/
ln -sf ~/unleash/scripts/exit-claude ~/bin/

# Optional: add alias to your shell config
alias unleash='unleash'
```

## Commands

| Command | Description |
|---------|-------------|
| `restart-claude` | Restart Claude, preserving session (`--continue`) |
| `restart-claude "message"` | Restart with custom initial message |
| `exit-claude` | Exit Claude and wrapper (no restart) |

## How It Works

Claude cannot spawn its own replacement directly. The solution uses an **external wrapper** that survives Claude's death and can restart it.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     unleash                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────┐                                        │
│  │ unleash │ ← Wrapper (while loop), holds TTY      │
│  │   PID: 1234     │                                        │
│  └────────┬────────┘                                        │
│           │                                                 │
│           ↓                                                 │
│  ┌─────────────────┐      ┌──────────────────┐              │
│  │  Claude Code    │ ──→  │  restart-claude  │              │
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
│           │               │ Wrapper detects  │              │
│           │               │ trigger file     │              │
│           │               └────────┬─────────┘              │
│           │                        │                        │
│           └────────────────────────┘                        │
│              Wrapper restarts Claude                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Process Isolation

Multiple concurrent `unleash` instances are fully isolated:

- Each wrapper uses a unique trigger file: `restart-trigger-${WRAPPER_PID}`
- `CLAUDE_WRAPPER_PID` env var ensures commands target the correct instance
- `restart-claude` finds Claude's PID via process tree traversal
- No race conditions between concurrent sessions

### Environment Variables

| Variable | Description |
|----------|-------------|
| `AGENT_UNLEASH=1` | Set when running under wrapper |
| `AGENT_WRAPPER_PID` | PID of the wrapper process |

## What Gets Preserved

### Preserved via `--continue`
- Session ID (conversation history)
- Message history
- Working directory context

### Reloaded Fresh
- MCP server connections (run `/mcp` to reconnect)
- Plugin configurations
- Runtime state

## What Does NOT Persist

- Active tool executions (interrupted)
- Streaming responses (cut off)
- MCP connections (must run `/mcp` after restart)
- Background processes spawned by tools

## Post-Restart

After restart:

1. Run `/mcp` to reconnect MCP servers
2. Continue where you left off

## File Locations

| File | Purpose |
|------|---------|
| `scripts/wrapper.sh` | Main wrapper script |
| `scripts/restart-claude` | Restart command |
| `scripts/exit-claude` | Exit command |
| `~/.cache/unleash/process-restart/` | Trigger files |

## Troubleshooting

### "Not running under unleash wrapper"

Start Claude with `unleash` or `unleash` instead of `claude` directly.

### MCP servers disconnected after restart

Run `/mcp` in Claude Code to reconnect.

### Restart doesn't preserve session

1. Ensure wrapper adds `--continue` flag (it does by default)
2. Check session exists: `ls ~/.claude/projects/`

## Alternative: tmux Method

If you can't use the wrapper, you can run Claude inside tmux and use `trigger-restart.sh`:

```bash
tmux new-session -s claude
claude

# To restart (from within Claude):
~/unleash/scripts/trigger-restart.sh
```

## Version History

- **1.2.0** (2026-01-07) - `restart-claude` and `exit-claude` commands
  - Added simple commands for restart/exit
  - Process isolation with wrapper-specific trigger files
  - No more manual pgrep/kill needed

- **1.1.0** (2026-01-06) - Working wrapper-based self-restart
  - Discovered working restart method using wrapper loop
  - Added tmux fallback method

- **1.0.0** (2026-01-01) - Initial release

## License

Same as unleash parent repository.

## Author

Heiervang Technologies
