# Process Restart Plugin

Restart Claude Code while preserving your session state, conversation history, and working context.

## Overview

This plugin provides:
- Seamless process restart without losing your session
- Automatic state preservation and restoration
- Integration with MCP configuration reloading
- Safety features to prevent accidental data loss

## Features

### Session Preservation

The plugin automatically saves and restores your session state across restarts:
- **Session ID** - Maintains conversation history continuity
- **Working directory** - Returns to your project location
- **Model selection** - Preserves your chosen model (e.g., claude-sonnet-4-5)
- **Git context** - Remembers your current branch
- **Plugin configuration** - Maintains enabled plugins and settings

When you restart, you can continue exactly where you left off with full access to your conversation history.

### MCP Server Reloading

Configuration changes to MCP (Model Context Protocol) servers are automatically applied during restart:
- Updates from `.mcp.json` (project-level)
- Updates from `.claude.json` (user-level)
- Plugin-level MCP configurations
- OAuth tokens reused when still valid

This makes `/restart` the recommended way to apply MCP configuration changes detected by the `mcp-refresh` plugin.

### Safety Features

- **Confirmation prompts** - Asks before restarting (unless `--force` used)
- **State file expiry** - Prevents stale state restoration (default: 5 minutes)
- **Graceful shutdown** - Allows Claude Code to clean up properly
- **Error handling** - Falls back to normal start if restoration fails

## Commands

### `/restart`

Restart Claude Code while preserving your session.

**Usage:**
```
/restart              # Standard restart with confirmation
/restart --force      # Skip confirmation prompt
/restart --clean      # Restart without preserving state (fresh session)
```

**Example interaction:**
```
You: /restart

⚠️  This will restart the Claude Code process.
   Your session will be preserved and automatically resumed.

   Preserve:
   - Session ID: a8ea16a
   - Working directory: /home/me/my-project
   - Model: claude-sonnet-4-5
   - Git branch: feature/my-feature

Proceed with restart? (y/n): y

✅ Restart initiated. New Claude Code process started.
   Session will be restored automatically.

[Process exits and new process starts]

🔄 Session restored from restart

Restored state:
- Session ID: a8ea16a
- Working directory: /home/me/my-project
- Model: claude-sonnet-4-5
- Git branch: feature/my-feature

MCP servers reloaded with current configuration.

You can continue where you left off.
```

## Installation

1. The plugin is already included in Claude Unleashed
2. Enable it in `.claude/settings.json`:

```json
{
  "plugins": {
    "enabled": [
      "process-restart",
      "mcp-refresh"
    ]
  }
}
```

**Note**: This plugin works best when paired with `mcp-refresh` for detecting and applying MCP configuration changes.

## Configuration

Configure the plugin in `.claude/settings.json`:

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

### Settings

- **`preserveSession`** (boolean, default: `true`)
  - Preserve session ID and conversation history across restarts
  - Disable to always start fresh sessions after restart

- **`preserveWorkingDir`** (boolean, default: `true`)
  - Restore working directory after restart
  - Disable to start in default directory

- **`preservePermissions`** (boolean, default: `true`)
  - Restore permission mode (auto-allow, manual, etc.) after restart
  - Disable to reset to default permissions

- **`stateFileExpiry`** (number, default: `300`)
  - State file expiry time in seconds (default: 5 minutes)
  - Prevents restoration of stale state
  - Increase for longer-running restarts, decrease for stricter freshness

## How It Works

### Architecture

```
┌─────────────────────────────────────┐
│  User Runs /restart Command         │
└──────────┬──────────────────────────┘
           │
           ↓
┌─────────────────────────────────────┐
│  Trigger File Created               │
│  ~/.cache/.../restart-trigger       │
└──────────┬──────────────────────────┘
           │
           ↓ (Claude Code initiates exit)
┌─────────────────────────────────────┐
│  Stop Hook Triggered                │
│  - Detects restart trigger          │
│  - Saves session state              │
│  - Spawns new process               │
└──────────┬──────────────────────────┘
           │
           ↓
┌─────────────────────────────────────┐
│  State Saved to File                │
│  ~/.cache/.../restart-state.json    │
└──────────┬──────────────────────────┘
           │
           ↓
┌─────────────────────────────────────┐
│  New Process Spawned                │
│  claude --resume <session-id>       │
└──────────┬──────────────────────────┘
           │
           ↓
┌─────────────────────────────────────┐
│  Current Process Exits              │
└──────────┬──────────────────────────┘
           │
           ↓
┌─────────────────────────────────────┐
│  New Process Starts                 │
│  SessionStart Hook Triggered        │
└──────────┬──────────────────────────┘
           │
           ↓
┌─────────────────────────────────────┐
│  State Restored                     │
│  - Read state file                  │
│  - Apply working directory          │
│  - Resume session                   │
│  - Reload MCP servers               │
└──────────┬──────────────────────────┘
           │
           ↓
┌─────────────────────────────────────┐
│  State File Cleaned Up              │
│  Session Resumed                    │
└─────────────────────────────────────┘
```

### Restart Flow

1. **Command Execution**: User runs `/restart` command
2. **Trigger Creation**: `trigger-restart.sh` creates trigger file
3. **Exit Initiation**: Claude Code begins shutdown process
4. **Stop Hook**: `restart-handler.sh` detects trigger and saves state
5. **Process Spawn**: New Claude Code process started with `--resume` flag
6. **Current Exit**: Original process exits gracefully
7. **Session Start**: New process starts, `session-restore.sh` runs
8. **State Restoration**: Working directory, session ID, etc. restored
9. **MCP Reload**: MCP servers initialized with current configuration
10. **Cleanup**: State file removed after successful restoration

### Hook Integration

The plugin uses two lifecycle hooks:

**Stop Hook** (`hooks-handlers/restart-handler.sh`):
- Intercepts process exit
- Checks for restart trigger file
- Saves current state to JSON file
- Spawns new Claude Code process
- Allows current process to exit

**SessionStart Hook** (`hooks-handlers/session-restore.sh`):
- Runs at start of new session
- Checks for state file
- Validates file age (expiry check)
- Restores session context
- Cleans up state file

## State Preservation

### What Gets Preserved

When you restart with session preservation enabled:

#### Session Information
- **Session ID** - Maintains conversation history and context
- **Message history** - Full conversation available in new session
- **Working directory** - Returns to your project location
- **Git branch** - Remembers current branch (if in git repository)

#### Configuration
- **Model selection** - Your chosen model (claude-sonnet-4-5, opus, etc.)
- **Permission mode** - auto-allow, manual, or other settings
- **Enabled plugins** - Which plugins are active
- **Plugin settings** - Configuration from `.claude/settings.json`

#### MCP State
- **Server configurations** - All MCP servers from config files
- **Connection state** - Servers reinitialized on startup
- **Current config** - Latest `.mcp.json` and `.claude.json` changes applied

### What Does NOT Persist

Some runtime state cannot be preserved:

#### Active Operations
- **Tool executions in progress** - Running commands will be interrupted
- **Streaming responses** - Partial responses will be cut off
- **File locks** - Any held locks released
- **Network connections** - Active connections closed

#### Temporary State
- **Temporary files** - Files in `/tmp` or session-specific locations
- **Background processes** - Processes spawned by tools will terminate
- **Cached data** - In-memory caches cleared
- **Terminal state** - Any terminal-specific context lost

**Recommendation**: Before restarting, ensure no critical operations are running. Check for active tool executions and wait for them to complete.

## Use Cases

### Apply MCP Configuration Changes

When you modify MCP server configurations, restart to apply them:

```bash
# 1. Edit MCP configuration
vim .mcp.json

# 2. Check what changed (optional)
/reload-mcps

# 3. Restart to apply changes
/restart

# Your session continues with updated MCP servers
```

**Integration with mcp-refresh**: The `mcp-refresh` plugin will automatically notify you when MCP configurations change and suggest using `/restart` to apply them.

### Update Plugin Settings

After modifying plugin configuration in `.claude/settings.json`:

```bash
# 1. Edit settings
vim .claude/settings.json

# 2. Restart to reload plugins
/restart

# Plugins reinitialize with new settings
```

### Recover from Plugin Issues

If a plugin misbehaves or causes problems:

```bash
# Option 1: Clean restart (fresh session)
/restart --clean

# Option 2: Disable plugin and restart
# Edit .claude/settings.json to remove plugin from enabled list
/restart
```

### Switch Model or Configuration

To change fundamental settings while preserving your conversation:

```bash
# Model switch happens on restart
# Your conversation history is maintained
/restart
```

### Long-Running Session Cleanup

After extended use, restart to clear accumulated state:

```bash
# Restart periodically to:
# - Clear memory leaks
# - Reload updated configurations
# - Refresh MCP connections
/restart
```

## Safety Features

### Confirmation Prompts

By default, the plugin asks for confirmation before restarting:

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

This prevents accidental restarts that could interrupt important work.

**Skip confirmation**: Use `/restart --force` when you're certain.

### State File Expiry

State files automatically expire after a configurable time (default: 5 minutes):

```json
{
  "plugins": {
    "process-restart": {
      "stateFileExpiry": 300
    }
  }
}
```

**Why expiry matters**:
- Prevents restoring very old state
- Avoids confusion from stale sessions
- Cleans up disk space automatically
- Ensures MCP configs are current

If a state file is expired, you'll see:

```
⚠️  Restart state file found but expired (age: 320s, max: 300s).

Starting fresh session instead.
```

### Graceful Shutdown

The plugin ensures Claude Code exits gracefully:
- Allows cleanup handlers to run
- Closes connections properly
- Saves any pending data
- Releases resources before exit

### Error Recovery

If restoration fails for any reason:
- Falls back to normal session start
- Displays clear error messages
- Doesn't leave system in broken state
- State file cleaned up automatically

## Integration with MCP Refresh

This plugin is designed to work seamlessly with the `mcp-refresh` plugin for a complete MCP management workflow:

### Workflow

1. **Change Detection**: `mcp-refresh` monitors config files for changes
2. **Notification**: You're notified when changes are detected
3. **Review**: Use `/reload-mcps` to see what changed
4. **Apply**: Use `/restart` to apply changes while preserving session
5. **Verification**: New MCP servers load with updated configuration

### Example

```
[You edit .mcp.json to add a new server]

MCP configuration changes detected!

Changes:
  - Added: new-database-server (type: stdio)

To review changes, use: /reload-mcps
To apply changes, use: /restart

You: /reload-mcps

Checking MCP configurations...

Changes detected:
  - Added: new-database-server (type: stdio)
    Command: npx database-mcp-server
    Environment: DATABASE_URL=postgresql://...

To apply these changes, use the /restart command.

You: /restart

✅ Restart initiated. New Claude Code process started.
   Session will be restored automatically.

[Process restarts]

🔄 Session restored from restart

Restored state:
- Session ID: a8ea16a
- Working directory: /home/me/my-project
- Model: claude-sonnet-4-5

MCP servers reloaded with current configuration.

[new-database-server is now available]
```

See the [mcp-refresh plugin](../mcp-refresh/README.md) for details on MCP change detection.

## Troubleshooting

### Restart doesn't preserve session

**Problem**: Session starts fresh after restart, conversation history lost

**Symptoms**:
- No "Session restored" message
- Different session ID
- Conversation history not available

**Solutions**:
1. Check that `preserveSession` is enabled in settings:
   ```bash
   cat .claude/settings.json | grep -A 5 process-restart
   ```

2. Verify state file was created:
   ```bash
   cat ~/.cache/claude-unleashed/process-restart/restart-state.json
   ```

3. Check file permissions on cache directory:
   ```bash
   ls -la ~/.cache/claude-unleashed/process-restart/
   # Should be owned by you with read/write permissions
   ```

4. Look for errors in Claude Code logs:
   ```bash
   tail -f ~/.claude/logs/debug.log
   ```

5. Ensure you're using `/restart` command (not manual exit/start):
   ```bash
   # Correct: Uses plugin
   /restart

   # Incorrect: Doesn't preserve state
   exit
   claude
   ```

### State file not found after restart

**Problem**: "State file expired or not found" message on restart

**Symptoms**:
- Restart works but state not restored
- Warning about expired state file
- Fresh session starts

**Solutions**:
1. State file may have expired (check age vs. expiry setting):
   ```bash
   # Check current expiry setting
   cat .claude/settings.json | grep stateFileExpiry
   ```

2. Increase expiry for slower systems:
   ```json
   {
     "plugins": {
       "process-restart": {
         "stateFileExpiry": 600
       }
     }
   }
   ```

3. Verify cache directory exists and is writable:
   ```bash
   mkdir -p ~/.cache/claude-unleashed/process-restart
   chmod 755 ~/.cache/claude-unleashed/process-restart
   ```

4. Check for disk space issues:
   ```bash
   df -h ~
   ```

### Process doesn't restart automatically

**Problem**: Current process exits but new one doesn't start

**Symptoms**:
- Claude Code exits after `/restart`
- No new process appears
- Terminal returns to shell prompt

**Solutions**:
1. Check Claude Code is in PATH:
   ```bash
   which claude
   # or
   which claude-code
   ```

2. Verify Claude Code executable exists and is executable:
   ```bash
   claude --version
   # Should display version information
   ```

3. Check logs for spawn errors:
   ```bash
   tail -20 ~/.claude/logs/debug.log
   ```

4. Try manual restart to test:
   ```bash
   # Get your session ID first
   cat ~/.cache/claude-unleashed/process-restart/restart-state.json

   # Then manually start
   claude --resume <session-id>
   ```

5. Verify nohup is available:
   ```bash
   which nohup
   # Should return path to nohup
   ```

### Working directory not restored

**Problem**: Restart succeeds but wrong working directory

**Symptoms**:
- Session restored but in different directory
- Wrong project context

**Solutions**:
1. Check that `preserveWorkingDir` is enabled:
   ```json
   {
     "plugins": {
       "process-restart": {
         "preserveWorkingDir": true
       }
     }
   }
   ```

2. Verify directory exists and is accessible:
   ```bash
   # Check state file for directory path
   cat ~/.cache/claude-unleashed/process-restart/restart-state.json

   # Verify directory exists
   ls -ld /path/from/state/file
   ```

3. Check for permission issues:
   ```bash
   # Ensure you can cd to the directory
   cd /path/to/working/dir
   ```

### Multiple restart attempts fail

**Problem**: Repeated `/restart` commands don't work

**Symptoms**:
- First restart works, subsequent ones don't
- Trigger file persists
- State file corruption

**Solutions**:
1. Clean up restart state:
   ```bash
   rm -rf ~/.cache/claude-unleashed/process-restart/*
   ```

2. Check for processes holding locks:
   ```bash
   ps aux | grep claude
   # Kill any orphaned Claude Code processes
   ```

3. Restart Claude Code manually:
   ```bash
   # Complete shutdown
   pkill -9 claude

   # Fresh start
   claude
   ```

## Technical Details

### State File Format

The state file (`~/.cache/claude-unleashed/process-restart/restart-state.json`) contains:

```json
{
  "version": "1.0.0",
  "timestamp": 1735689600,
  "sessionId": "a8ea16a",
  "workingDir": "/home/me/my-project",
  "model": "claude-sonnet-4-5",
  "gitBranch": "feature/my-feature",
  "enabledPlugins": ["mcp-refresh", "process-restart"]
}
```

**Fields**:
- `version`: State file format version (for compatibility)
- `timestamp`: Unix timestamp when state was saved (for expiry check)
- `sessionId`: Claude Code session identifier (for conversation history)
- `workingDir`: Absolute path to working directory
- `model`: Model identifier (claude-sonnet-4-5, claude-opus-4-5, etc.)
- `gitBranch`: Current git branch name (empty if not in git repo)
- `enabledPlugins`: Array of enabled plugin names

### Restart Flow Details

```
┌───────────────────────┐
│ /restart command      │
│                       │
│ trigger-restart.sh    │
└─────────┬─────────────┘
          │
          ↓ Creates trigger file
┌──────────────────────────┐
│ Trigger File             │
│ ~/.cache/.../            │
│   restart-trigger        │
└─────────┬────────────────┘
          │
          ↓ Claude exits, Stop hook fires
┌──────────────────────────┐
│ Stop Hook                │
│ restart-handler.sh       │
│                          │
│ 1. Detect trigger        │
│ 2. Read session state    │
│ 3. Save to JSON          │
│ 4. Spawn new process     │
│ 5. Exit current process  │
└─────────┬────────────────┘
          │
          ↓ Writes state
┌──────────────────────────┐
│ State File               │
│ restart-state.json       │
│                          │
│ - sessionId              │
│ - workingDir             │
│ - model                  │
│ - gitBranch              │
│ - enabledPlugins         │
└─────────┬────────────────┘
          │
          ↓ New process reads
┌──────────────────────────┐
│ SessionStart Hook        │
│ session-restore.sh       │
│                          │
│ 1. Find state file       │
│ 2. Validate age          │
│ 3. Extract state         │
│ 4. Apply to session      │
│ 5. Clean up file         │
└─────────┬────────────────┘
          │
          ↓
┌──────────────────────────┐
│ Restored Session         │
│                          │
│ - Same session ID        │
│ - Same directory         │
│ - Same configuration     │
│ - MCP servers reloaded   │
└──────────────────────────┘
```

### File Permissions

All cache files use restrictive permissions for security:

```bash
# Cache directory
~/.cache/claude-unleashed/process-restart/
drwxr-xr-x (755) - readable by all, writable by owner

# State file (contains sensitive session info)
restart-state.json
-rw------- (600) - readable/writable only by owner

# Trigger file
restart-trigger
-rw-r--r-- (644) - readable by all, writable by owner
```

The state file is particularly restricted because it contains:
- Session ID (access to conversation history)
- Working directory path (potentially sensitive)
- Configuration details

### Process Spawning

The plugin uses `nohup` to spawn the new process:

```bash
nohup claude \
  --cwd "/working/dir" \
  --model "claude-sonnet-4-5" \
  --resume "session-id" \
  > /dev/null 2>&1 &
```

**Why nohup**:
- Allows process to continue after parent exits
- Prevents signal propagation (SIGHUP)
- Detaches from terminal
- Redirects output to avoid blocking

**Process lifecycle**:
1. Parent process (current Claude) spawns child
2. Child process starts in background
3. Parent waits briefly (0.5s) for child to start
4. Parent exits gracefully
5. Child continues running independently
6. Child becomes new session leader

## Security Considerations

### State File Security

The state file contains potentially sensitive information:

**What's included**:
- Session ID (grants access to conversation history)
- Working directory path (may reveal project structure)
- Model and configuration (may indicate usage patterns)
- Git branch name (may reveal project status)

**Security measures**:
- File permissions: 600 (owner read/write only)
- Location: User-specific cache directory
- Automatic expiry: Removes old state files
- Automatic cleanup: Deleted after successful restoration

**Best practices**:
```bash
# DO: Let the plugin manage state files
/restart

# DON'T: Share state files
cp ~/.cache/claude-unleashed/process-restart/restart-state.json /shared/

# DON'T: Commit to version control
# Add to .gitignore:
.cache/
*.restart-state.json

# DON'T: Modify manually
# Use plugin commands instead
```

### OAuth Tokens and Credentials

**Important**: OAuth tokens and credentials are NOT stored in restart state files.

- MCP OAuth tokens managed by Claude Code's secure credential storage
- Tokens persist across restarts automatically
- No additional credential handling needed
- State file only references MCP server names, not secrets

### Multi-User Systems

On multi-user systems:

```bash
# Cache directory is user-specific
~/.cache/claude-unleashed/process-restart/

# State files isolated per user
# User A cannot access User B's state files
# File permissions prevent cross-user access
```

**Recommendation**: On shared systems, consider:
- Shorter state file expiry times
- Regular cleanup of cache directory
- Monitoring for unauthorized access

### Temporary File Cleanup

The plugin automatically cleans up:
- State files after successful restoration
- Expired state files (older than `stateFileExpiry`)
- Trigger files after processing

**Manual cleanup**:
```bash
# Remove all restart state
rm -rf ~/.cache/claude-unleashed/process-restart/

# Plugin will recreate directory on next use
```

## Performance Impact

### Restart Timing

Typical restart timing breakdown:

```
Command execution:        < 0.1s
State file creation:      < 0.1s
Process spawn:             0.5s
Current process exit:      0.5-1s
New process startup:       2-3s
State restoration:         0.1-0.2s
MCP initialization:        1-2s
────────────────────────────────
Total:                     4-7s
```

**Comparison**:
```
Manual restart (exit + start):     ~3-5s
Plugin restart (with preservation): ~4-7s
Overhead:                          ~1-2s
```

The additional overhead comes from:
- State file I/O (write + read)
- Process spawn coordination
- State restoration logic
- MCP server reinitialization

### Resource Usage

**Disk space**:
- State file: < 1 KB
- Trigger file: 0 bytes (empty)
- Total cache usage: < 5 KB

**Memory**:
- No persistent memory usage
- Temporary buffers during save/restore: < 100 KB
- Cleared after restoration complete

**CPU**:
- JSON parsing: negligible
- State save/restore: < 10ms
- No ongoing background processing

### Optimization Tips

For faster restarts:

1. **Reduce MCP servers**: Fewer servers initialize faster
   ```json
   {
     "mcpServers": {
       // Only enable needed servers
     }
   }
   ```

2. **Clean session history**: Periodically use `--clean` restart
   ```bash
   # Every few days for performance
   /restart --clean
   ```

3. **Increase state expiry**: Reduce validation overhead
   ```json
   {
     "plugins": {
       "process-restart": {
         "stateFileExpiry": 600
       }
     }
   }
   ```

4. **Use SSD storage**: Faster state file I/O
   ```bash
   # Ensure cache is on fast storage
   df -h ~/.cache
   ```

## Development

### Testing the Plugin

#### Basic Functionality Test

```bash
# 1. Start Claude Code in a project directory
cd ~/my-project
claude

# 2. Have a brief conversation to create history
You: What files are in this directory?
Claude: [Lists files]

# 3. Run restart command
You: /restart

# 4. Verify restoration
# Should see:
# - "Session restored from restart" message
# - Same working directory
# - Conversation history accessible
```

#### State Preservation Test

```bash
# 1. Create a test project
mkdir -p /tmp/restart-test
cd /tmp/restart-test
git init
git checkout -b test-branch

# 2. Start Claude Code
claude --model claude-opus-4-5

# 3. Note session details
# - Session ID (visible in UI)
# - Working directory (/tmp/restart-test)
# - Model (opus)
# - Branch (test-branch)

# 4. Restart
/restart

# 5. Verify all state restored:
pwd  # Should be /tmp/restart-test
# Model should be opus
# Branch should be test-branch
```

#### Expiry Test

```bash
# 1. Configure short expiry
cat > .claude/settings.json <<EOF
{
  "plugins": {
    "process-restart": {
      "stateFileExpiry": 5
    }
  }
}
EOF

# 2. Manually create state file
mkdir -p ~/.cache/claude-unleashed/process-restart
cat > ~/.cache/claude-unleashed/process-restart/restart-state.json <<EOF
{
  "version": "1.0.0",
  "timestamp": $(date -d '10 seconds ago' +%s),
  "sessionId": "test123",
  "workingDir": "/tmp",
  "model": "claude-sonnet-4-5",
  "gitBranch": "",
  "enabledPlugins": []
}
EOF

# 3. Start Claude Code
claude

# 4. Should see expiry warning:
# "Restart state file found but expired (age: 10s, max: 5s)"
```

#### Clean Restart Test

```bash
# 1. Start session and note session ID
claude
# Note session ID from UI

# 2. Create conversation history
You: Remember this: test123
Claude: I'll remember test123

# 3. Clean restart
/restart --clean

# 4. Verify fresh session:
You: What did I ask you to remember?
Claude: [Should not remember - fresh session]
```

### Hook Development

The plugin uses two hooks: Stop and SessionStart.

**Testing Stop Hook**:
```bash
# Run hook manually
cd plugins/unleashed/process-restart

# Create trigger
touch ~/.cache/claude-unleashed/process-restart/restart-trigger

# Simulate hook input
echo '{"session_id": "test123"}' | \
  ./hooks-handlers/restart-handler.sh

# Check state file created
cat ~/.cache/claude-unleashed/process-restart/restart-state.json
```

**Testing SessionStart Hook**:
```bash
# Create test state file
cat > ~/.cache/claude-unleashed/process-restart/restart-state.json <<EOF
{
  "version": "1.0.0",
  "timestamp": $(date +%s),
  "sessionId": "test123",
  "workingDir": "/tmp",
  "model": "claude-sonnet-4-5",
  "gitBranch": "",
  "enabledPlugins": []
}
EOF

# Run hook
./hooks-handlers/session-restore.sh

# Should output restoration message JSON
# State file should be deleted
```

### Debugging

Enable debug output:

```bash
# Run with bash debug mode
bash -x ~/.claude/plugins/unleashed/process-restart/hooks-handlers/restart-handler.sh

# Check Claude Code logs
tail -f ~/.claude/logs/debug.log

# Monitor state file changes
watch -n 1 'ls -la ~/.cache/claude-unleashed/process-restart/'

# View state file contents
watch -n 1 'cat ~/.cache/claude-unleashed/process-restart/restart-state.json'
```

### Common Development Issues

**State file not created**:
```bash
# Check trigger file exists
ls -la ~/.cache/claude-unleashed/process-restart/restart-trigger

# Verify Stop hook is registered
cat plugins/unleashed/process-restart/hooks/hooks.json

# Check hook script permissions
ls -la plugins/unleashed/process-restart/hooks-handlers/
# All .sh files should be executable (755)
```

**Session not restoring**:
```bash
# Verify SessionStart hook runs
# Add debug output to session-restore.sh
echo "DEBUG: Hook running" >&2

# Check for JSON errors
jq . ~/.cache/claude-unleashed/process-restart/restart-state.json

# Verify expiry calculation
# Check timestamp vs current time
```

## Related Documentation

- [MCP Refresh Plugin](../mcp-refresh/README.md) - Detect MCP configuration changes
- [Plugin Development Guide](../../../docs/extensions/plugin-development.md) - Create custom plugins
- [MCP Integration Guide](../../../docs/extensions/snail-integration.md) - MCP server setup
- [Claude Code Documentation](https://github.com/anthropics/claude-code) - Official CLI docs

## Future Enhancements

Potential improvements for future versions:

### Advanced State Preservation
- Preserve active tool execution state
- Resume interrupted operations
- Save/restore terminal history
- Preserve file watchers and background tasks

### Enhanced Safety
- Detect uncommitted git changes before restart
- Warn about unsaved editor buffers
- Preview state before applying
- Rollback failed restorations

### Performance Optimization
- Incremental state updates (only changed data)
- Compressed state files
- Parallel MCP server initialization
- Faster process handoff

### Configuration Improvements
- Per-directory state preservation rules
- Custom state file locations
- Selective state preservation (choose what to save)
- State file encryption for sensitive projects

## License

Same as Claude Unleashed parent repository.

## Author

Heiervang Technologies

## Version History

- **1.0.0** (2026-01-01) - Initial release
  - Session ID preservation across restarts
  - Working directory restoration
  - Model and configuration preservation
  - Git branch context retention
  - Stop and SessionStart hook integration
  - State file expiry mechanism
  - Integration with mcp-refresh plugin
  - Confirmation prompts and safety features
  - Clean restart option (--clean flag)
  - Force restart option (--force flag)
