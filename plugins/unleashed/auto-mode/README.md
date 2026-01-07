# Auto Mode Plugin

Enables autonomous operation mode where Claude continues working until explicitly stopped.

## Usage

```
/auto
```

## What It Does

When you run `/auto`, Claude enters autonomous mode with these behaviors:

1. **No voluntary exit** - Claude cannot decide to stop on its own
2. **Continuous operation** - After each task, Claude looks for more work
3. **Owner contact** - Claude checks for MCP notification tools to contact you
4. **Exit only by command** - Only `exit-claude` or owner's explicit "stop" ends the session

## Exit Conditions

Claude will only exit auto mode when:
- You explicitly tell it to stop ("quit", "exit", "stop", "that's enough")
- You run `exit-claude`
- A critical unrecoverable error occurs

## Auto-Reminder

After each response, Claude includes a reminder:

```
[AUTO MODE ACTIVE - Awaiting input or continuing work. Exit: exit-claude or owner command]
```

## MCP Integration

On entering auto mode, Claude checks for MCP tools that can contact you:
- Notification tools
- Messaging/email tools
- Webhook tools

If found, Claude uses these to request clarification when blocked or report progress.

## Use Cases

- Long-running autonomous coding tasks
- Background refactoring with minimal supervision
- Continuous integration/improvement workflows
- "Set and forget" development sessions

## Requirements

- Should be run under `claude-unleashed` wrapper for `exit-claude` to work
- Optional: MCP server with notification capabilities

## Version

1.0.0 - Initial release
