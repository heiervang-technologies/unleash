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

## Creating Custom Plugins

Plugins live in their own directory with a standard structure:

```
plugins/my-plugin/
├── plugin.json          # Manifest (name, version, hooks)
├── index.js             # Main entry point
└── README.md            # Documentation
```

For full details on the plugin API, see
[docs/extensions/plugin-development.md](extensions/plugin-development.md).

## Loading Plugins

Bundled plugins are loaded automatically. To add a custom plugin directory:

```bash
unleash claude --plugin-dir /path/to/my-plugins
```
