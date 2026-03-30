# Auto Mode Plugin

Enables autonomous operation mode where Claude continues working until explicitly stopped. Uses a **Stop hook** to enforce continuous operation.

## Usage

```
/auto
```

When prompted, Claude will activate the auto mode flag, which enables the Stop hook enforcement.

## How It Works

### Stop Hook Enforcement

The plugin uses Claude Code's `Stop` hook to prevent Claude from ending its turn:

1. `/auto` command instructs Claude to run the activation script
2. Activation script creates a flag file at `~/.cache/unleash/auto-mode/active`
3. The Stop hook (`auto-mode-stop.sh`) checks for this flag
4. If active, the hook returns `{"decision": "block"}`, forcing Claude to continue
5. Claude receives the block message and must keep working

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     AUTO MODE FLOW                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  User runs /auto                                            │
│       │                                                     │
│       ↓                                                     │
│  Claude activates flag file                                 │
│       │                                                     │
│       ↓                                                     │
│  Claude works on tasks...                                   │
│       │                                                     │
│       ↓                                                     │
│  Claude tries to end turn                                   │
│       │                                                     │
│       ↓                                                     │
│  ┌─────────────────────────────────────┐                    │
│  │ Stop Hook (auto-mode-stop.sh)       │                    │
│  │   - Checks for flag file            │                    │
│  │   - If exists: return BLOCK         │                    │
│  │   - Claude forced to continue       │                    │
│  └─────────────────────────────────────┘                    │
│       │                                                     │
│       ↓                                                     │
│  Claude continues working...                                │
│       │                                                     │
│       ↓                                                     │
│  Owner says "stop" OR runs exit-claude                      │
│       │                                                     │
│       ↓                                                     │
│  Flag file removed → Stop hook allows exit                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## What It Does

When you run `/auto`, Claude enters autonomous mode with these behaviors:

1. **No voluntary exit** - Stop hook blocks Claude from ending its turn
2. **Continuous operation** - After each task, Claude looks for more work
3. **Owner contact** - Claude checks for MCP notification tools to contact you
4. **Exit only by command** - Only `exit-claude` or owner's explicit "stop" ends the session

## Exit Conditions

Claude will only exit auto mode when:
- You explicitly tell it to stop ("quit", "exit", "stop", "that's enough")
- You run `exit-claude` (automatically deactivates auto mode)
- A critical unrecoverable error occurs

## MCP Integration

On entering auto mode, Claude checks for MCP tools that can contact you:
- Notification tools
- Messaging/email tools
- Webhook tools

If found, Claude uses these to request clarification when blocked or report progress.

## File Locations

| File | Purpose |
|------|---------|
| `commands/auto.md` | The /auto skill definition |
| `hooks/auto-mode-stop.sh` | Stop hook that enforces auto mode |
| `scripts/activate-auto-mode.sh` | Creates the auto mode flag |
| `scripts/deactivate-auto-mode.sh` | Removes the auto mode flag |
| `~/.cache/unleash/auto-mode/active` | Flag file when active |

## Configuration

The Stop hook must be configured in `~/.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "$HOME/unleash/plugins/bundled/auto-mode/hooks/auto-mode-stop.sh"
          }
        ]
      }
    ]
  }
}
```

### Customizing the Stop Prompt

By default, when Claude tries to exit in auto mode, it receives the message:
```
To exit: run 'exit-claude' via Bash tool. Do not end your turn without taking action.
```

You can customize this message globally using either the CLI or TUI.

#### Via CLI

```bash
# Set a custom prompt
unleash claude --stop-prompt="Keep working on the task until complete!"

# Edit with your $EDITOR
unleash claude --stop-prompt-edit

# Reset to default
unleash claude --stop-prompt-clear
```

#### Via TUI

```bash
unleash  # Launch the TUI
# Navigate to: Settings > Stop Prompt
# Press Enter to edit, type your custom message, press Enter to save
```

The custom prompt is stored in `~/.config/unleash/config.toml` and applies to all future auto-mode sessions.

#### Prompt Priority

The stop hook uses the following priority when determining which message to show:

1. **Session-specific** (highest): `~/.cache/unleash/auto-mode/reminder-${PID}`
   - Set programmatically for specific sessions
2. **Global config**: `~/.config/unleash/config.toml` (stop_prompt field)
   - Set via CLI flags or TUI
3. **Default** (lowest): Hardcoded message in the hook script

## Use Cases

- Long-running autonomous coding tasks
- Background refactoring with minimal supervision
- Continuous integration/improvement workflows
- "Set and forget" development sessions

## Requirements

- Must run under `unleash` wrapper for `exit-claude` to work
- Stop hook must be configured in settings
- Optional: MCP server with notification capabilities

## Version History

- **1.1.0** (2026-01-07) - Stop hook enforcement
  - Added Stop hook to block Claude from ending turn
  - Flag file system for activation/deactivation
  - Integration with exit-claude for clean shutdown

- **1.0.0** (2026-01-07) - Initial release
  - Basic /auto command with instructions
