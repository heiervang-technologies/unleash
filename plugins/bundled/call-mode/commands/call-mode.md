---
name: call-mode
description: Toggle call mode - agents phone the owner with their questions instead of leaving cards
---

# CALL MODE

Run (argument: on, off, toggle or status; default toggle):

```bash
"${CLAUDE_PLUGIN_ROOT}/scripts/call-mode.sh" $ARGUMENTS
```

Report the new state in one line.

## While call mode is ON

The switch is machine-wide: every agent here follows it.

- Questions for the owner go through `AskUserQuestion` as usual. The plugin
  rings the owner (`haios-holocall call`: desk card and the Fay phone) before
  the question is asked, and the question router speaks it on the line and
  takes the spoken answer.
- Once answered you hold the talking stick: the owner's speech comes to you
  as turns, and your `say` is spoken on the call. Ask follow-ups one at a time
  in plain speech.
- When every question is answered, say a short goodbye, hang up with
  `haios-holocall hangup`, and carry on with the work.
- Declined or missed: the call becomes a message and the card stays up;
  continue with whatever does not need the answer.
