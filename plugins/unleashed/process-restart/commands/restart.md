---
name: restarting
description: Restart Claude Code while preserving your session
---

# Restart Claude Code Process

Restarts the Claude Code process while preserving your current session, conversation history, and working state.

## Usage

- `/restart` - Restart with session preservation
- `/restart --force` - Force restart without confirmation
- `/restart --clean` - Restart without preserving state (fresh session)

## What Gets Preserved

When you restart, the following state is preserved:

### Session Information
- Session ID (maintains conversation history)
- Message history (can be resumed)
- Working directory
- Current branch (if in git repository)

### Configuration
- Model selection (e.g., claude-sonnet-4-5)
- Permission mode (auto-allow, manual, etc.)
- Enabled plugins
- Plugin settings

### MCP Servers
- All MCP servers are reinitialized with current configuration
- Changes to `.mcp.json` or `.claude.json` are automatically applied
- OAuth tokens are reused (if still valid)

## What Does NOT Persist

Some runtime state cannot be preserved:

- Active tool executions (interrupted)
- Streaming responses (will be cut off)
- Temporary files created during session
- Background processes spawned by tools

## How It Works

1. **Save State**: Creates a state file at `~/.cache/claude-unleashed/restart-state.json`
2. **Exit Gracefully**: Allows Claude Code to shut down cleanly
3. **Spawn New Process**: Starts a new Claude Code instance
4. **Restore State**: New process reads state file and resumes session
5. **Clean Up**: State file is removed after successful restoration

## State File Location

```
~/.cache/claude-unleashed/restart-state.json
```

The state file is automatically cleaned up after:
- Successful restoration
- Expiry (default: 5 minutes)
- Manual cleanup via `/restart --clean`

## Use Cases

### Apply MCP Configuration Changes

```
# 1. Edit MCP configuration
vim .mcp.json

# 2. Restart to apply changes
/restart
```

### Recover from Plugin Issue

```
# Something went wrong with a plugin
/restart --clean
```

### Update Plugin Settings

```
# After changing plugin settings in .claude/settings.json
/restart
```

## Safety Features

### Confirmation Prompt

By default, you'll be asked to confirm before restarting:

```
⚠️  This will restart the Claude Code process.
   Your session will be preserved and automatically resumed.

   Preserve:
   - Session ID: a8ea16a
   - Working directory: /home/me/my-project
   - Model: claude-sonnet-4-5
   - Permission mode: auto-allow

Proceed with restart? (y/n):
```

Use `--force` to skip confirmation.

### Active Tool Detection

If there are active tool executions, you'll receive a warning:

```
⚠️  Warning: Active tool execution detected

   The following tools are currently running:
   - Bash: npm install (running for 45s)

   Restarting now will interrupt these operations.

Proceed anyway? (y/n):
```

### State File Expiry

State files expire after 5 minutes (configurable) to prevent:
- Stale state restoration
- Disk space accumulation
- Confusion from old sessions

## Configuration

Configure restart behavior in `.claude/settings.json`:

```json
{
  "plugins": {
    "process-restart": {
      "preserveSession": true,
      "preserveWorkingDir": true,
      "preservePermissions": true,
      "stateFileExpiry": 300
    }
  }
}
```

## Technical Details

### State File Format

```json
{
  "version": "1.0.0",
  "timestamp": 1735689600,
  "sessionId": "a8ea16a",
  "workingDir": "/home/me/my-project",
  "model": "claude-sonnet-4-5",
  "permissionMode": "auto-allow",
  "gitBranch": "feature/my-feature",
  "enabledPlugins": ["mcp-refresh", "process-restart"]
}
```

### Restart Flow

```
User runs /restart
       ↓
Stop hook triggered
       ↓
Save state to file
       ↓
Spawn new process with --resume flag
       ↓
Allow current process to exit
       ↓
New process starts
       ↓
SessionStart hook triggered
       ↓
Read and apply state file
       ↓
Resume session
       ↓
Clean up state file
```

## Integration with MCP Refresh

This command integrates with the `mcp-refresh` plugin:

When MCP configuration changes are detected, you'll be prompted to use `/restart` to apply them. The restart preserves your session while loading the new MCP configuration.

## Troubleshooting

### Restart doesn't preserve session

**Problem**: Session starts fresh after restart

**Solutions**:
1. Check that `preserveSession` is enabled in settings
2. Verify state file was created: `cat ~/.cache/claude-unleashed/restart-state.json`
3. Check file permissions on cache directory
4. Look for errors in Claude Code logs

### State file not found

**Problem**: "State file expired or not found" message

**Solutions**:
1. State file may have expired (default: 5 minutes)
2. Increase expiry: Set `stateFileExpiry` to larger value
3. Check cache directory exists: `~/.cache/claude-unleashed/`

### Process doesn't restart

**Problem**: Current process exits but new one doesn't start

**Solutions**:
1. Check Claude Code is in PATH: `which claude`
2. Verify Claude Code executable: `claude --version`
3. Check logs in `~/.claude/logs/`
4. Try manual restart: `claude --resume <session-id>`

## Related Commands

- `/reload-mcps` - Check for MCP configuration changes before restarting
- `/mcp-status` - View current MCP server status
- `/exit` - Exit without restarting (session preserved for later resume)

## Security Considerations

### State File Security

The state file is stored in your user cache directory with restricted permissions (600). It contains:

- Session ID (sensitive)
- Working directory path
- Configuration preferences

**Do not**:
- Share state files between users
- Modify state files manually
- Store state files in version control

### OAuth Tokens

OAuth tokens for MCP servers are NOT stored in the restart state file. They are managed separately by Claude Code's credential storage.

## Performance Impact

Restarting takes approximately:
- Exit: < 1 second
- New process start: 2-3 seconds
- Session restoration: < 1 second
- Total: ~3-5 seconds

This is comparable to manually exiting and starting Claude Code, but with the added benefit of automatic session resumption.

## Version History

- **1.0.0** - Initial release
  - Session ID preservation
  - Working directory restoration
  - Model and permission mode preservation
  - Automatic state cleanup
  - Integration with MCP refresh
