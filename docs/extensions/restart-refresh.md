# MCP Refresh & Process Restart Guide

Comprehensive guide to the MCP hot-reload and process restart functionality in unleash.

## Table of Contents

1. [Overview](#overview)
2. [The Problem](#the-problem)
3. [The Solution](#the-solution)
4. [Architecture](#architecture)
5. [Installation & Setup](#installation--setup)
6. [Usage Workflows](#usage-workflows)
7. [MCP Refresh Plugin](#mcp-refresh-plugin)
8. [Process Restart Plugin](#process-restart-plugin)
9. [Integration](#integration)
10. [Limitations](#limitations)
11. [Troubleshooting](#troubleshooting)
12. [Future Enhancements](#future-enhancements)
13. [Technical Reference](#technical-reference)

## Overview

The MCP Refresh and Process Restart plugins work together to provide a seamless experience for managing MCP (Model Context Protocol) server configurations and restarting Claude Code without losing your work.

### What These Plugins Do

**MCP Refresh Plugin** (`mcp-refresh`):
- Automatically detects changes to MCP configuration files
- Notifies you when MCP servers are added, modified, or removed
- Provides detailed change reporting via `/reload-mcps` command
- Shows current MCP server status via `/mcp-status` command

**Process Restart Plugin** (`process-restart`):
- Restarts Claude Code while preserving your session
- Maintains conversation history and working context
- Applies MCP configuration changes during restart
- Provides safe, confirmed restart experience

### Why They Were Created

Claude Code initializes MCP servers at startup and deeply integrates them into the runtime. Without these plugins:
- You must manually exit and restart Claude Code to apply MCP changes
- You lose your conversation history and context
- You have to manually navigate back to your working directory
- There's no visibility into what MCP configuration changes occurred

These plugins solve these problems with a plugin-only approach that works within Claude Code's architectural constraints.

## The Problem

### MCP Hot-Reload Challenges

Claude Code has several architectural characteristics that make hot-reloading MCP servers challenging:

1. **Startup Initialization**: MCP servers are initialized when Claude Code starts
2. **Deep Integration**: MCP connections are woven throughout the runtime
3. **Closed Source**: Claude Code's internal server management is not accessible
4. **No Public API**: No exposed methods to start/stop individual MCP servers
5. **Plugin Constraints**: Plugins can't modify core runtime behavior

### Manual Restart Pain Points

Without these plugins, applying MCP configuration changes requires:

```bash
# Old workflow
1. Edit .mcp.json
2. Remember your session ID
3. Note your working directory
4. Exit Claude Code
5. Manually restart with: claude --cwd /path/to/project --resume session-id
6. Hope you remembered everything correctly
```

This is:
- Error-prone (easy to forget session ID or directory)
- Time-consuming (manual steps, no automation)
- Context-losing (lose track of what you were doing)
- Frustrating (interrupts flow state)

## The Solution

### Plugin-Only Approach

Due to Claude Code being closed-source, we cannot:
- ❌ Modify core MCP initialization code
- ❌ Hook into MCP server lifecycle
- ❌ Hot-reload individual servers in-process
- ❌ Access internal MCP connection management

Instead, we use a **plugin-only approach** that works within the constraints:
- ✅ Detect configuration changes via file hashing
- ✅ Notify users when changes occur
- ✅ Save and restore session state across restarts
- ✅ Provide seamless restart experience
- ✅ Integrate MCP reloading with session preservation

### Two-Plugin Design

The functionality is split into two focused plugins:

**1. Detection & Reporting (mcp-refresh)**
- Uses PreToolUse hooks to monitor config files
- Computes SHA256 hashes to detect changes
- Provides commands to view changes
- Recommends when to restart

**2. Restart & Restoration (process-restart)**
- Uses Stop and SessionStart hooks
- Saves session state before exit
- Spawns new process with preserved state
- Restores context after restart
- Cleans up temporary files

This separation of concerns makes each plugin:
- Focused on a single responsibility
- Independently testable
- Easier to maintain
- Reusable in other contexts

## Architecture

### High-Level Overview

```
┌────────────────────────────────────────────────────────────────┐
│                     User Interaction                           │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  Edit MCP Configuration                                        │
│  ├─ .mcp.json                                                  │
│  ├─ .claude.json                                               │
│  └─ plugins/*/.mcp.json                                        │
│                                                                │
└──────────┬─────────────────────────────────────────────────────┘
           │
           ↓
┌────────────────────────────────────────────────────────────────┐
│              MCP Refresh Plugin (Detection)                    │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  PreToolUse Hook:                                              │
│  ├─ Compute configuration hash (SHA256)                        │
│  ├─ Compare with cached hash                                   │
│  └─ Notify if changed                                          │
│                                                                │
│  Commands:                                                     │
│  ├─ /reload-mcps - Show detailed changes                       │
│  └─ /mcp-status - Show current server status                   │
│                                                                │
└──────────┬─────────────────────────────────────────────────────┘
           │
           │ User decides to apply changes
           ↓
┌────────────────────────────────────────────────────────────────┐
│           Process Restart Plugin (Application)                 │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  /restart Command:                                             │
│  ├─ Create restart trigger file                                │
│  ├─ Initiate Claude Code exit                                  │
│  └─ Wait for Stop hook                                         │
│                                                                │
│  Stop Hook:                                                    │
│  ├─ Detect restart trigger                                     │
│  ├─ Save session state (session ID, working dir, etc.)         │
│  ├─ Spawn new Claude Code process                              │
│  └─ Allow current process to exit                              │
│                                                                │
│  SessionStart Hook (New Process):                              │
│  ├─ Check for state file                                       │
│  ├─ Validate file age (not expired)                            │
│  ├─ Restore session context                                    │
│  ├─ Apply working directory                                    │
│  └─ Clean up state file                                        │
│                                                                │
└──────────┬─────────────────────────────────────────────────────┘
           │
           ↓
┌────────────────────────────────────────────────────────────────┐
│                  Claude Code Runtime                           │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  ├─ Resume session with preserved ID                           │
│  ├─ Return to working directory                                │
│  ├─ Initialize MCP servers with NEW configuration              │
│  ├─ Restore conversation history                               │
│  └─ Continue where user left off                               │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

### Component Interaction

```
Time ──────────────────────────────────────────────────────────►

┌──────────┐  PreToolUse   ┌──────────┐
│   User   │ ────────────► │   MCP    │
│  Action  │               │ Refresh  │
└──────────┘               └────┬─────┘
                                │
                                ↓ Notification
┌──────────┐               ┌─────────┐
│   User   │  /reload-mcps │   MCP   │
│ Reviews  │ ◄────────────┤ Refresh │
│ Changes  │               └─────────┘
└──────────┘
     │
     │ Decides to restart
     ↓
┌──────────┐  /restart     ┌──────────┐
│   User   │ ────────────► │ Process  │
│ Confirms │               │ Restart  │
└──────────┘               └────┬─────┘
                                │
                                │ Stop Hook
                                ↓
                           ┌─────────┐
                           │  Save   │
                           │  State  │
                           └────┬────┘
                                │
                                │ Spawn New Process
                                ↓
                           ┌─────────┐
                           │   New   │
                           │ Claude  │
                           │  Code   │
                           └────┬────┘
                                │
                                │ SessionStart Hook
                                ↓
                           ┌─────────┐
                           │ Restore │
                           │  State  │
                           └────┬────┘
                                │
                                ↓
┌──────────────────────────────────────┐
│  Session Resumed                     │
│  - Same conversation history         │
│  - Same working directory            │
│  - NEW MCP configuration applied     │
└──────────────────────────────────────┘
```

## Installation & Setup

### Prerequisites

- Claude Code CLI installed and working
- unleash repository cloned
- Bash shell (for hook scripts)
- Basic tools: `sha256sum`, `jq`, `nohup`

### Enable the Plugins

1. **Edit Configuration**:

```bash
vim <REPO_ROOT>/.claude/settings.json
```

2. **Add Plugins to Enabled List**:

```json
{
  "plugins": {
    "enabled": [
      "mcp-refresh",
      "process-restart"
    ]
  }
}
```

3. **Configure Plugin Settings** (Optional):

```json
{
  "plugins": {
    "enabled": ["mcp-refresh", "process-restart"],
    "mcp-refresh": {
      "autoDetect": true,
      "configPaths": [
        ".mcp.json",
        ".claude.json",
        "~/.claude.json"
      ]
    },
    "process-restart": {
      "preserveSession": true,
      "preserveWorkingDir": true,
      "preservePermissions": true,
      "stateFileExpiry": 300
    }
  }
}
```

### Verify Installation

1. **Start Claude Code**:

```bash
cd /home/me/unleash
claude
```

2. **Check Plugin Loading**:

```bash
# In Claude Code session
/help

# Should see:
# - /reload-mcps command
# - /mcp-status command
# - /restart command
```

3. **Test Detection**:

```bash
# Edit MCP config
echo '{"test": {"command": "echo", "args": ["test"]}}' > .mcp.json

# Run any command - should see notification
You: List files
Claude: [Response]

# You should see:
# "MCP configuration files have changed..."
```

4. **Test Restart**:

```bash
# In Claude session
/restart

# Should see:
# - Confirmation prompt
# - State preservation details
# - Process restart
# - Session restored message
```

## Usage Workflows

### Workflow 1: Add New MCP Server

**Scenario**: You want to add a new database MCP server to your project.

**Steps**:

```bash
# 1. Edit MCP configuration
vim .mcp.json
```

```json
{
  "mcpServers": {
    "database": {
      "command": "npx",
      "args": ["-y", "database-mcp-server"],
      "env": {
        "DATABASE_URL": "postgresql://localhost/mydb"
      }
    }
  }
}
```

```bash
# 2. Continue working - automatic notification appears
You: Can you help with this code?
Claude: [Response]

# Notification:
# "MCP configuration files have changed since session start.
#  Use `/reload-mcps` to see what changed, or `/restart` to apply changes."

# 3. Review changes
You: /reload-mcps

Claude:
Checking MCP configurations...

Changes detected:
  - Added: database (type: stdio)
    Command: npx -y database-mcp-server
    Environment: DATABASE_URL=postgresql://localhost/mydb

To apply these changes, use the `/restart` command to restart Claude Code
while preserving your current session.

# 4. Apply changes
You: /restart

⚠️  This will restart the Claude Code process.
   Your session will be preserved and automatically resumed.

   Preserve:
   - Session ID: a8ea16a
   - Working directory: <PROJECT_ROOT>
   - Model: claude-sonnet-4-5
   - Git branch: feature/add-database

Proceed with restart? (y/n): y

✅ Restart initiated. New Claude Code process started.
   Session will be restored automatically.

[Process restarts...]

🔄 Session restored from restart

Restored state:
- Session ID: a8ea16a
- Working directory: <PROJECT_ROOT>
- Model: claude-sonnet-4-5
- Git branch: feature/add-database

MCP servers reloaded with current configuration.

# 5. Verify new server is available
You: Can you query the database using the new MCP server?

Claude: Yes, I can now access the database server. What would you like to query?
```

### Workflow 2: Update OAuth Token

**Scenario**: Your GitHub MCP server OAuth token expired and you need to update it.

**Steps**:

```bash
# 1. Update token in configuration
vim .claude.json
```

```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_TOKEN": "ghp_new_token_here"
      }
    }
  }
}
```

```bash
# 2. Check what changed (optional)
You: /reload-mcps github

Checking MCP configuration for server: github

Changes detected:
  - Modified: github (type: stdio)
    Changed: Environment variable GITHUB_TOKEN updated

To apply these changes, use `/restart` to reload MCP servers.

# 3. Apply with quick restart
You: /restart --force

🔄 Restart triggered (forced)
   Session will be preserved and restored automatically

[Process restarts...]

🔄 Session restored from restart
   MCP servers reloaded with current configuration.

# GitHub server now uses new token
```

### Workflow 3: Remove Unused MCP Server

**Scenario**: You no longer need the Slack MCP server and want to remove it.

**Steps**:

```bash
# 1. Edit config to remove server
vim .mcp.json
```

```json
{
  "mcpServers": {
    "database": {
      "command": "npx",
      "args": ["-y", "database-mcp-server"]
    }
    // Removed: "slack" server
  }
}
```

```bash
# 2. Notification appears automatically
# "MCP configuration files have changed..."

# 3. Check what was removed
You: /reload-mcps

Changes detected:
  - Removed: slack

To apply these changes, use `/restart`.

# 4. Restart to apply
You: /restart

[Confirm and restart...]

# Slack server no longer initialized
```

### Workflow 4: Fresh Start After Issues

**Scenario**: A plugin is misbehaving and you need a clean restart.

**Steps**:

```bash
# 1. Clean restart (no state preservation)
You: /restart --clean

⚠️  This will restart with a CLEAN session.
   Your conversation history will NOT be preserved.

   This is useful for:
   - Recovering from plugin issues
   - Starting fresh
   - Clearing cached state

Proceed? (y/n): y

[Process restarts with fresh session...]

# 2. Verify fresh start
# - New session ID
# - No conversation history
# - Default working directory
# - Fresh MCP server initialization
```

### Workflow 5: Multiple Configuration Changes

**Scenario**: You're setting up a new project with multiple MCP servers.

**Steps**:

```bash
# 1. Create project MCP configuration
cat > .mcp.json <<'EOF'
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "<PROJECT_ROOT>"]
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_TOKEN": "${GITHUB_TOKEN}"
      }
    },
    "database": {
      "command": "npx",
      "args": ["-y", "database-mcp-server"],
      "env": {
        "DATABASE_URL": "${DATABASE_URL}"
      }
    }
  }
}
EOF

# 2. Review all changes
You: /reload-mcps

Checking MCP configurations...

Changes detected:
  - Added: filesystem (type: stdio)
  - Added: github (type: stdio)
  - Added: database (type: stdio)

3 servers will be added when you restart.

# 3. Check current status before restarting
You: /mcp-status verbose

Current MCP Servers:
  (No servers currently active)

Configuration files:
  - .mcp.json (3 servers defined)
  - .claude.json (not found)

# 4. Restart to load all servers
You: /restart

[Restart with confirmation...]

# 5. Verify all servers loaded
You: /mcp-status

Current MCP Servers:
  ✓ filesystem (connected)
  ✓ github (connected)
  ✓ database (connected)

3 servers active
```

## MCP Refresh Plugin

### Overview

The `mcp-refresh` plugin provides automatic detection and reporting of MCP configuration changes.

**Plugin Location**: `<REPO_ROOT>/plugins/bundled/mcp-refresh/`

**Components**:
- PreToolUse hook for automatic detection
- `/reload-mcps` command for detailed change reporting
- `/mcp-status` command for server status
- SHA256-based change detection
- Configurable monitoring paths

### Features

#### 1. Automatic Change Detection

The plugin uses a PreToolUse hook that runs before each tool execution:

```bash
# Hook: hooks-handlers/check-mcp-changes.sh
PreToolUse → Compute config hash → Compare with cache → Notify if changed
```

**Detection Method**:
- Computes SHA256 hash of all MCP configuration files
- Caches hash at `~/.cache/unleash/mcp-refresh/config-hashes.txt`
- Compares current hash with cached hash before each tool use
- Notifies user if hash differs

**Monitored Files** (default):
- `.mcp.json` (project-level)
- `.claude.json` (user-level)
- `~/.claude.json` (global user-level)
- `plugins/*/.mcp.json` (plugin-level)

#### 2. Change Reporting (`/reload-mcps`)

View detailed information about what changed:

```bash
# Check all servers
/reload-mcps

# Check specific server
/reload-mcps github
```

**Output Example**:
```
Checking MCP configurations...

Changes detected:
  - Added: new-server (type: stdio)
    Command: npx -y new-server
    Environment: API_KEY=***

  - Modified: existing-server (type: stdio)
    Changed: Environment variable updated

  - Removed: old-server

To apply these changes, use the `/restart` command.
```

#### 3. Server Status (`/mcp-status`)

View current MCP server status:

```bash
# Show basic status
/mcp-status

# Show detailed configuration
/mcp-status verbose
```

**Output Example**:
```
Current MCP Servers:
  ✓ github (connected)
  ✓ database (connected)
  ✗ slack (error: authentication failed)

3 servers configured, 2 active

Configuration sources:
  - .mcp.json (2 servers)
  - .claude.json (1 server)
```

### Configuration

Configure in `.claude/settings.json`:

```json
{
  "plugins": {
    "mcp-refresh": {
      "autoDetect": true,
      "configPaths": [
        ".mcp.json",
        ".claude.json",
        "~/.claude.json"
      ]
    }
  }
}
```

**Settings**:

- **`autoDetect`** (boolean, default: `true`)
  - Enable automatic change detection via PreToolUse hook
  - Disable if you prefer manual checking only
  - Reduces notification frequency

- **`configPaths`** (array, default: `[".mcp.json", ".claude.json", "~/.claude.json"]`)
  - Paths to monitor for changes
  - Relative paths resolved from working directory
  - Add custom paths as needed

### How It Works

#### Hash-Based Detection

The plugin uses SHA256 hashing for efficient change detection:

```bash
# Pseudocode of detection algorithm
function compute_config_hash():
  hash = ""

  for each config_file in monitored_files:
    if file_exists(config_file):
      hash += sha256(file_contents)

  return sha256(hash)

function check_for_changes():
  current_hash = compute_config_hash()
  cached_hash = read_from_cache()

  if current_hash != cached_hash:
    notify_user()
    update_cache(current_hash)
```

**Why SHA256?**
- Fast computation (< 10ms for typical configs)
- Cryptographically secure (no collisions)
- Content-based (whitespace changes don't matter)
- Deterministic (same content = same hash)

#### Cache Location

```
~/.cache/unleash/mcp-refresh/
├── config-hashes.txt    # SHA256 hash of current config
└── last-check.txt       # Timestamp of last check (optional)
```

**Cache Management**:
- Automatically created on first run
- Updated when changes detected
- Cleared on plugin disable
- Expires never (hash-based, not time-based)

**Manual Cache Reset**:
```bash
# Force fresh detection
rm -rf ~/.cache/unleash/mcp-refresh/
```

### Commands Reference

#### `/reload-mcps [server-name]`

**Purpose**: Check for MCP configuration changes and report details.

**Usage**:
```bash
/reload-mcps              # Check all servers
/reload-mcps github       # Check specific server
```

**Options**:
- `server-name` (optional): Check only specified server

**Output**:
- List of added servers
- List of modified servers (with details)
- List of removed servers
- Instructions for applying changes

**Markdown**: See [commands/reload-mcps.md](<REPO_ROOT>/plugins/bundled/mcp-refresh/commands/reload-mcps.md)

#### `/mcp-status [verbose]`

**Purpose**: Display current MCP server status and configuration.

**Usage**:
```bash
/mcp-status               # Show basic status
/mcp-status verbose       # Show detailed configuration
```

**Options**:
- `verbose` (optional): Show detailed server configuration

**Output**:
- Server names and connection status
- Active vs. configured count
- Configuration file sources
- Error messages (if any)

**Markdown**: See [commands/mcp-status.md](<REPO_ROOT>/plugins/bundled/mcp-refresh/commands/mcp-status.md)

### Implementation Details

#### PreToolUse Hook

**File**: `hooks-handlers/check-mcp-changes.sh`

**Hook Configuration** (`hooks/hooks.json`):
```json
{
  "PreToolUse": {
    "script": "./hooks-handlers/check-mcp-changes.sh",
    "description": "Automatically detect MCP configuration changes",
    "outputFormat": "prompt"
  }
}
```

**Execution Flow**:
1. Claude Code about to execute a tool
2. PreToolUse hook triggered
3. Script computes current config hash
4. Compares with cached hash
5. If different, outputs notification prompt
6. Claude Code displays notification to user
7. Tool execution proceeds normally

**Output Format** (JSON):
```json
{
  "type": "prompt",
  "content": "MCP configuration files have changed..."
}
```

#### Configuration Parsing

The plugin reads MCP configurations from JSON files:

```bash
# Example configuration parsing
function read_mcp_config():
  configs = {}

  # Project-level
  if exists(".mcp.json"):
    configs.merge(parse_json(".mcp.json"))

  # User-level
  if exists("~/.claude.json"):
    configs.merge(parse_json("~/.claude.json"))

  # Plugin-level
  for plugin_config in glob("plugins/*/.mcp.json"):
    configs.merge(parse_json(plugin_config))

  return configs
```

### Detailed README

For complete plugin documentation, see:
[<REPO_ROOT>/plugins/bundled/mcp-refresh/README.md](<REPO_ROOT>/plugins/bundled/mcp-refresh/README.md)

## Process Restart Plugin

### Overview

The `process-restart` plugin enables restarting Claude Code while preserving your session state and conversation history.

**Plugin Location**: `<REPO_ROOT>/plugins/bundled/process-restart/`

**Components**:
- `/restart` command to trigger restart
- Stop hook to save state and spawn new process
- SessionStart hook to restore state
- State file management
- Safety features (confirmation, expiry)

### Features

#### 1. Session Preservation

Maintains continuity across process restarts:

**Preserved State**:
- Session ID (conversation history access)
- Working directory
- Model selection
- Git branch context
- Permission mode
- Enabled plugins
- Plugin settings

**Example**:
```bash
# Before restart:
Session ID: a8ea16a
Working Dir: <PROJECT_ROOT>
Model: claude-sonnet-4-5
Branch: feature/add-auth

# After restart:
Session ID: a8ea16a        # SAME - can access history
Working Dir: <PROJECT_ROOT>  # SAME - back in project
Model: claude-sonnet-4-5          # SAME - same model
Branch: feature/add-auth          # SAME - same branch
```

#### 2. MCP Server Reloading

New MCP configurations applied automatically during restart:

```bash
# Edit config
vim .mcp.json  # Add new server

# Restart
/restart

# New process starts with:
# - Old session ID (preserve history)
# - New MCP configuration (apply changes)
```

This is the recommended way to apply MCP configuration changes.

#### 3. Safety Features

**Confirmation Prompts**:
```bash
You: /restart

⚠️  This will restart the Claude Code process.
   Your session will be preserved and automatically resumed.

   Preserve:
   - Session ID: a8ea16a
   - Working directory: <PROJECT_ROOT>
   - Model: claude-sonnet-4-5

Proceed with restart? (y/n):
```

**State File Expiry**:
- State files expire after 5 minutes (default)
- Prevents restoring very old state
- Configurable via `stateFileExpiry` setting

**Active Tool Detection** (planned):
- Warns if tools are currently executing
- Prevents interrupting long-running operations

### Configuration

Configure in `.claude/settings.json`:

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

**Settings**:

- **`preserveSession`** (boolean, default: `true`)
  - Preserve session ID and conversation history
  - Disable for always-fresh sessions

- **`preserveWorkingDir`** (boolean, default: `true`)
  - Restore working directory after restart
  - Disable to start in default directory

- **`preservePermissions`** (boolean, default: `true`)
  - Restore permission mode (auto-allow, manual, etc.)
  - Disable to reset to default permissions

- **`stateFileExpiry`** (number, default: `300`)
  - State file expiry time in seconds
  - Increase for slower systems
  - Decrease for stricter freshness

### How It Works

#### Restart Flow

```
1. User runs /restart command
         ↓
2. trigger-restart.sh creates trigger file
         ↓
3. Claude Code initiates exit
         ↓
4. Stop hook detects trigger file
         ↓
5. Save current state to JSON file
         ↓
6. Spawn new Claude Code process
         ↓
7. Current process exits gracefully
         ↓
8. New process starts
         ↓
9. SessionStart hook runs
         ↓
10. Restore state from JSON file
         ↓
11. Resume session
         ↓
12. Clean up state file
```

#### State File Format

**Location**: `~/.cache/unleash/process-restart/restart-state.json`

**Format**:
```json
{
  "version": "1.0.0",
  "timestamp": 1735689600,
  "sessionId": "a8ea16a",
  "workingDir": "<PROJECT_ROOT>",
  "model": "claude-sonnet-4-5",
  "gitBranch": "feature/my-feature",
  "enabledPlugins": ["mcp-refresh", "process-restart"]
}
```

**Fields**:
- `version`: State file format version
- `timestamp`: Unix timestamp (for expiry check)
- `sessionId`: Claude session identifier
- `workingDir`: Absolute path to working directory
- `model`: Model identifier
- `gitBranch`: Git branch name (empty if not in repo)
- `enabledPlugins`: Array of plugin names

**Security**:
- File permissions: 600 (owner read/write only)
- Contains sensitive session ID
- Auto-deleted after restoration
- Expires after 5 minutes (default)

#### Hook Integration

**Stop Hook** (`hooks-handlers/restart-handler.sh`):

```bash
# Triggered when Claude Code exits
function on_stop():
  if exists(restart_trigger_file):
    # This is a restart, not a normal exit
    state = {
      sessionId: current_session_id,
      workingDir: current_working_dir,
      model: current_model,
      ...
    }

    write_json(state_file, state)
    spawn_process("claude --resume " + state.sessionId)
    allow_exit()
```

**SessionStart Hook** (`hooks-handlers/session-restore.sh`):

```bash
# Triggered when new Claude Code session starts
function on_session_start():
  if exists(state_file):
    state = read_json(state_file)

    if is_expired(state.timestamp):
      warn("State file expired")
      delete(state_file)
      return

    # Apply state
    set_working_dir(state.workingDir)
    notify_user("Session restored")

    delete(state_file)
```

### Commands Reference

#### `/restart [--force] [--clean]`

**Purpose**: Restart Claude Code while preserving session.

**Usage**:
```bash
/restart              # Standard restart with confirmation
/restart --force      # Skip confirmation prompt
/restart --clean      # Restart without preserving state
```

**Options**:
- `--force`: Skip confirmation prompt
- `--clean`: Don't preserve state (fresh session)

**Behavior**:
1. Show confirmation prompt (unless `--force`)
2. Create restart trigger file
3. Save state (unless `--clean`)
4. Initiate Claude Code exit
5. Spawn new process
6. Restore state in new process

**Examples**:

```bash
# Standard restart with confirmation
/restart

# Quick restart (no confirmation)
/restart --force

# Fresh start (no state preservation)
/restart --clean

# Force clean restart
/restart --force --clean
```

**Markdown**: See [commands/restart.md](<REPO_ROOT>/plugins/bundled/process-restart/commands/restart.md)

### Implementation Details

#### Trigger File

**Location**: `~/.cache/unleash/process-restart/restart-trigger`

**Purpose**: Signal to Stop hook that this is a restart, not a normal exit.

**Creation**:
```bash
# trigger-restart.sh
touch ~/.cache/unleash/process-restart/restart-trigger
```

**Detection**:
```bash
# restart-handler.sh (Stop hook)
if [[ -f "$TRIGGER_FILE" ]]; then
  # This is a restart - save state and spawn new process
  ...
fi
```

**Cleanup**:
```bash
# After processing
rm ~/.cache/unleash/process-restart/restart-trigger
```

#### Process Spawning

The plugin uses `nohup` to spawn the new process:

```bash
nohup claude \
  --cwd "/working/dir" \
  --model "claude-sonnet-4-5" \
  --resume "session-id" \
  > /dev/null 2>&1 &
```

**Why nohup?**
- Detaches from parent process
- Prevents SIGHUP signal propagation
- Allows parent to exit cleanly
- Redirects output to prevent blocking

**Process Lifecycle**:
1. Parent (current Claude) spawns child
2. Child starts in background
3. Parent waits briefly (0.5s)
4. Parent exits
5. Child continues independently
6. Child becomes session leader

#### State Preservation Logic

```bash
# Save state (Stop hook)
function save_state():
  state = {
    sessionId: get_session_id(),
    workingDir: get_cwd(),
    model: get_model(),
    gitBranch: get_git_branch(),
    ...
  }

  write_json(state_file, state)
  chmod(state_file, 0600)  # Owner read/write only

# Restore state (SessionStart hook)
function restore_state():
  state = read_json(state_file)

  # Validate
  if !is_valid(state):
    return error

  # Apply
  cd(state.workingDir)
  notify("Session restored: " + state.sessionId)

  # Cleanup
  delete(state_file)
```

### Detailed README

For complete plugin documentation, see:
[<REPO_ROOT>/plugins/bundled/process-restart/README.md](<REPO_ROOT>/plugins/bundled/process-restart/README.md)

## Integration

### How the Plugins Work Together

The two plugins form a complete MCP management workflow:

```
┌─────────────────────────────────────────────────────────────┐
│                  Complete Workflow                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  1. User edits MCP configuration                            │
│     ↓                                                       │
│  2. MCP Refresh detects change (PreToolUse hook)            │
│     ↓                                                       │
│  3. User notified: "MCP config changed"                     │
│     ↓                                                       │
│  4. User reviews with /reload-mcps                          │
│     ↓                                                       │
│  5. User decides to apply changes                           │
│     ↓                                                       │
│  6. User runs /restart                                      │
│     ↓                                                       │
│  7. Process Restart saves session state                     │
│     ↓                                                       │
│  8. Process Restart spawns new Claude Code                  │
│     ↓                                                       │
│  9. New process loads NEW MCP configuration                 │
│     ↓                                                       │
│  10. Process Restart restores session state                 │
│     ↓                                                       │
│  11. User continues with new MCP servers                    │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

```
Configuration Files          MCP Refresh           Process Restart
─────────────────           ────────────           ───────────────

.mcp.json              ──► Hash Detection
.claude.json                    │
plugins/*/.mcp.json             │
                                ↓
                           Notification
                                │
                                ↓
                        /reload-mcps ──► Report Changes
                                │
                                ↓
User Decision ──────────────────┘
                                │
                                ↓
                           /restart ────────────► Save State
                                                       │
                                                       ↓
                                                  Spawn Process
                                                       │
                                                       ↓
Claude Code Runtime ◄───────────────────────── Restore State
(New MCP Config)
```

### Example: Complete End-to-End Flow

```bash
# Starting state
$ claude
Session ID: abc123
Working Dir: /home/me/project
MCP Servers: github, filesystem

─────────────────────────────────────────────

# 1. Add new MCP server
$ vim .mcp.json
{
  "mcpServers": {
    "github": {...},
    "filesystem": {...},
    "database": {              ← NEW
      "command": "npx",
      "args": ["-y", "db-server"]
    }
  }
}

─────────────────────────────────────────────

# 2. Continue working in Claude
You: Can you help with this function?
Claude: [Response]

# MCP Refresh (automatic notification):
⚠️ MCP configuration files have changed since session start.
   Use `/reload-mcps` to see what changed, or `/restart` to apply.

─────────────────────────────────────────────

# 3. Review changes
You: /reload-mcps

# MCP Refresh response:
Checking MCP configurations...

Changes detected:
  - Added: database (type: stdio)
    Command: npx -y db-server

To apply these changes, use the `/restart` command.

─────────────────────────────────────────────

# 4. Restart to apply
You: /restart

# Process Restart (confirmation):
⚠️  This will restart the Claude Code process.
   Your session will be preserved and automatically resumed.

   Preserve:
   - Session ID: abc123
   - Working directory: /home/me/project
   - Model: claude-sonnet-4-5

Proceed with restart? (y/n): y

# Process Restart (trigger):
✅ Restart initiated. New Claude Code process started.
   Session will be restored automatically.

# Process Restart (Stop hook):
# - Saves state to JSON
# - Spawns: claude --resume abc123
# - Exits current process

─────────────────────────────────────────────

# 5. New process starts

# Process Restart (SessionStart hook):
🔄 Session restored from restart

Restored state:
- Session ID: abc123
- Working directory: /home/me/project
- Model: claude-sonnet-4-5

MCP servers reloaded with current configuration.

─────────────────────────────────────────────

# 6. Verify new state
Session ID: abc123          ← SAME (conversation history preserved)
Working Dir: /home/me/project  ← SAME
MCP Servers: github, filesystem, database  ← NEW SERVER ADDED

You can continue where you left off.
```

### Inter-Plugin Communication

While the plugins are independent, they communicate through:

1. **Shared Cache Directory**:
```
~/.cache/unleash/
├── mcp-refresh/
│   └── config-hashes.txt
└── process-restart/
    ├── restart-trigger
    └── restart-state.json
```

2. **Command References**:
- `mcp-refresh` suggests using `/restart`
- Users naturally flow from `/reload-mcps` to `/restart`

3. **Complementary Hooks**:
- MCP Refresh: PreToolUse (detection)
- Process Restart: Stop, SessionStart (application)

4. **No Direct Dependencies**:
- Each plugin works independently
- Can be enabled/disabled separately
- No shared code or state

### Design Rationale

**Why Two Plugins?**

1. **Separation of Concerns**:
   - Detection vs. Application
   - Read-only vs. State-modifying
   - Automatic vs. User-triggered

2. **Independent Testing**:
   - Test MCP detection separately
   - Test restart separately
   - Easier to debug issues

3. **Reusability**:
   - Process Restart useful for non-MCP scenarios
   - MCP Refresh useful without restart
   - Composable functionality

4. **Maintenance**:
   - Smaller, focused codebases
   - Easier to understand
   - Clear ownership

## Limitations

### What Cannot Be Done (Plugin-Only Approach)

Due to Claude Code being closed-source, the following are **not possible** with a plugin-only approach:

#### 1. True Hot-Reload

**Cannot do**:
- Stop and restart individual MCP servers without full process restart
- Reload configuration for a single server while others continue running
- Update server configuration in-place

**Why not**:
- MCP servers are initialized in Claude Code's core
- No exposed API to manage server lifecycle
- Server connections deeply integrated with runtime
- Cannot modify core initialization code

**Workaround**:
- Full process restart (what we do)
- Preserves session to minimize disruption
- Applies all config changes at once

#### 2. Selective Server Reload

**Cannot do**:
- `/reload-mcps github` to reload only GitHub server
- Update one server while others stay connected
- Graceful server-specific reconnection

**Why not**:
- No server-level lifecycle management exposed
- Cannot access internal MCP manager
- All servers initialized together at startup

**Workaround**:
- Restart all servers together
- User can see what changed via `/reload-mcps`
- Fast restart time (3-5 seconds)

#### 3. OAuth Token Refresh Without Restart

**Cannot do**:
- Refresh expired OAuth token without full restart
- Trigger re-authentication for specific server
- Update credentials while server is running

**Why not**:
- Credential management in core, not exposed
- OAuth flow tied to server initialization
- No plugin API for credential updates

**Workaround**:
- Update token in config file
- Restart to apply new token
- OAuth tokens are reused across restarts

#### 4. MCP Server Health Monitoring

**Cannot do**:
- Check if MCP server is actually connected
- Detect server crashes or disconnections
- Auto-reconnect on server failure
- Show real-time connection status

**Why not**:
- No exposed API to query server status
- Cannot hook into connection management
- Health checks happen in core

**Workaround**:
- `/mcp-status` shows configured servers
- Users notice when servers don't work
- Restart to recover from issues

#### 5. Automatic Configuration Reload

**Cannot do**:
- Automatically apply config changes without user action
- Silent background reload
- Zero-downtime configuration updates

**Why not**:
- Restart requires user consent (exit process)
- Cannot force process restart
- Session preservation needs user awareness

**Workaround**:
- Automatic **detection** (notification)
- Manual **application** (`/restart`)
- User controls when restart happens

### Plugin Constraints

The Claude Code plugin system has architectural constraints:

#### Plugin Capabilities

Plugins **can**:
- ✅ Add commands (`/reload-mcps`, `/restart`)
- ✅ Add agents and skills
- ✅ Add hooks (PreToolUse, Stop, SessionStart, etc.)
- ✅ Read configuration files
- ✅ Execute shell scripts
- ✅ Create and manage files
- ✅ Spawn processes (via hooks)
- ✅ Modify plugin-owned state

Plugins **cannot**:
- ❌ Modify Claude Code core behavior
- ❌ Access internal runtime state
- ❌ Hook into MCP server lifecycle
- ❌ Replace or override core components
- ❌ Access conversation history directly
- ❌ Modify other plugins' state
- ❌ Intercept tool executions (only observe)
- ❌ Force process restart

#### Hook Limitations

Hooks **can**:
- ✅ Observe events (PreToolUse, PostToolUse, etc.)
- ✅ Return prompts or JSON to Claude
- ✅ Execute arbitrary shell commands
- ✅ Block tool execution (return "deny")
- ✅ Modify hook-specific context

Hooks **cannot**:
- ❌ Modify tool inputs or outputs
- ❌ Change Claude Code's internal state
- ❌ Access session history
- ❌ Directly control MCP servers
- ❌ Override core decisions

### Workarounds and Trade-offs

Despite these limitations, the plugin-only approach provides:

**Advantages**:
- ✅ No core modifications (upstream sync safe)
- ✅ Easy to install and uninstall
- ✅ Portable across Claude Code versions
- ✅ Safe (cannot break core functionality)
- ✅ Maintainable (isolated code)

**Trade-offs**:
- ⚠️ Full process restart required (not true hot-reload)
- ⚠️ 3-5 second downtime during restart
- ⚠️ Cannot reload individual servers
- ⚠️ User action required to apply changes
- ⚠️ No automatic recovery from server failures

**User Experience**:
- Good: Clear notifications when configs change
- Good: Easy to review changes before applying
- Good: Session preserved across restart
- Good: Fast restart time (3-5 seconds)
- Limitation: Must confirm restart
- Limitation: Brief interruption of work

### Comparison: Plugin vs. Core Integration

If we had access to Claude Code's source:

| Feature | Plugin-Only (Current) | Core Integration (Ideal) |
|---------|----------------------|-------------------------|
| Detect config changes | ✅ Yes | ✅ Yes |
| Notify user | ✅ Yes | ✅ Yes |
| Reload all servers | ✅ Via restart | ✅ True hot-reload |
| Reload one server | ❌ No | ✅ Yes |
| Preserve session | ✅ Yes | ✅ Yes |
| Downtime | ⚠️ 3-5 seconds | ✅ Zero downtime |
| User action required | ⚠️ Yes | ✅ Optional |
| Auto-recovery | ❌ No | ✅ Yes |
| Health monitoring | ❌ No | ✅ Yes |
| Upstream sync safe | ✅ Yes | ❌ Conflicts |

**Conclusion**: Plugin-only approach is 80% of ideal functionality with 100% maintainability.

## Troubleshooting

### Common Issues and Solutions

#### Issue 1: MCP Changes Not Detected

**Symptoms**:
- Edit `.mcp.json` but no notification appears
- `/reload-mcps` shows no changes
- Notification doesn't appear before tool use

**Possible Causes**:
1. Auto-detect disabled in settings
2. File not in monitored paths
3. Cache hash corrupted
4. Plugin not loaded

**Solutions**:

```bash
# 1. Check auto-detect setting
cat .claude/settings.json | grep -A 5 mcp-refresh

# Should see:
# "mcp-refresh": {
#   "autoDetect": true
# }

# If false, enable it:
vim .claude/settings.json
# Set "autoDetect": true

# 2. Verify file path is monitored
cat .claude/settings.json | grep -A 5 configPaths

# Should include your config file
# Add if missing:
# "configPaths": [".mcp.json", "your-file.json"]

# 3. Clear cache to force redetection
rm -rf ~/.cache/unleash/mcp-refresh/

# 4. Verify plugin is loaded
# In Claude:
/help
# Should see /reload-mcps and /mcp-status
```

#### Issue 2: Restart Doesn't Preserve Session

**Symptoms**:
- After `/restart`, new session ID assigned
- Conversation history lost
- No "Session restored" message

**Possible Causes**:
1. State file not created
2. State file expired
3. Permissions issue
4. `preserveSession` disabled

**Solutions**:

```bash
# 1. Check if state file was created
ls -la ~/.cache/unleash/process-restart/
# Should see restart-state.json

# 2. Check state file age
cat ~/.cache/unleash/process-restart/restart-state.json
# Check timestamp vs current time

# 3. Verify file permissions
ls -l ~/.cache/unleash/process-restart/restart-state.json
# Should be: -rw------- (600)

# Fix permissions:
chmod 600 ~/.cache/unleash/process-restart/restart-state.json

# 4. Check preserveSession setting
cat .claude/settings.json | grep -A 5 process-restart
# Should see: "preserveSession": true

# 5. Increase expiry for slow systems
vim .claude/settings.json
# Set "stateFileExpiry": 600  # 10 minutes
```

#### Issue 3: State File Expired Error

**Symptoms**:
- "State file expired or not found" message
- State file exists but not used
- Fresh session starts after restart

**Possible Causes**:
1. Slow system (takes > 5 minutes to restart)
2. State file from old restart attempt
3. Clock skew

**Solutions**:

```bash
# 1. Increase expiry time
vim .claude/settings.json

{
  "plugins": {
    "process-restart": {
      "stateFileExpiry": 600  # Increase from 300 to 600
    }
  }
}

# 2. Clean up old state files
rm -rf ~/.cache/unleash/process-restart/*

# 3. Check system time
date
# Verify time is correct

# 4. Test restart timing
time /restart
# Should complete in < 5 seconds
# If slower, investigate system performance
```

#### Issue 4: Process Doesn't Restart Automatically

**Symptoms**:
- Current process exits after `/restart`
- No new process starts
- Back at shell prompt

**Possible Causes**:
1. Claude Code not in PATH
2. nohup not available
3. Spawn script error
4. Insufficient permissions

**Solutions**:

```bash
# 1. Verify Claude Code is in PATH
which claude
# Should output: /usr/local/bin/claude (or similar)

# If not found:
export PATH="/path/to/claude:$PATH"
# Add to ~/.bashrc for persistence

# 2. Verify nohup exists
which nohup
# Should output: /usr/bin/nohup (or similar)

# If not found:
sudo apt-get install coreutils  # Debian/Ubuntu
# or
brew install coreutils  # macOS

# 3. Check Stop hook logs
tail -f ~/.claude/logs/debug.log
# Look for errors during restart

# 4. Test manual restart
# Get session ID first
SESSION_ID=$(cat ~/.cache/unleash/process-restart/restart-state.json | jq -r '.sessionId')

# Then manually start
claude --resume "$SESSION_ID"

# If this works, hook script has issue
# Check hook script permissions:
ls -la <REPO_ROOT>/plugins/bundled/process-restart/hooks-handlers/
# All .sh files should be executable (755)
chmod +x <REPO_ROOT>/plugins/bundled/process-restart/hooks-handlers/*.sh
```

#### Issue 5: Working Directory Not Restored

**Symptoms**:
- After restart, wrong working directory
- Session restored but in different location
- MCP servers can't find project files

**Possible Causes**:
1. `preserveWorkingDir` disabled
2. Directory no longer exists
3. Permission issue accessing directory
4. State file corrupted

**Solutions**:

```bash
# 1. Check preserveWorkingDir setting
cat .claude/settings.json | grep -A 5 process-restart

# Should see:
# "preserveWorkingDir": true

# 2. Verify directory exists
cat ~/.cache/unleash/process-restart/restart-state.json | jq -r '.workingDir'
# Output: <PROJECT_ROOT>

ls -ld <PROJECT_ROOT>
# Should show directory exists

# 3. Check directory permissions
cd <PROJECT_ROOT>
# Should succeed

# If permission denied:
chmod 755 <PROJECT_ROOT>
# Or adjust ownership:
sudo chown $USER:$USER <PROJECT_ROOT>

# 4. Verify state file is valid JSON
jq . ~/.cache/unleash/process-restart/restart-state.json
# Should output formatted JSON

# If error, remove corrupted file:
rm ~/.cache/unleash/process-restart/restart-state.json
```

#### Issue 6: Frequent False Positive Notifications

**Symptoms**:
- "MCP config changed" notification appears often
- No actual changes made to config files
- Notification on every tool use

**Possible Causes**:
1. File formatting changes (auto-formatter)
2. Whitespace or comment changes
3. Another process modifying files
4. Editor auto-save

**Solutions**:

```bash
# 1. Disable auto-formatting for MCP config files
# In your editor, exclude .mcp.json from auto-format

# 2. Reduce notification frequency
vim .claude/settings.json

{
  "plugins": {
    "mcp-refresh": {
      "autoDetect": false  # Disable automatic detection
    }
  }
}

# Then check manually:
/reload-mcps

# 3. Identify what's changing
# Before tool use:
sha256sum .mcp.json
# After notification:
sha256sum .mcp.json
# Compare hashes to see if file actually changed

# 4. Lock config file (prevent external modifications)
chmod 444 .mcp.json  # Read-only
# Edit: chmod 644, make changes, chmod 444 again

# 5. Clear cache and let it rebuild
rm -rf ~/.cache/unleash/mcp-refresh/
```

#### Issue 7: Multiple Restart Attempts Fail

**Symptoms**:
- First `/restart` works
- Subsequent `/restart` commands fail
- Trigger or state files persist

**Possible Causes**:
1. Trigger file not cleaned up
2. State file not removed
3. Orphaned Claude processes
4. Cache corruption

**Solutions**:

```bash
# 1. Clean up all restart state
rm -rf ~/.cache/unleash/process-restart/*

# 2. Kill any orphaned processes
ps aux | grep claude
# Look for multiple claude processes

# Kill extras:
pkill -9 claude

# 3. Restart Claude manually
claude

# 4. Test /restart again
/restart

# 5. If still fails, check logs
tail -50 ~/.claude/logs/debug.log
# Look for error messages

# 6. Verify cache directory permissions
ls -ld ~/.cache/unleash/
# Should be: drwxr-xr-x (755)

chmod 755 ~/.cache/unleash/
chmod 755 ~/.cache/unleash/process-restart/
```

### Debug Mode

Enable detailed logging for troubleshooting:

```bash
# 1. Enable bash debug mode in hooks
vim <REPO_ROOT>/plugins/bundled/mcp-refresh/hooks-handlers/check-mcp-changes.sh

# Add after shebang:
set -x  # Enable debug output

# 2. Run Claude Code and capture output
claude 2>&1 | tee claude-debug.log

# 3. Trigger the issue
# Edit .mcp.json or run /restart

# 4. Review debug output
less claude-debug.log
# Look for error messages or unexpected behavior

# 5. Disable debug mode when done
# Remove 'set -x' from scripts
```

### Getting Help

If issues persist:

1. **Check Plugin README Files**:
   - [MCP Refresh README](<REPO_ROOT>/plugins/bundled/mcp-refresh/README.md)
   - [Process Restart README](<REPO_ROOT>/plugins/bundled/process-restart/README.md)

2. **Review Logs**:
   ```bash
   tail -100 ~/.claude/logs/debug.log
   ```

3. **Check Plugin Status**:
   ```bash
   # In Claude:
   /help
   # Verify plugins are loaded
   ```

4. **Test Minimal Configuration**:
   ```bash
   # Disable all plugins except these two
   vim .claude/settings.json
   {
     "plugins": {
       "enabled": ["mcp-refresh", "process-restart"]
     }
   }
   ```

5. **Create Issue**:
   - Repository: unleash
   - Include: error messages, logs, configuration
   - Steps to reproduce

## Future Enhancements

### If Claude Code Source Becomes Available

If Anthropic open-sources Claude Code or provides MCP management APIs, we could implement:

#### 1. True Hot-Reload

**Capability**: Reload MCP servers without process restart.

**Implementation**:
```javascript
// Hypothetical core integration
class MCPManager {
  async reloadServer(serverName) {
    const config = readConfig();
    const server = config.mcpServers[serverName];

    // Stop existing server
    await this.servers[serverName].stop();

    // Start with new config
    this.servers[serverName] = await this.startServer(server);
  }
}
```

**User Experience**:
```bash
You: /reload-mcps github

Reloading github server...
✓ Stopped github server
✓ Started github server with new config
✓ Server ready in 0.5s

No process restart needed!
```

#### 2. Selective Server Reload

**Capability**: Update one server while others continue running.

**Implementation**:
```javascript
// Plugin command
async function reloadSpecificServer(serverName) {
  // Access to core MCP manager
  const manager = claudeCode.mcp;

  // Reload only specified server
  await manager.reloadServer(serverName);

  return `Server ${serverName} reloaded successfully`;
}
```

**User Experience**:
```bash
You: /reload-mcps github

Changes detected for: github
  - Environment: GITHUB_TOKEN updated

Reload only github server? (y/n): y

✓ github server reloaded (0.3s)
  Other servers unaffected
```

#### 3. Automatic OAuth Refresh

**Capability**: Refresh OAuth tokens without manual intervention.

**Implementation**:
```javascript
// Hook into OAuth token expiry
class MCPOAuthManager {
  onTokenExpired(serverName, refreshToken) {
    // Automatic refresh
    const newToken = await this.refreshOAuthToken(refreshToken);

    // Update server config
    await this.updateServerToken(serverName, newToken);

    // Reload server
    await this.reloadServer(serverName);
  }
}
```

**User Experience**:
```bash
# Background, automatic
[github server OAuth token expired]
[Automatically refreshing token...]
[Token refreshed, server reconnected]

# User never interrupted
```

#### 4. Server Health Monitoring

**Capability**: Detect and auto-recover from server failures.

**Implementation**:
```javascript
// Health check system
class MCPHealthMonitor {
  async checkHealth(serverName) {
    const server = this.servers[serverName];

    try {
      await server.ping();
      return { status: 'healthy' };
    } catch (error) {
      return { status: 'unhealthy', error };
    }
  }

  async autoRecover(serverName) {
    // Attempt reconnection
    await this.reloadServer(serverName);
  }
}
```

**User Experience**:
```bash
You: /mcp-status

Current MCP Servers:
  ✓ github (healthy, uptime: 2h 15m)
  ⚠ database (reconnecting, last error: connection timeout)
  ✓ filesystem (healthy, uptime: 2h 15m)

Auto-recovery in progress for: database
```

#### 5. Configuration Hot-Swap

**Capability**: Apply config changes instantly without restart.

**Implementation**:
```javascript
// File watcher integration
const watcher = fs.watch('.mcp.json', async (event) => {
  if (event === 'change') {
    const newConfig = readConfig();
    const diff = computeDiff(oldConfig, newConfig);

    // Apply only changed servers
    for (const [name, change] of diff) {
      if (change.type === 'added') {
        await mcpManager.addServer(name, change.config);
      } else if (change.type === 'modified') {
        await mcpManager.reloadServer(name);
      } else if (change.type === 'removed') {
        await mcpManager.removeServer(name);
      }
    }

    notify(`Applied ${diff.length} MCP configuration changes`);
  }
});
```

**User Experience**:
```bash
# User edits .mcp.json
vim .mcp.json

# Automatic, instant
✓ Detected configuration change
✓ Added: new-server
✓ Modified: github
✓ Configuration applied in 0.8s

# No restart, no interruption
```

#### 6. Advanced Plugin APIs

**New Capabilities**:
- `claudeCode.mcp.list()` - Get all MCP servers
- `claudeCode.mcp.reload(name)` - Reload specific server
- `claudeCode.mcp.health(name)` - Check server health
- `claudeCode.session.preserve()` - Save session state
- `claudeCode.session.restore()` - Restore session state

**Example Plugin**:
```javascript
// Advanced MCP management plugin
module.exports = {
  commands: {
    'mcp-reload': async (args) => {
      const serverName = args[0];

      // Direct access to MCP manager
      const manager = claudeCode.mcp;

      // Check current status
      const health = await manager.health(serverName);

      if (health.status === 'healthy') {
        console.log(`${serverName} is healthy, reload anyway? (y/n)`);
        const confirm = await getUserConfirmation();
        if (!confirm) return;
      }

      // Reload
      await manager.reload(serverName);

      // Verify
      const newHealth = await manager.health(serverName);
      return `Server ${serverName} reloaded: ${newHealth.status}`;
    }
  }
};
```

### Enhanced User Experience

With core integration, the workflow becomes:

```
Current (Plugin-Only):
1. Edit config
2. Notification
3. /reload-mcps
4. /restart
5. Confirm
6. Wait 3-5s
7. Session restored

Future (Core Integration):
1. Edit config
2. Automatic reload (0.5s)
3. Continue working

Or with confirmation:
1. Edit config
2. Notification
3. /reload-mcps
4. Instant reload
```

### Migration Path

When core APIs become available:

```bash
# Phase 1: Core APIs released
# - Update plugins to use new APIs
# - Keep compatibility with old approach

# Phase 2: Deprecation period
# - Both approaches work
# - Users can choose

# Phase 3: Full transition
# - Remove restart-based approach
# - Use only hot-reload APIs
```

## Technical Reference

### File Locations

#### MCP Refresh Plugin

```
<REPO_ROOT>/plugins/bundled/mcp-refresh/
├── .claude-plugin/
│   └── plugin.json                  # Plugin manifest
├── commands/
│   ├── reload-mcps.md              # /reload-mcps command
│   └── mcp-status.md               # /mcp-status command
├── hooks/
│   └── hooks.json                  # Hook configuration
├── hooks-handlers/
│   └── check-mcp-changes.sh        # PreToolUse hook script
└── README.md                        # Plugin documentation
```

#### Process Restart Plugin

```
<REPO_ROOT>/plugins/bundled/process-restart/
├── .claude-plugin/
│   └── plugin.json                  # Plugin manifest
├── commands/
│   └── restart.md                   # /restart command
├── hooks/
│   └── hooks.json                  # Hook configuration
├── hooks-handlers/
│   ├── restart-handler.sh          # Stop hook script
│   └── session-restore.sh          # SessionStart hook script
├── scripts/
│   └── trigger-restart.sh          # Restart trigger script
└── README.md                        # Plugin documentation
```

#### Cache Files

```
~/.cache/unleash/
├── mcp-refresh/
│   └── config-hashes.txt           # SHA256 hash cache
└── process-restart/
    ├── restart-trigger             # Restart trigger file
    └── restart-state.json          # Session state file
```

### Hook Specifications

#### PreToolUse Hook (MCP Refresh)

**Event**: Before any tool execution

**Input** (stdin, JSON):
```json
{
  "toolName": "Read",
  "toolInput": {
    "file_path": "/home/me/project/file.txt"
  },
  "sessionId": "abc123",
  "workingDir": "/home/me/project"
}
```

**Output** (stdout, JSON):
```json
{
  "type": "prompt",
  "content": "MCP configuration files have changed..."
}
```

**Exit Codes**:
- `0`: Continue normally (with or without prompt)
- Non-zero: Error (tool execution continues anyway)

#### Stop Hook (Process Restart)

**Event**: When Claude Code is exiting

**Input** (stdin, JSON):
```json
{
  "reason": "user_exit",
  "sessionId": "abc123",
  "workingDir": "/home/me/project",
  "model": "claude-sonnet-4-5"
}
```

**Output** (stdout, JSON):
```json
{
  "type": "info",
  "content": "Session state saved, restarting..."
}
```

**Side Effects**:
- Creates `restart-state.json` file
- Spawns new Claude Code process via `nohup`
- Deletes `restart-trigger` file

**Exit Codes**:
- `0`: Allow exit to proceed
- Non-zero: Error (exit proceeds anyway)

#### SessionStart Hook (Process Restart)

**Event**: When new Claude Code session starts

**Input** (stdin, JSON):
```json
{
  "sessionId": "abc123",
  "workingDir": "/home/me/project",
  "model": "claude-sonnet-4-5"
}
```

**Output** (stdout, JSON):
```json
{
  "type": "prompt",
  "content": "🔄 Session restored from restart\n\nRestored state:\n- Session ID: abc123\n- Working directory: /home/me/project"
}
```

**Side Effects**:
- Reads `restart-state.json` file
- Changes working directory via `cd`
- Deletes `restart-state.json` after restoration

**Exit Codes**:
- `0`: Success
- Non-zero: Error (session continues with defaults)

### Configuration Schema

#### MCP Refresh Settings

```typescript
interface MCPRefreshSettings {
  // Enable automatic change detection
  autoDetect: boolean;  // default: true

  // Paths to monitor for MCP configuration
  configPaths: string[];  // default: [".mcp.json", ".claude.json", "~/.claude.json"]
}
```

**Example**:
```json
{
  "plugins": {
    "mcp-refresh": {
      "autoDetect": true,
      "configPaths": [
        ".mcp.json",
        ".claude.json",
        "~/.claude.json",
        "custom/config.json"
      ]
    }
  }
}
```

#### Process Restart Settings

```typescript
interface ProcessRestartSettings {
  // Preserve session ID and conversation history
  preserveSession: boolean;  // default: true

  // Restore working directory after restart
  preserveWorkingDir: boolean;  // default: true

  // Restore permission mode
  preservePermissions: boolean;  // default: true

  // State file expiry time (seconds)
  stateFileExpiry: number;  // default: 300
}
```

**Example**:
```json
{
  "plugins": {
    "process-restart": {
      "preserveSession": true,
      "preserveWorkingDir": true,
      "preservePermissions": true,
      "stateFileExpiry": 600
    }
  }
}
```

### State File Format

**File**: `~/.cache/unleash/process-restart/restart-state.json`

**Schema**:
```typescript
interface RestartState {
  // State file format version
  version: string;  // "1.0.0"

  // Unix timestamp when state was saved
  timestamp: number;  // seconds since epoch

  // Claude session identifier
  sessionId: string;  // e.g., "abc123"

  // Absolute path to working directory
  workingDir: string;  // e.g., "/home/me/project"

  // Model identifier
  model: string;  // e.g., "claude-sonnet-4-5"

  // Git branch name (empty if not in repo)
  gitBranch: string;  // e.g., "feature/my-feature"

  // List of enabled plugin names
  enabledPlugins: string[];  // e.g., ["mcp-refresh", "process-restart"]
}
```

**Example**:
```json
{
  "version": "1.0.0",
  "timestamp": 1735689600,
  "sessionId": "a8ea16a",
  "workingDir": "/home/me/unleash",
  "model": "claude-sonnet-4-5",
  "gitBranch": "feature/mcp-refresh",
  "enabledPlugins": ["mcp-refresh", "process-restart"]
}
```

### Performance Metrics

#### MCP Refresh

**Detection Time**:
- Hash computation: < 10ms
- File I/O: < 5ms
- Comparison: < 1ms
- Total: < 20ms per check

**Memory Usage**:
- Cache file: < 100 bytes
- Runtime memory: < 1 MB
- No persistent background processes

**Disk Usage**:
- Cache directory: < 1 KB

#### Process Restart

**Restart Time**:
- State save: < 100ms
- Process spawn: 500ms
- Current exit: 500-1000ms
- New process start: 2-3s
- State restore: 100-200ms
- Total: 3-5s

**Memory Usage**:
- State file: < 1 KB
- Runtime overhead: < 100 KB
- No persistent memory after restoration

**Disk Usage**:
- State file: < 1 KB (temporary)
- Trigger file: 0 bytes (empty)
- Total: < 2 KB

### Security Considerations

#### State File Security

**Permissions**: 600 (owner read/write only)

**Contents** (sensitive):
- Session ID (grants access to conversation history)
- Working directory path
- Configuration details

**Protection**:
- Restrictive file permissions
- Automatic deletion after restoration
- Time-based expiry
- User-specific cache directory

**Best Practices**:
```bash
# DO: Let plugin manage state files
/restart

# DON'T: Share state files
# DON'T: Commit to version control
# DON'T: Modify manually
# DON'T: Use in shared directories
```

#### OAuth Token Handling

**Important**: OAuth tokens are NOT stored in restart state files.

- Tokens managed by Claude Code's credential storage
- Persist across restarts automatically
- Not included in plugin state
- No additional security risk

### Command Reference

#### `/reload-mcps [server-name]`

**Plugin**: mcp-refresh

**Purpose**: Check MCP configuration changes

**Arguments**:
- `server-name` (optional): Specific server to check

**Output**: Change summary with add/modify/remove details

**Examples**:
```bash
/reload-mcps
/reload-mcps github
```

#### `/mcp-status [verbose]`

**Plugin**: mcp-refresh

**Purpose**: Show current MCP server status

**Arguments**:
- `verbose` (optional): Show detailed configuration

**Output**: Server list with connection status

**Examples**:
```bash
/mcp-status
/mcp-status verbose
```

#### `/restart [--force] [--clean]`

**Plugin**: process-restart

**Purpose**: Restart Claude Code with session preservation

**Arguments**:
- `--force` (optional): Skip confirmation prompt
- `--clean` (optional): Don't preserve state

**Output**: Confirmation prompt (unless --force), restart status

**Examples**:
```bash
/restart
/restart --force
/restart --clean
/restart --force --clean
```

---

## Related Documentation

- [MCP Refresh Plugin README](<REPO_ROOT>/plugins/bundled/mcp-refresh/README.md)
- [Process Restart Plugin README](<REPO_ROOT>/plugins/bundled/process-restart/README.md)
- [Plugin Development Guide](<REPO_ROOT>/docs/extensions/plugin-development.md)

- [Testing Guide](<REPO_ROOT>/docs/extensions/testing-guide.md)

## Contributing

Contributions to improve these plugins are welcome! Please:

1. Read the [Plugin Development Guide](<REPO_ROOT>/docs/extensions/plugin-development.md)
2. Test changes locally with `--plugin-dir`
3. Update documentation
4. Submit PR with clear description

## License

Same as unleash parent repository.

## Authors

Heiervang Technologies

## Version History

### mcp-refresh 1.0.0 (2026-01-01)
- Automatic change detection via PreToolUse hook
- `/reload-mcps` command for detailed change reporting
- `/mcp-status` command for server status
- SHA256-based change detection
- Configurable monitoring paths

### process-restart 1.0.0 (2026-01-01)
- Session ID preservation across restarts
- Working directory restoration
- Model and configuration preservation
- Stop and SessionStart hook integration
- State file expiry mechanism
- Confirmation prompts and safety features
- Clean restart option (--clean flag)
- Force restart option (--force flag)
- Integration with mcp-refresh plugin
