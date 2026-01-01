---
name: mcp-status
description: Display current MCP server status and configuration
---

# MCP Server Status

Displays the current status of all MCP servers and their configurations.

## Usage

- `/mcp-status` - Show status of all MCP servers
- `/mcp-status verbose` - Show detailed configuration for each server

## What This Command Shows

1. **Active Servers**
   - Server name
   - Type (stdio, sse, http, websocket)
   - Connection status
   - Configuration source

2. **Configuration Sources**
   - Project: `.mcp.json`
   - User: `~/.claude.json`
   - Plugins: `plugins/*/.mcp.json`

3. **Server Health**
   - Connected / Disconnected
   - Last activity
   - Error states (if any)

## Example Output

```
MCP Server Status
=================

Active Servers (3):

  github
    Type: sse
    Status: Connected
    Source: .mcp.json (project)
    Last activity: 2 minutes ago

  database
    Type: stdio
    Status: Connected
    Source: plugins/db-plugin/.mcp.json
    Last activity: 1 minute ago

  signal
    Type: sse
    Status: Connected
    Source: ~/.claude.json (user)
    Last activity: Just now

Configuration Files:
  - .mcp.json (2 servers)
  - ~/.claude.json (1 server)
  - plugins/db-plugin/.mcp.json (1 server)

Last configuration check: Just now
No pending changes detected.
```

## Related Commands

- `/reload-mcps` - Check for configuration changes
- `/restart` - Restart Claude Code to apply MCP changes
