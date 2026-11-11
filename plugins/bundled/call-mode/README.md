# Call Mode Plugin

When call mode is on, an agent that has questions for the owner phones them
(`haios-holocall call`, which rings the desk card and the Fay phone) and asks
in a live conversation, instead of leaving a Cinder card or an ears
quick-time event. When the questions are answered the agent hangs up and
continues.

## Usage

```
/call-mode            # toggle
/call-mode on|off|status
```

The switch is one machine-wide flag file, `~/.config/haios-ui/call-mode`
(override with `CALL_MODE_FLAG`), so it applies to every agent on the machine.

## How it works

1. `hooks/call-mode-ask.sh` runs on `PreToolUse` for `AskUserQuestion`. With
   the flag set and no call live, it starts `haios-holocall call --from
   <persona> --pane $TMUX_PANE` detached and waits (up to
   `CALL_MODE_RING_SECONDS`, 45) for it to be answered or refused.
2. The question router hook (`askuserquestion-cinder.py`) sees the live call,
   speaks the question and options into it and takes the spoken answer; the
   Cinder card stays as the fallback.
3. The answered call hands the agent the talking stick, so the conversation
   can continue past the card. The agent hangs up with `haios-holocall hangup`.

The hook never denies or allows the tool call, so with call mode off (or no
holocall on the machine) it is a no-op.

Agent shells often lack `WAYLAND_DISPLAY`; the hook fills it in, because
without it the desk card exits at once and holocall counts that as declined.
