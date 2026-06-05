# MCP Hot Reload Plugin

Automatically detect and manage MCP (Model Context Protocol) server configuration changes without restarting Claude Code.

## Overview

This plugin provides:
- Automatic detection of MCP configuration file changes
- Commands to check and report configuration changes
- Integration with the process-restart plugin for seamless MCP updates

## Features

### Automatic Change Detection

The plugin monitors MCP configuration files and notifies you when changes are detected:
- `.mcp.json` (project-level)
- `~/.claude.json` (user-level)
- `plugins/*/.mcp.json` (plugin-level)

When a change is detected, you'll receive a notification with options to:
1. View changes with `/reload-mcps`
2. Apply changes with `/restart` (preserves your session)

### Commands

#### `/reload-mcps [server-name]`

Check for MCP configuration changes and see what has changed.

**Usage:**
```
/reload-mcps              # Check all servers
/reload-mcps github       # Check specific server
```

**Example output:**
```
Checking MCP configurations...

Changes detected:
  - Added: new-database-server (type: stdio)
  - Modified: github-server (OAuth token updated)
  - Removed: old-api-server

To apply these changes, use the `/restart` command.
```

#### `/mcp-status [verbose]`

Display current MCP server status and configuration.

**Usage:**
```
/mcp-status               # Show server status
/mcp-status verbose       # Show detailed configuration
```

## Installation

Bundled with unleash. Enabled by default — the wrapper picks it up from `plugins/bundled/mcp-refresh/` on launch.

## Configuration

The plugin has no per-instance settings — the watched paths are hardcoded (see [How It Works](#how-it-works)) and detection runs on every `PreToolUse` event whenever the plugin is enabled.

**To disable:** add an `enabled_plugins` allowlist to `~/.config/unleash/config.toml` that excludes `mcp-refresh`. See [docs/extensions/configuration.md](../../../docs/extensions/configuration.md) — the empty default (`enabled_plugins = []`) means "all bundled plugins enabled"; switching to a non-empty list makes it an explicit allowlist.

```toml
enabled_plugins = ["process-restart", "auto-mode"]   # mcp-refresh omitted = disabled
```

You can also toggle plugins from the **Plugins** tab in the unleash TUI without editing the TOML by hand.

## How It Works

### Architecture

```
┌─────────────────────────────────────┐
│  MCP Configuration Files            │
│  - .mcp.json                        │
│  - .claude.json                     │
│  - plugins/*/.mcp.json              │
└──────────┬──────────────────────────┘
           │
           ↓ (PreToolUse Hook)
┌─────────────────────────────────────┐
│  Change Detection                   │
│  - Compute SHA256 hash              │
│  - Compare with cached hash         │
│  - Detect: added/modified/removed   │
└──────────┬──────────────────────────┘
           │
           ↓ (If changes detected)
┌─────────────────────────────────────┐
│  User Notification                  │
│  - Prompt with change summary       │
│  - Suggest /reload-mcps or /restart │
└─────────────────────────────────────┘
```

### Detection Method

The plugin uses SHA256 hashing to efficiently detect configuration changes:

1. On first run, computes and caches configuration hash
2. Before each tool use, recomputes hash
3. Compares with cached hash
4. If different, notifies user and updates cache

### Cache Location

Configuration hashes are stored in:
```
~/.cache/unleash/mcp-refresh/config-hashes.txt
```

## Limitations

### Why Not True Hot-Reload?

Claude Code's MCP servers are initialized at session startup and deeply integrated with the runtime. Without access to Claude Code's internal source code, we cannot:

- Stop and restart individual MCP servers
- Reload configuration without process restart
- Modify the MCP manager lifecycle

### Current Approach

Instead, this plugin provides:
- **Detection**: Know immediately when configs change
- **Reporting**: See exactly what changed
- **Guidance**: Clear path to apply changes via `/restart`

This approach:
- Works within Claude Code's plugin constraints
- Doesn't require core modifications
- Maintains stability and reliability
- Integrates seamlessly with session preservation

## Integration with Process Restart

This plugin is designed to work with the `process-restart` plugin:

1. MCP config changes detected → User notified
2. User reviews changes with `/reload-mcps`
3. User runs `/restart` to apply changes
4. Process restarts, preserving session
5. New MCP configuration loaded automatically

See the [process-restart plugin](../process-restart/README.md) for details.

## Troubleshooting

### Changes not detected

**Problem**: Configuration file changed but no notification

**Solutions**:
1. Verify the file is one of the watched paths (`.mcp.json` at project root, `~/.claude.json`, or `plugins/*/.mcp.json`). Other paths aren't monitored — extending the watch list requires a code change to `hooks-handlers/check-mcp-changes.sh`.
2. Confirm the plugin is enabled (`enabled_plugins` either empty or includes `mcp-refresh` in `~/.config/unleash/config.toml`, or check the Plugins tab in the TUI).
3. Clear cache: `rm -rf ~/.cache/unleash/mcp-refresh/`
4. Manually run `/reload-mcps` to check.

### False positives

**Problem**: Notified about changes when none were made

**Solutions**:
1. Check for automatic file formatting (e.g. JSON prettier rewriting whitespace).
2. Verify no other process is modifying config files.
3. Clear cache and let it rebuild: `rm -rf ~/.cache/unleash/mcp-refresh/`

### Automatic detection too frequent

**Problem**: Notifications appearing too often

**Solutions**:
1. Disable the plugin entirely via `enabled_plugins` allowlist (see [Configuration](#configuration)) or the TUI Plugins tab — there is no granular auto-detect-off toggle today.
2. Use manual checking with `/reload-mcps` after disabling.
3. Move frequently-changing configs to a path outside the watched set.

## Development

### Testing the Plugin

1. Make a change to `.mcp.json`:
   ```bash
   # Add a new server
   echo '{"test-server": {"command": "echo", "args": ["test"]}}' > .mcp.json
   ```

2. Run any Claude Code command
3. You should receive a notification about the change

4. Run `/reload-mcps` to see details

### Hook Execution Flow

```bash
# Hook is called before each tool use
PreToolUse → check-mcp-changes.sh → compute_config_hash()
                                   ↓
                            compare with cache
                                   ↓
                          if changed: notify user
                                   ↓
                            update cache hash
```

## Future Enhancements

If Claude Code's source becomes available or if core APIs are exposed:

- True hot-reload of individual MCP servers
- Automatic reload without user intervention
- Per-server reload (modify one, reload one)
- OAuth token refresh without restart
- Server health monitoring and auto-restart

## Related Documentation

- [MCP Refresh & Process Restart Guide](../../../docs/extensions/restart-refresh.md) - Comprehensive guide for both plugins
- [Process Restart Plugin](../process-restart/README.md)

- [Plugin Development Guide](../../../docs/internal/claude-code/plugin-development.md)

## License

Same as unleash parent repository.

## Author

Heiervang Technologies

## Version History

- **1.0.0** (2026-01-01) - Initial release
  - Automatic change detection via PreToolUse hook
  - `/reload-mcps` command
  - `/mcp-status` command
  - SHA256-based change detection
  - Integration with process-restart plugin
