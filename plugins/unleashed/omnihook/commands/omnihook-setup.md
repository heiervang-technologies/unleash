---
name: omnihook-setup
description: Initialize omnihook FIFO for this session (enables instant message wakeup)
allowed-tools: ["Bash(${CLAUDE_PLUGIN_ROOT}/scripts/au-wait:*)"]
---

# Omnihook Setup

Initialize the omnihook system for this session. This creates a FIFO (named pipe) that enables instant wakeup when voice messages arrive.

Run the setup command:

```!
"${CLAUDE_PLUGIN_ROOT}/scripts/au-wait" --setup
```

After setup, the omnihook is ready to receive messages via `au-queue`.

## How It Works

1. **FIFO Created**: A named pipe is created for this session
2. **Hooks Active**: The omnihook handler checks for messages on every hook event
3. **Instant Wakeup**: When `au-queue --notify` is called, any waiting `au-wait` unblocks immediately

## Integration with Voice

When using Jessica (voice assistant):
1. User speaks via voice input
2. Transcription calls `au-queue --notify "transcribed text"`
3. If Claude is in `au-wait`, it wakes up immediately
4. The next hook event picks up the queued message

## Cleanup

When done, run `/omnihook-cleanup` to remove the FIFO and queue files.
