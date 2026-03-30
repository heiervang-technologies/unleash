# Hyprland Focus Plugin

Visual focus indicator for Hyprland window manager. Makes the terminal window transparent while the agent is working and restores opacity with a notification sound when it stops.

## What It Does

- **On prompt submit:** Sets the terminal window to transparent so you can see it's working
- **On stop:** Restores full opacity and plays a notification sound (`idle.wav`)

Requires Hyprland. Skipped automatically on non-Hyprland systems.

## Hooks

| Event | Action |
|-------|--------|
| `UserPromptSubmit` | Set window transparency via `hypr-window-opacity.sh set` |
| `Stop` | Restore opacity via `hypr-window-opacity.sh reset`, play sound |

## Configuration

Disable with environment variable:

```bash
export AU_HYPRLAND_FOCUS=0
```

## Audio

The stop sound plays via PipeWire (`pw-play`), PulseAudio (`paplay`), or SoX (`play`) — whichever is available. Set `HOOK_NO_SOUND=1` to suppress.
