# Bundled Plugins

Unleash ships with plugins in `plugins/bundled/`, loaded via `--plugin-dir`
when launching an agent.

## Plugin Index

| Plugin | What it does | Config |
|--------|-------------|--------|
| **auto-mode** | Autonomous operation -- agent keeps working without user prompts | `--auto` / `-a` flag, or `defaults.auto = true` in profile |
| **process-restart** | Restart agent while preserving session state | `unleash-refresh` command |
| **mcp-refresh** | Detect MCP config changes and notify for reload | Automatic via PreToolUse hook |
| **hyprland-focus** | Window transparency while agent works (Hyprland only) | `AU_HYPRLAND_FOCUS=0` to disable |
| **omnihook** | Unified hook handler with voice input integration and FIFO wakeup | Automatic |
| **supercompact** | Entity-preservation conversation compaction (`/compact` replacement) | Plugins tab in TUI; method = `eitf` / `setcover` / `dedup` |
| **token-usage** | Centralized token-usage log across all agent CLIs | Plugins tab in TUI; per-method toggles |

## Plugin Descriptions

### auto-mode

Keeps the agent running autonomously by intercepting the stop hook and
re-prompting. Enable with the `--auto` flag or set `defaults.auto = true` in
your profile TOML.

### process-restart

Provides the `unleash-refresh` command that restarts the agent process while
preserving the current session (`--continue`). Useful after config changes or
when MCP servers need reloading.

### mcp-refresh

Watches `.mcp.json` for changes on each PreToolUse hook invocation. When a
change is detected, it notifies the agent to reload MCP servers.

### hyprland-focus

On Hyprland desktops, dims the terminal window while the agent is working and
restores opacity when it needs input. Disable with `AU_HYPRLAND_FOCUS=0`.

### omnihook

Unified hook handler that combines multiple hook functions into a single
entry point. Manages a FIFO queue for instant message wakeup in auto-mode,
replacing sleep-based polling.

### supercompact

Drop-in replacement for `/compact` that runs entity-preservation scoring
(EITF / set-cover / dedup) instead of summarization. ~400× faster than the
default summarizer and retains roughly twice as many distinct entities
across a single compaction. Pick the scoring method from the Plugins tab.

### token-usage

Centralizes token-usage data from every agent CLI into a single append-only
log at `~/.local/share/unleash/token-usage.jsonl`. Multiple collection
methods (Stop-hook session tail for live Claude data, on-demand session
scan for other CLIs) are independently toggleable in the Plugins tab.

## Creating Custom Plugins

Claude Code plugins are config + scripts (not Node.js modules). A typical
layout:

```
plugins/my-plugin/
├── .claude-plugin/
│   └── plugin.json      # Manifest (Claude Code reads from here)
├── commands/            # Slash commands (*.md files), optional
├── hooks/               # Lifecycle hooks, optional
│   ├── hooks.json       # Event → script mapping
│   └── *.sh             # Hook scripts (bash, python, anything executable)
├── scripts/             # Helper scripts called by hooks/commands, optional
└── README.md
```

For full details on the plugin API, hook event types, and tested patterns
see [Plugin Development Guide](internal/claude-code/plugin-development.md).

## Loading Plugins

Bundled plugins under `plugins/bundled/` are discovered automatically.
Unleash also picks up user plugins from `~/.local/share/unleash/plugins/`
and (with `--plugin-dir`) from any extra directory you point it at:

```bash
unleash claude --plugin-dir /path/to/my-plugins
```
