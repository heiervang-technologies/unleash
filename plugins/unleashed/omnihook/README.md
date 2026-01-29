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
2. Transcription calls `cu-queue --notify "transcribed text"`
3. Claude's `cu-wait` unblocks immediately
4. Message injected via next hook event
5. Near-instant response

## Installation

The plugin is automatically available when using agent-unleashed. The hooks register for all event types.

## CLI Tools

### cu-queue

Add messages to the omnihook queue:

```bash
# Queue a message for the current session
cu-queue "Please search for files containing auth"

# From voice transcription pipeline
echo "$TRANSCRIPTION" | cu-queue --stdin --notify

# Target a specific session
cu-queue --pid 12345 "Message for that session"

# Send to all active sessions
cu-queue --all "Attention all sessions"

# List active queues
cu-queue --list

# Clear the queue
cu-queue --clear
```

### cu-wait

Block until a message arrives:

```bash
# Wait with 60 second timeout
cu-wait --timeout 60

# Wait indefinitely
cu-wait

# Just check if messages exist
cu-wait --check

# Setup FIFO for this session
cu-wait --setup

# Cleanup when done
cu-wait --cleanup
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
    cu-queue --notify "message"
         |
         +---> Queue File (~/.cache/agent-unleashed/omnihook/queue-PID)
         |
         +---> FIFO (~/.cache/agent-unleashed/omnihook/fifo-PID)
                    |
                    v
              cu-wait unblocks
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
| `~/.cache/agent-unleashed/omnihook/queue-PID` | Message queue (JSON lines) |
| `~/.cache/agent-unleashed/omnihook/fifo-PID` | Named pipe for instant wakeup |
| `~/.cache/agent-unleashed/omnihook/lock-PID` | Lock file for atomic queue operations |

## Integration Example

### Voice Transcription Integration

```bash
#!/bin/bash
# Example: jessica-listen integration

# Start listening
jessica-listen --continuous | while read -r transcription; do
  # Queue the transcription with notification
  cu-queue --notify "${transcription}"
done
```

### Auto-Mode Integration

```bash
# In auto-mode loop, instead of:
sleep 30

# Use:
cu-wait --timeout 60
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
  "source": "cu-queue"
}
```

The omnihook handler extracts the `text` field for injection.

## Debugging

```bash
# Check what's in the queue
cat ~/.cache/agent-unleashed/omnihook/queue-*

# Check if FIFO exists
ls -la ~/.cache/agent-unleashed/omnihook/fifo-*

# Test queue manually
CLAUDE_WRAPPER_PID=$$ cu-queue "Test message"
CLAUDE_WRAPPER_PID=$$ cu-wait --check
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `CLAUDE_WRAPPER_PID` | Session identifier (set by agent-unleashed wrapper) |
| `HOOK_EVENT` | Hook event type (set by hooks.json) |

## Limitations

- Requires running under agent-unleashed wrapper (CLAUDE_WRAPPER_PID must be set)
- FIFO-based wakeup requires the FIFO to be set up first
- Messages are processed one at a time on each hook event

## Version History

- 1.0.0: Initial implementation with queue, FIFO, and all hook types
