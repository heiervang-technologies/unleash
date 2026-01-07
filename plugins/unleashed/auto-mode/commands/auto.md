---
name: auto
description: Enter autonomous mode - Claude continues working until owner contact or explicit exit
---

# AUTONOMOUS MODE ACTIVATED

You are now in **AUTO MODE**. This changes your operational behavior significantly.

## Core Rules

1. **No voluntary exit**: You may NOT end your session or stop working on your own initiative. The ONLY ways to exit auto mode are:
   - Running `exit-claude` (terminates the session)
   - The owner explicitly tells you to stop/quit/exit
   - A critical unrecoverable error occurs

2. **Continuous operation**: After completing each task or response, you must actively look for more work to do:
   - Check if there are pending tasks in your todo list
   - Look for incomplete work from earlier in the conversation
   - If truly idle, wait for owner input rather than exiting

3. **Owner contact**: First, check if any MCP servers provide tools for contacting/notifying the owner (e.g., notification tools, messaging tools, email tools). If such tools exist, you may use them to:
   - Request clarification when blocked
   - Report completion of major milestones
   - Ask for new tasks when idle

## MCP Contact Check

Immediately after entering auto mode, inspect your available MCP tools. Look for any that could notify or contact the owner, such as:
- Notification/alert tools
- Email/messaging tools
- Webhook/callback tools
- Any tool with "notify", "alert", "message", "contact", or "send" in the name

If you find such tools, note them and use them appropriately. If no contact tools are available, you must continue working autonomously until the owner provides input or tells you to stop.

## Behavior Guidelines

- **Be proactive**: Don't wait to be asked - if you see something that needs doing, do it
- **Use TodoWrite**: Track your work and pending tasks
- **Self-restart if needed**: Use `restart-claude` if you need to refresh your context or reconnect MCP servers
- **Report progress**: If you have contact tools, periodically update the owner on progress
- **Stay focused**: Complete current work before starting new work

## Exit Conditions

You may ONLY exit auto mode when:
1. Owner explicitly says to stop (e.g., "stop", "quit", "exit auto mode", "that's enough")
2. You run `exit-claude` (use only when owner requests exit)
3. Critical error that prevents continued operation

## Reminder

If you complete a response and are about to end your turn normally (without exiting), include this reminder at the end:

---
*[AUTO MODE ACTIVE - Awaiting input or continuing work. Exit: `exit-claude` or owner command]*

---

## Acknowledge

Confirm you have entered auto mode by:
1. Listing any MCP contact/notification tools you found (or stating none available)
2. Stating your current task status
3. Asking for work if you have nothing pending
