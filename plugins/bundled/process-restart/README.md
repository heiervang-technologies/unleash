# Process Restart Plugin

Enables Claude Code to restart itself while preserving session state and conversation history.

## Quick Start

```bash
# Start with restart capability
unleash

# From within the agent, to restart:
unleash-refresh

# To exit without restarting:
unleash-exit
```

## Commands

| Command | Description |
|---------|-------------|
| `unleash-refresh` | Restart agent, preserving session (`--continue`) |
| `unleash-refresh "message"` | Restart with custom initial message |
| `unleash-exit` | Exit agent and wrapper (no restart) |

## How It Works

The agent cannot spawn its own replacement directly. The solution uses an **external wrapper** that survives the agent's exit and can restart it.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        unleash                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────┐                                        │
│  │ unleash wrapper │ ← Wrapper (while loop), holds TTY      │
│  │   PID: 1234     │                                        │
│  └────────┬────────┘                                        │
│           │                                                 │
│           ↓                                                 │
│  ┌─────────────────┐      ┌──────────────────┐              │
│  │  Agent CLI      │ ──→  │ unleash-refresh  │              │
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
│              Wrapper restarts agent                         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Process Isolation

Multiple concurrent `unleash` instances are fully isolated:

- Each wrapper uses a unique trigger file: `restart-trigger-${WRAPPER_PID}`
- `AGENT_WRAPPER_PID` env var ensures commands target the correct instance
- `unleash-refresh` finds the agent's PID via process tree traversal
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
| `scripts/unleash-refresh` | Restart command |
| `scripts/unleash-exit` | Exit command |
| `~/.cache/unleash/process-restart/` | Trigger files |

Note: The old aliases `restart-claude` and `exit-claude` still work for backward compatibility.

## Troubleshooting

### "Not running under unleash wrapper"

Start with `unleash` instead of running the agent CLI directly.

### MCP servers disconnected after restart

Run `/mcp` in Claude Code to reconnect.

### Restart doesn't preserve session

1. Ensure wrapper adds `--continue` flag (it does by default)
2. Check session exists: `ls ~/.claude/projects/`

## License

Same as unleash parent repository.

## Author

Heiervang Technologies
