---
name: auto
description: Toggle autonomous mode - Claude continues working until owner contact or explicit exit
---

# AUTO MODE TOGGLE

First, check the current state and toggle it by running:

```bash
~/claude-unleashed/plugins/unleashed/auto-mode/scripts/toggle-auto-mode.sh
```

Run this command NOW. It will either activate or deactivate auto mode based on current state.

---

## If Auto Mode Was ACTIVATED:

You are now in **AUTO MODE**. A Stop hook will **block you from ending your turn**.

### Core Rules

1. **No voluntary exit**: You may NOT end your session on your own initiative. The ONLY ways to exit are:
   - Running `exit-claude` (terminates the session)
   - The owner explicitly tells you to stop/quit/exit
   - Running `/auto` again to toggle off

2. **Continuous operation**: After completing each task, actively look for more work:
   - Check pending tasks in your todo list
   - Look for incomplete work from earlier
   - If truly idle, wait for owner input

3. **Owner contact**: Check if any MCP servers provide tools for contacting/notifying the owner. If found, use them to request clarification or report progress.

### Behavior Guidelines

- **Be proactive**: Don't wait to be asked - if you see something that needs doing, do it
- **Use TodoWrite**: Track your work and pending tasks
- **Self-restart if needed**: Use `restart-claude` if you need to refresh context
- **Stay focused**: Complete current work before starting new work

---

## If Auto Mode Was DEACTIVATED:

You have exited auto mode. Normal operation resumes - you can end your turn when appropriate.

---

## After Running Toggle

Report the new state and, if auto mode is now active:
1. List any MCP contact/notification tools found (or state none available)
2. State your current task status
3. Ask for work if nothing pending
