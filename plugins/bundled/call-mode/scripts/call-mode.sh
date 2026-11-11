#!/usr/bin/env bash
# call-mode.sh [on|off|toggle|status] - machine-wide call mode switch.
# On: every agent on this machine phones the owner with its questions.
set -uo pipefail
FLAG="${CALL_MODE_FLAG:-${XDG_CONFIG_HOME:-$HOME/.config}/haios-ui/call-mode}"
case "${1:-toggle}" in
    on) mkdir -p "$(dirname "$FLAG")"; date -Is > "$FLAG" ;;
    off) rm -f "$FLAG" ;;
    toggle) if [[ -f "$FLAG" ]]; then rm -f "$FLAG"; else mkdir -p "$(dirname "$FLAG")"; date -Is > "$FLAG"; fi ;;
    status) ;;
    *) echo "usage: call-mode.sh [on|off|toggle|status]" >&2; exit 2 ;;
esac
if [[ -f "$FLAG" ]]; then echo "CALL MODE: ON (since $(cat "$FLAG"))"; else echo "CALL MODE: OFF"; fi
