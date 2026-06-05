# Omnihook Plugin

Universal hook system for low-latency voice integration with Claude Code.

## Overview

The omnihook plugin enables smooth, low-latency voice conversations with Claude by:

1. **Triggering on ALL hook events** - Not just Stop, but PreToolUse, PostToolUse, SessionStart, and Notification
2. **Maintaining a message queue** - External tools can queue messages for Claude
3. **Instant wakeup via FIFO** - No polling with fixed sleep intervals
4. **Automatic message injection** - Queued messages are injected into the session immediately

## Use Case

When using Jessica (voice assistant) with Claude Code:

**Before (with fixed sleep):**
1. User speaks via voice input
2. Transcription completes
3. Claude is sleeping for 30 seconds...
4. 28 seconds later, Claude wakes up
5. User frustrated by delay

**After (with omnihook):**
1. User speaks via voice input
2. Transcription pipeline writes to the queue via the plugin's
   `unleash-queue` script (path: `${CLAUDE_PLUGIN_ROOT}/scripts/unleash-queue`)
3. Claude's `/voice-wait` invocation of `unleash-wait` unblocks immediately
4. Message injected via next hook event
5. Near-instant response

## Installation

Bundled with unleash and enabled by default. The wrapper auto-syncs
the hook registrations (Stop / PreToolUse / PostToolUse /
SessionStart / Notification) into `~/.claude/settings.json` via
`HookManager::sync_plugin_hooks` (`src/launcher.rs`). No manual
configuration required.

To disable, exclude `omnihook` from the `enabled_plugins` allowlist
in `~/.config/unleash/config.toml`, or toggle it off in the **Plugins**
tab in the TUI.

## Plugin Scripts

These are *plugin-internal* scripts, NOT globally-installed CLI tools.
They live at `plugins/bundled/omnihook/scripts/` and are invoked
either through the slash commands below or directly from external
pipelines via `${CLAUDE_PLUGIN_ROOT}/scripts/<name>` (Claude Code
substitutes `CLAUDE_PLUGIN_ROOT` at hook/command time).

If you want to call them from outside Claude Code (e.g. a voice
transcription daemon), resolve the path yourself:

```bash
OMNIHOOK="$HOME/.local/share/unleash/plugins/omnihook/scripts"   # installed layout
# or
OMNIHOOK="$(git rev-parse --show-toplevel)/plugins/bundled/omnihook/scripts"  # in-tree
```

### unleash-queue

Add messages to the omnihook queue:

```bash
# Queue a message for the current session
"$OMNIHOOK/unleash-queue" "Please search for files containing auth"

# From voice transcription pipeline
echo "$TRANSCRIPTION" | "$OMNIHOOK/unleash-queue" --stdin --notify

# Target a specific session
"$OMNIHOOK/unleash-queue" --pid 12345 "Message for that session"

# Send to all active sessions
"$OMNIHOOK/unleash-queue" --all "Attention all sessions"

# List active queues
"$OMNIHOOK/unleash-queue" --list

# Clear the queue
"$OMNIHOOK/unleash-queue" --clear
```

### unleash-wait

Block until a message arrives:

```bash
# Wait with 60 second timeout
"$OMNIHOOK/unleash-wait" --timeout 60

# Wait indefinitely
"$OMNIHOOK/unleash-wait"

# Just check if messages exist
"$OMNIHOOK/unleash-wait" --check

# Setup FIFO for this session
"$OMNIHOOK/unleash-wait" --setup

# Cleanup when done
"$OMNIHOOK/unleash-wait" --cleanup
```

## Slash Commands

- `/omnihook-setup` - Initialize the FIFO for instant wakeup
- `/omnihook-cleanup` - Remove FIFO and queue files
- `/omnihook-status` - Check queue status and pending messages
- `/voice-wait` - Wait for voice input with instant wakeup

## Architecture

```
External Tool (jessica-listen)
         |
         v
    unleash-queue --notify "message"
         |
         +---> Queue File (~/.cache/unleash/omnihook/queue-PID)
         |
         +---> FIFO (~/.cache/unleash/omnihook/fifo-PID)
                    |
                    v
              unleash-wait unblocks
                    |
                    v
         Claude resumes work
                    |
                    v
         Hook event triggers
                    |
                    v
         omnihook-handler.sh reads queue
                    |
                    v
         Message injected into session
```

## Hook Event Behavior

| Hook Event | Behavior |
|------------|----------|
| Stop | Can block exit and inject message as "reason" |
| PreToolUse | Injects message as prompt |
| PostToolUse | Injects message as prompt |
| SessionStart | Injects message as initial prompt |
| Notification | Injects message as prompt |

## Files

| Path | Purpose |
|------|---------|
| `~/.cache/unleash/omnihook/queue-PID` | Message queue (JSON lines) |
| `~/.cache/unleash/omnihook/fifo-PID` | Named pipe for instant wakeup |
| `~/.cache/unleash/omnihook/lock-PID` | Lock file for atomic queue operations |

## Integration Example

### Voice Transcription Integration

```bash
#!/bin/bash
# Example: jessica-listen integration
# Resolve the plugin scripts dir (installed layout shown; adjust for in-tree)
OMNIHOOK="$HOME/.local/share/unleash/plugins/omnihook/scripts"

# Start listening
jessica-listen --continuous | while read -r transcription; do
  # Queue the transcription with notification
  "$OMNIHOOK/unleash-queue" --notify "${transcription}"
done
```

### Auto-Mode Integration

```bash
# In auto-mode loop, instead of:
sleep 30

# Use (path resolved as above):
"$OMNIHOOK/unleash-wait" --timeout 60
if [[ $? -eq 0 ]]; then
  echo "Voice message received!"
fi
```

## Message Format

Messages in the queue are JSON objects:

```json
{
  "text": "The transcribed voice message",
  "timestamp": "2026-01-23T15:30:00Z",
  "source": "unleash-queue"
}
```

The omnihook handler extracts the `text` field for injection.

## Debugging

```bash
# Check what's in the queue
cat ~/.cache/unleash/omnihook/queue-*

# Check if FIFO exists
ls -la ~/.cache/unleash/omnihook/fifo-*

# Test queue manually (resolve $OMNIHOOK as in the Integration Example above)
AGENT_WRAPPER_PID=$$ "$OMNIHOOK/unleash-queue" "Test message"
AGENT_WRAPPER_PID=$$ "$OMNIHOOK/unleash-wait" --check
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `AGENT_WRAPPER_PID` | Session identifier (set by unleash wrapper) |
| `HOOK_EVENT` | Hook event type (set by hooks.json) |

## Limitations

- Requires running under unleash wrapper (AGENT_WRAPPER_PID must be set)
- FIFO-based wakeup requires the FIFO to be set up first
- Messages are processed one at a time on each hook event

## Version History

- 1.0.0: Initial implementation with queue, FIFO, and all hook types
