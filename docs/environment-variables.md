# Environment Variables

## User-Controllable

These can be set before launching `unleash` to change behavior.

| Variable | Purpose | Default |
|----------|---------|---------|
| `UNLEASH_ANIMATIONS` | Enable TUI animations (lava color cycling) | `0` (off) |
| `AU_HYPRLAND_FOCUS` | Hyprland window transparency during agent work | `1` (on) |
| `HOOK_NO_SOUND` | Suppress notification sounds from hooks | unset |
| `EDITOR` / `VISUAL` | Editor for TUI text input fields | system default |

### Examples

```bash
# Enable lava animations in the TUI
UNLEASH_ANIMATIONS=1 unleash

# Disable Hyprland transparency
AU_HYPRLAND_FOCUS=0 unleash claude

# Silence hook sounds
HOOK_NO_SOUND=1 unleash claude
```

Animations can also be toggled persistently via `animations = true` in
`~/.config/unleash/config.toml` (see [configuration.md](configuration.md)).

## Set by Unleash (Read-Only)

These are set automatically when the wrapper launches an agent. Useful for
troubleshooting, plugin development, and scripts that need to detect the
runtime environment.

| Variable | Purpose |
|----------|---------|
| `AGENT_CMD` | Which agent binary is running (`claude`, `codex`, `gemini`, `opencode`) |
| `AGENT_UNLEASH` | Set to `1` when running under the unleash wrapper |
| `AGENT_WRAPPER_PID` | PID of the wrapper process (used by plugins and Hyprland focus) |
| `AGENT_UNLEASH_ROOT` | Path to the unleash installation directory |
| `UNLEASH_POLYFILL_ACTIVE` | Set to `1` when polyfill flag translation is active |
| `BASH_DEFAULT_TIMEOUT_MS` | Default bash timeout for agent tools (set to `999999999` ~11.5 days) |
| `BASH_MAX_TIMEOUT_MS` | Max bash timeout (set to `999999999`) |
| `MCP_TOOL_TIMEOUT` | MCP tool timeout (set to `999999999`) |
| `CODEX_HOME` | Override Codex home directory (read from environment if already set) |

### Timeout Variables

Unleash sets generous timeout defaults so long-running agent commands do not
get killed unexpectedly. These are only set if not already present in the
environment, so you can override them:

```bash
# Use a shorter bash timeout (5 minutes)
BASH_DEFAULT_TIMEOUT_MS=300000 unleash claude
```

### Detecting the Wrapper

Scripts and plugins can check whether they are running inside unleash:

```bash
if [ "$AGENT_UNLEASH" = "1" ]; then
  echo "Running under unleash (PID $AGENT_WRAPPER_PID)"
fi
```
