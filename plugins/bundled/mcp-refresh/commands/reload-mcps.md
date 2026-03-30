---
name: reload-mcps
description: Check MCP configuration changes and reload servers
argument-hint: "[server-name]"
---

# Reload MCP Server Configurations

Checks for changes in MCP configuration files and provides guidance on reloading servers.

## Usage

- `/reload-mcps` - Check all MCP configurations for changes
- `/reload-mcps <server-name>` - Check specific server configuration

## What This Command Does

1. Reads MCP configurations from:
   - `.mcp.json` (project-level)
   - `.claude.json` (user-level)
   - Plugin-specific `.mcp.json` files

2. Compares with last known configuration state

3. Reports:
   - New servers added
   - Servers removed
   - Configuration changes to existing servers

4. Provides instructions for applying changes

## Implementation Note

Due to Claude Code's architecture, MCP servers are initialized at startup and cannot be hot-reloaded during a session. This command:

- **Detects** configuration changes
- **Informs** you about what changed
- **Recommends** using `/restart` to apply changes while preserving your session

## Example Output

```
Checking MCP configurations...

Changes detected:
  - Added: new-database-server (type: stdio)
  - Modified: github-server (OAuth token updated)
  - Removed: old-api-server

To apply these changes, use the `/restart` command to restart Claude Code
while preserving your current session.
```

## Related Commands

- `/mcp-status` - View current MCP server status
- `/restart` - Restart Claude Code with session preservation (applies MCP changes)

## Automatic Detection

If the plugin setting `autoDetect` is enabled (default: true), this check runs automatically before each tool execution. You'll be notified if MCP configurations have changed.
