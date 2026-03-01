---
name: voice-wait
description: Wait for voice input with instant wakeup (replaces sleep in auto-mode)
allowed-tools: ["Bash(${CLAUDE_PLUGIN_ROOT}/scripts/unleash-wait:*)"]
---

# Voice Wait

Wait for voice input with instant wakeup capability. This replaces fixed sleep intervals in auto-mode for low-latency voice conversations.

## Wait with Timeout

```!
"${CLAUDE_PLUGIN_ROOT}/scripts/unleash-wait" --timeout 60
```

This will:
1. Return immediately if a message is already queued
2. Block until a message arrives via `unleash-queue --notify`
3. Timeout after 60 seconds if no message arrives

## Exit Codes

- **0**: Message available - check your pending messages
- **1**: Timeout reached - no message arrived
- **2**: Interrupted (Ctrl+C)

## Integration with Auto-Mode

Instead of using fixed sleep intervals:
```bash
sleep 30  # Old way - slow, unresponsive
```

Use voice-wait:
```bash
unleash-wait --timeout 60  # New way - instant response
```

When a voice message arrives via `unleash-queue --notify`, the wait returns immediately, enabling smooth voice conversations with minimal latency.

## After Receiving a Message

The omnihook will automatically inject the queued message into your session on the next hook event. You should see the message appear as a prompt.
