---
name: auto
description: Toggle autonomous mode - Claude continues working until owner contact or explicit exit
---

# AUTO MODE TOGGLE

First, check the current state and toggle it by running:

```bash
"${AGENT_UNLEASH_ROOT:-$HOME/unleash}/plugins/bundled/auto-mode/scripts/toggle-auto-mode.sh"
```

Run this command NOW. It will either activate or deactivate auto mode based on current state.

---

## If Auto Mode Was ACTIVATED:

You are now in **AUTO MODE**. A Stop hook will prompt you to check MCP tools before stopping.

### How It Works

1. **Stop hook guidance**: When you try to end your turn, the hook reminds you to:
   - Check MCP tools for pending tasks or owner messages
   - Proceed with implementation if owner asked "do you want to do X?"
   - Use sleep with exponential backoff if waiting for owner

2. **Decision making**: If the owner asked something like "do you want to implement X?" or "should we do Y?" - the answer is usually **YES**. Proceed with implementation.

3. **Waiting for owner**: If truly idle with no work, use Bash `sleep` command with exponential backoff:
   ```bash
   sleep 30   # First wait
   sleep 60   # Second wait
   sleep 120  # Third wait (double each time)
   ```

4. **Exit options**: The session can end when:
   - Owner says stop/quit/exit
   - Running `exit-claude`
   - Running `/auto` again to toggle off

### Behavior Guidelines

- **Be proactive**: If you see something that needs doing, do it
- **Use TodoWrite**: Track your work and pending tasks
- **Check MCP tools**: Signal server, omni-mcp may have tasks or messages
- **Self-restart if needed**: Use `restart-claude` to refresh context

---

## If Auto Mode Was DEACTIVATED:

You have exited auto mode. Normal operation resumes - you can end your turn when appropriate.

---

## After Running Toggle

Report the new state and:

1. **Sync visual indicator** (if patches installed):
   - If toggled ON: Tell user "Press shift+tab until you see yellow »» indicator"
   - If toggled OFF: Tell user "Press shift+tab to cycle away from auto mode"

2. If auto mode is now active:
   - List any MCP contact/notification tools found (or state none available)
   - State your current task status
   - Ask for work if nothing pending
