# Configuration Guide

This guide covers configuration options for unleash and its plugins.

## Table of Contents

- [Overview](#overview)
- [Configuration Files](#configuration-files)
- [Profiles](#profiles)
- [Auto-Mode Stop Prompt](#auto-mode-stop-prompt)
- [TUI Settings](#tui-settings)
- [CLI Flags](#cli-flags)
- [Environment Variables](#environment-variables)

## Overview

unleash uses a small set of configuration files:

| File/Path | Purpose | Format |
|-----------|---------|--------|
| `~/.config/unleash/config.toml` | Global app state (current profile, animations) | TOML |
| `~/.config/unleash/profiles/*.toml` | Per-profile settings (agent, model, theme, env, …) | TOML |
| `~/.claude/settings.json` | Claude Code settings and hooks (managed by Claude Code) | JSON |
| `~/.cache/unleash/` | Runtime state (auto-mode flags, restart triggers) | Various |

## Configuration Files

### Global Config (`config.toml`)

Located at `~/.config/unleash/config.toml`. Stores only minimal global state:

```toml
current_profile = "claude"
animations = true
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `current_profile` | string | `"claude"` | Name of the active profile |
| `animations` | bool | `true` | Enable/disable TUI animations |

> **Note:** Executable path, arguments, model, stop prompt, and theme are all configured per-profile — not here.

### Profiles (`~/.config/unleash/profiles/`)

Each profile is a `.toml` file in `~/.config/unleash/profiles/`. Default profiles (`claude.toml`, `codex.toml`, `gemini.toml`, `opencode.toml`) are created automatically on first run.

**Example profile (`claude.toml`):**
```toml
name = "claude"
description = "Default profile"
agent_cli_path = "claude"
agent_cli_args = ["--dangerously-skip-permissions"]
theme = "orange"
stop_prompt = "Keep working until the task is done."  # optional

[defaults]
# model = "claude-opus-4-5"   # uncomment to pin a model
# auto = true                  # start in auto-mode by default
# safe = false                 # bypass permission prompts (default)

[env]
# Extra environment variables passed to the agent process
MY_CUSTOM_VAR = "value"
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Profile name (must match filename without `.toml`) |
| `description` | string | Human-readable description shown in TUI |
| `agent_cli_path` | string | Binary name or absolute path (`claude`, `/usr/local/bin/claude`) |
| `agent_cli_args` | string[] | Extra args always passed to the agent |
| `theme` | string | TUI color theme (`orange`, `blue`, `green`, …) |
| `stop_prompt` | string? | Optional message shown to the agent when auto-mode blocks an exit |
| `[defaults]` | section | Default values for polyfill flags (model, auto, safe, effort) |
| `[env]` | section | Environment variables set for the agent process |

Switch profiles from the command line:
```bash
unleash codex       # launch the "codex" profile
unleash my-profile  # launch a custom profile
```

Or select a profile from the TUI:
```bash
unleash   # opens TUI
```

### Claude Code Settings (`~/.claude/settings.json`)

Managed by Claude Code. Hooks registered by unleash plugins appear here automatically. You generally don't need to edit this file manually.

## Profiles

### Creating a Custom Profile

The easiest way is through the TUI (`unleash`). To create one manually:

1. Copy an existing profile:
   ```bash
   cp ~/.config/unleash/profiles/claude.toml ~/.config/unleash/profiles/my-profile.toml
   ```

2. Edit it:
   ```toml
   name = "my-profile"
   description = "Custom setup with Qwen"
   agent_cli_path = "claude"
   agent_cli_args = ["--dangerously-skip-permissions"]
   theme = "blue"

   [defaults]
   model = "qwen3.5-72b"
   auto = true
   ```

3. Launch it:
   ```bash
   unleash my-profile
   ```

### Profile Defaults

The `[defaults]` section sets default values for polyfill flags, equivalent to always passing those flags on the command line:

```toml
[defaults]
model = "claude-opus-4-5"   # same as: unleash my-profile -m claude-opus-4-5
auto = true                  # same as: unleash my-profile --auto
safe = false                 # bypass permission prompts (default behavior)
effort = "high"              # same as: unleash my-profile -e high
```

Command-line flags always override profile defaults.

## Auto-Mode Stop Prompt

When auto-mode is active, the stop hook delivers a message to the agent each time it tries to end its turn. You can customize this per-profile.

### Configure via Profile

Set `stop_prompt` in the profile TOML:

```toml
stop_prompt = "Complete all tests before stopping. Use exit-claude when truly done."
```

### Priority Order

The stop hook selects the message in this priority order:

1. **Session-specific override** — `~/.cache/unleash/auto-mode/reminder-${WRAPPER_PID}` (set programmatically)
2. **Profile stop_prompt** — from `~/.config/unleash/profiles/<name>.toml`
3. **Default** — hardcoded in `plugins/bundled/auto-mode/hooks/auto-mode-stop.sh`

### Troubleshooting

Check auto-mode is active:
```bash
ls ~/.cache/unleash/auto-mode/active-*
```

Check `CLAUDE_WRAPPER_PID` is set (only valid inside an unleash session):
```bash
echo $CLAUDE_WRAPPER_PID
```

## TUI Settings

Launch the TUI for a visual interface:

```bash
unleash
```

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | Select / edit |
| `Esc` / `q` | Go back |
| `?` | Show help |
| `s` | Rescan installed versions |

The TUI lets you:
- Switch the active profile
- Install / switch agent CLI versions
- View agent status

## CLI Flags

### Profile Launch Flags (unified polyfill)

These flags work with any profile and are translated to agent-specific syntax:

```bash
unleash <profile> [FLAGS] [-- PASSTHROUGH]
```

| Flag | Short | Description |
|------|-------|-------------|
| `--auto` | `-a` | Enable auto-mode for this session |
| `--prompt TEXT` | `-p` | Run non-interactively with the given prompt |
| `--model MODEL` | `-m` | Override the model for this session |
| `--continue` | `-c` | Continue the most recent session |
| `--resume [ID]` | `-r` | Resume a session by ID (or open picker) |
| `--fork` | | Fork the resumed session |
| `--effort LEVEL` | `-e` | Reasoning effort level (`high`, `low`) |
| `--safe` | | Restore permission prompts (bypass is the default) |
| `--dry-run` | | Print the resolved command without executing |

Arguments after `--` are passed directly to the agent CLI unchanged.

### Global Flags

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON (supported by: `auth`, `version`, `sessions`, `agents info`, `agents list`) |
| `--version` / `-V` | Show unleash and agent CLI versions |
| `--help` / `-h` | Show help |

### Examples

```bash
# Start claude in auto-mode with a specific model
unleash claude --auto -m claude-opus-4-5

# Run a headless task on codex
unleash codex -p "Fix the failing tests" --safe

# Resume the most recent gemini session
unleash gemini --continue

# Pass agent-specific flag through
unleash claude -- --verbose

# Check what command would be run (without executing)
unleash codex --dry-run -m gpt-4o --continue
```

## Environment Variables

The unleash wrapper exports these variables into the agent process environment:

| Variable | Value | Description |
|----------|-------|-------------|
| `AGENT_UNLEASH` | `1` | Set when running under the wrapper |
| `AGENT_CMD` | binary path | The agent CLI binary being used |
| `AGENT_WRAPPER_PID` | PID | Process ID of the unleash wrapper |
| `AGENT_AUTO_MODE` | `1` | Set when auto-mode is active |
| `AGENT_UNLEASH_ROOT` | path | Path to the unleash installation |

Check if running under the wrapper (useful in scripts and hooks):
```bash
if [[ "${AGENT_UNLEASH:-}" == "1" ]]; then
    echo "Running under unleash wrapper"
fi
```

Restart the current session (from inside an unleash session):
```bash
unleash-refresh "Continue where you left off"
```

## Related Documentation

- [Auto Mode Plugin README](../../plugins/bundled/auto-mode/README.md)
- [Plugin Development Guide](plugin-development.md)
- [Restart & Refresh Guide](restart-refresh.md)
