#!/usr/bin/env bash
# call-mode-ask.sh - PreToolUse(AskUserQuestion): when call mode is on, phone
# the owner first. haios-holocall call rings the desk card and the Fay phone;
# once answered, the question router (askuserquestion-cinder.py) sees the live
# call, speaks the question into it and takes the spoken answer. This hook only
# places the call and never decides the tool call itself.
set -uo pipefail

FLAG="${CALL_MODE_FLAG:-${XDG_CONFIG_HOME:-$HOME/.config}/haios-ui/call-mode}"
RING_WAIT="${CALL_MODE_RING_SECONDS:-45}"

[[ -t 0 ]] || cat >/dev/null  # hook input is not needed
[[ -f "$FLAG" ]] || exit 0
command -v haios-holocall >/dev/null || exit 0
haios-holocall can-call >/dev/null 2>&1 || exit 0
[[ -n "${TMUX_PANE:-}" ]] || exit 0                    # the stick needs a pane
RUN="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

# One ring at a time: this hook and the question router (which runs it for
# agents started before the plugin) can fire for the same question.
exec 9>"$RUN/call-mode.lock"
flock -w "$RING_WAIT" 9 || exit 0
haios-holocall call-status >/dev/null 2>&1 && exit 0  # a call is already live
stamp="$RUN/call-mode-${TMUX_PANE#%}.rang"
# Rang this pane a moment ago (the other hook, or a declined call): don't redial.
[[ -n "$(find "$stamp" -newermt '-90 seconds' 2>/dev/null)" ]] && exit 0
touch "$stamp"

# Agent shells often lack the compositor env; without it the desk card dies at
# once and holocall reads the missing card as "declined".
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
    w=$(cd "$RUN" 2>/dev/null && ls -1 wayland-[0-9] 2>/dev/null | head -1)
    [[ -n "$w" ]] && export WAYLAND_DISPLAY="$w"
fi
if [[ -z "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    h=$(ls -1t "$RUN/hypr" 2>/dev/null | head -1)
    [[ -n "$h" ]] && export HYPRLAND_INSTANCE_SIGNATURE="$h"
fi

name="${AGENT_PERSONA:-$(tmux display-message -p -t "$TMUX_PANE" '#S' 2>/dev/null)}"
name="${name:-agent}"
log="$RUN/call-mode-${TMUX_PANE#%}.log"

# The call blocks for its whole length: detach it so it outlives this hook.
setsid haios-holocall call 9>&- --from "$name" --pane "$TMUX_PANE" \
    --text "Hei, ${name} here. I have a question for you." >"$log" 2>&1 < /dev/null &
pid=$!

# Hold the question until the call is answered, refused or times out, so the
# router asks it on the line rather than on a card nobody is looking at.
for _ in $(seq $((RING_WAIT * 5))); do
    haios-holocall call-status >/dev/null 2>&1 && break
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.2
done
exit 0
