---
name: omnihook-cleanup
description: Clean up omnihook FIFO and queue files for this session
allowed-tools: ["Bash(${CLAUDE_PLUGIN_ROOT}/scripts/unleash-wait:*)"]
---

# Omnihook Cleanup

Remove the omnihook FIFO and queue files for this session.

```!
"${CLAUDE_PLUGIN_ROOT}/scripts/unleash-wait" --cleanup
```

This should be run when ending a voice-enabled session to clean up resources.
