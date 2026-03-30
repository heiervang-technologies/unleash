# Configuration Guide

This guide covers configuration options for unleash and its extensions.

## Table of Contents

- [Overview](#overview)
- [Configuration Files](#configuration-files)
- [Stop Prompt Configuration](#stop-prompt-configuration)
- [TUI Settings](#tui-settings)
- [CLI Configuration](#cli-configuration)

## Overview

unleash uses multiple configuration files to manage settings:

| File | Purpose | Format |
|------|---------|--------|
| `~/.config/unleash/config.toml` | TUI and global settings | TOML |
| `~/.claude/settings.json` | Claude Code settings and hooks | JSON |
| `~/.cache/unleash/` | Runtime state and temporary files | Various |

## Configuration Files

### TUI Configuration (`config.toml`)

Located at `~/.config/unleash/config.toml`, this file stores:

- Current profile selection
- Claude executable path
- Default command-line arguments
- **Stop prompt message** (auto-mode)

Example:
```toml
current_profile = "default"
claude_path = "claude"
agent_args = []
stop_prompt = "Keep working on the task!"
```

### Claude Code Settings (`settings.json`)

Located at `~/.claude/settings.json`, this file configures:

- Enabled plugins
- Hook configurations
- MCP server settings
- Organization settings

Example:
```json
{
  "plugins": {
    "enabled": [
      "auto-mode",
      "mcp-refresh",
      "process-restart"
    ]
  },
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

## Stop Prompt Configuration

The stop prompt is the message Claude receives when the auto-mode stop hook blocks it from ending its turn.

### Default Message

```
To exit: run 'exit-claude' via Bash tool. Do not end your turn without taking action.
```

### Customizing via CLI

The `unleash` command provides flags to manage the stop prompt:

#### Set Inline

```bash
unleash --stop-prompt="Your custom message here"
```

Example:
```bash
unleash --stop-prompt="Stay focused! Use exit-claude when done."
```

#### Edit with $EDITOR

Opens your default editor ($EDITOR, $VISUAL, or vi) to edit the prompt:

```bash
unleash --stop-prompt-edit
```

This creates a temporary file with the current prompt, opens it in your editor, and saves the result when you exit.

#### Clear (Reset to Default)

```bash
unleash --stop-prompt-clear
```

Removes the custom prompt from config.toml, causing the hook to use its default hardcoded message.

### Customizing via TUI

Launch the TUI and navigate to the Settings screen:

```bash
unleash
```

Steps:
1. Press `j` or `↓` to navigate to "Settings"
2. Press `Enter` to open Settings
3. Press `j` or `↓` to navigate to "Stop Prompt"
4. Press `Enter` to edit
5. Type your custom message
6. Press `Enter` to save
7. Press `Esc` to return to main menu

The setting is saved immediately to `~/.config/unleash/config.toml`.

### Priority Order

When determining which stop prompt message to show, the hook checks in this order:

1. **Session-specific** (highest priority)
   - File: `~/.cache/unleash/auto-mode/reminder-${WRAPPER_PID}`
   - Set programmatically for specific sessions
   - Allows per-session overrides

2. **Global configuration**
   - File: `~/.config/unleash/config.toml`
   - Field: `stop_prompt`
   - Set via CLI flags or TUI
   - Applies to all sessions

3. **Default** (lowest priority)
   - Hardcoded in: `plugins/bundled/auto-mode/hooks/auto-mode-stop.sh`
   - Used when no custom configuration exists

### Use Cases

#### Different Messages for Different Tasks

Set task-specific prompts:

```bash
# For debugging sessions
unleash --stop-prompt="Debug the issue completely before stopping."

# For feature development
unleash --stop-prompt="Complete the feature and all tests before exiting."

# For refactoring
unleash --stop-prompt="Finish the refactoring and verify all tests pass."
```

#### Team Standards

Organizations can standardize stop prompts:

```bash
# Company-wide prompt
unleash --stop-prompt="Follow team exit checklist: tests pass, docs updated, PR ready."
```

#### Motivational Messages

Use prompts to encourage autonomous behavior:

```bash
unleash --stop-prompt="You're doing great! Keep going until the task is complete. Use exit-claude when truly done."
```

### Verification

Check the current stop prompt configuration:

```bash
# View config file
cat ~/.config/unleash/config.toml | grep stop_prompt

# Test the hook directly (requires auto mode active)
export CLAUDE_WRAPPER_PID=$$
mkdir -p ~/.cache/unleash/auto-mode
touch ~/.cache/unleash/auto-mode/active-$$
~/unleash/plugins/bundled/auto-mode/hooks/auto-mode-stop.sh
```

### Troubleshooting

#### Prompt Not Appearing

1. Verify auto mode is active:
   ```bash
   ls ~/.cache/unleash/auto-mode/active-*
   ```

2. Check config file exists and is valid:
   ```bash
   cat ~/.config/unleash/config.toml
   ```

3. Ensure CLAUDE_WRAPPER_PID is set:
   ```bash
   echo $CLAUDE_WRAPPER_PID
   ```

#### Prompt Not Persisting

The prompt is only shown when:
- Auto mode is active (flag file exists)
- Running under the `unleash` wrapper (CLAUDE_WRAPPER_PID set)
- The stop hook is triggered (Claude tries to end turn)

If you're not seeing the prompt:
- Verify you started Claude with `unleash claude` (not `claude` directly)
- Check that auto mode is active (`/auto` was run)
- Ensure Claude is actually trying to exit

## TUI Settings

The TUI (`unleash`) provides a visual interface for managing configuration.

### Available Settings

| Setting | Description | Access |
|---------|-------------|--------|
| Entry Point | Command to launch Claude (e.g., `claude`) | Settings > Entry Point |
| Arguments | Default CLI arguments to pass to Claude | Settings > Arguments |
| Stop Prompt | Auto-mode stop hook message | Settings > Stop Prompt |
| Reset Settings | Restore all settings to defaults | Settings > Reset Settings |

### Navigation

- `j` / `↓` - Move down
- `k` / `↑` - Move up
- `Enter` - Select/Edit
- `Esc` / `q` - Go back
- `?` - Show help

### Editing Values

1. Navigate to the setting
2. Press `Enter`
3. Type the new value (cursor indicator: █)
4. Press `Enter` to save
5. Press `Esc` to cancel

Settings are saved immediately to `~/.config/unleash/config.toml`.

## CLI Configuration

### Command-Line Flags

The `unleash claude` command accepts several configuration flags:

#### Auto Mode

```bash
unleash claude --auto          # Enable auto mode on startup
unleash claude -a              # Short form
```

#### Stop Prompt

```bash
unleash claude --stop-prompt="message"    # Set prompt inline
unleash claude --stop-prompt "message"    # Alternative syntax
unleash claude --stop-prompt-edit         # Edit with $EDITOR
unleash claude --stop-prompt-clear        # Reset to default
```

#### Examples

```bash
# Start with auto mode and custom prompt
unleash claude --auto --stop-prompt="Complete all tests before stopping."

# Edit prompt, then start normally
unleash claude --stop-prompt-edit
unleash claude

# Clear prompt and start
unleash claude --stop-prompt-clear && unleash claude
```

### Environment Variables

The `unleash` wrapper exports these variables:

| Variable | Purpose |
|----------|---------|
| `AGENT_UNLEASH` | Set to `1` when running under wrapper |
| `AGENT_WRAPPER_PID` | Process ID of the wrapper |
| `AGENT_AUTO_MODE` | Set to `1` when auto mode is active |
| `AGENT_UNLEASH_ROOT` | Path to unleash repository |

Check if running under wrapper:

```bash
if [[ "$AGENT_UNLEASH" == "1" ]]; then
    echo "Running under unleash wrapper"
fi
```

## Best Practices

### Configuration Management

1. **Use version control** for team settings
   - Share `config.toml` templates for consistent setups

2. **Document custom prompts**
   - Keep a record of prompt variations for different task types
   - Share effective prompts with your team

3. **Test configuration changes**
   - Verify stop prompts in a test session before committing
   - Check that hooks are triggered correctly

### Security Considerations

1. **Don't commit secrets** in configuration files
   - Use environment variables for API keys
   - Keep credentials in profiles, not global config

2. **Review stop prompts**
   - Avoid prompts that might leak sensitive information
   - Keep messages professional and task-focused

### Performance

1. **Minimize hook complexity**
   - Stop hooks are called frequently
   - Keep custom logic fast and simple

2. **Clean up cache files**
   - Remove old session-specific reminder files
   - Clear `~/.cache/unleash/` periodically

## Related Documentation

- [Auto Mode Plugin README](../../plugins/bundled/auto-mode/README.md)
- [Plugin Development Guide](plugin-development.md)
- [Restart & Refresh Guide](restart-refresh.md)

## Version History

- **1.1.0** (2026-01-12) - Added configurable stop prompts
  - CLI flags: `--stop-prompt`, `--stop-prompt-edit`, `--stop-prompt-clear`
  - TUI settings screen integration
  - Global configuration in config.toml
  - Priority-based prompt selection

- **1.0.0** (2026-01-07) - Initial configuration system
  - TUI configuration file
  - Basic settings management
