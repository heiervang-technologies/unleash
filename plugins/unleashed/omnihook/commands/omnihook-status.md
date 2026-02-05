---
name: omnihook-status
description: Check omnihook queue status and pending messages
allowed-tools: ["Bash(${CLAUDE_PLUGIN_ROOT}/scripts/au-queue:*)", "Bash(${CLAUDE_PLUGIN_ROOT}/scripts/au-wait:*)"]
---

# Omnihook Status

Check the current status of the omnihook system.

## Queue Status

```!
"${CLAUDE_PLUGIN_ROOT}/scripts/au-queue" --list
```

## Check for Pending Messages

```!
"${CLAUDE_PLUGIN_ROOT}/scripts/au-wait" --check
```

## Understanding the Output

- **Queue files**: Show messages waiting to be injected into sessions
- **FIFO files**: Show which sessions have set up instant wakeup capability
- **Message count**: Number of messages waiting in each queue

## Clearing the Queue

To clear pending messages:
```bash
au-queue --clear  # Clear this session's queue
```
