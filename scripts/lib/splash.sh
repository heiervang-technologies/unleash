#!/usr/bin/env bash
# splash.sh — Interactive agent picker splash screen for unleash installer
#
# Shows the ANSI mascot art with a cycleable input field.
# Arrow keys cycle through: claude, codex, gemini, opencode.
# Enter confirms, q/Esc quits.
#
# Usage: run_splash /path/to/mascot.claude.ans
#
# Colors match the TUI theme presets:
#   claude   → orange (217, 119, 87)
#   codex    → grey   (140, 140, 140)
#   gemini   → purple (162, 87, 217)
#   opencode → blue   (87, 142, 217)

set -euo pipefail

# Agent definitions
AGENTS=(claude codex gemini opencode)
DISPLAY_NAMES=("Claude Code" "Codex" "Gemini CLI" "OpenCode")

# Accent colors (from TUI ThemePreset::accent_rgb)
ACCENT_R=(217 140 162 87)
ACCENT_G=(119 140 87  142)
ACCENT_B=(87  140 217 217)

RESET=$'\033[0m'

current=0
MASCOT_FILE=""

fg_rgb() { printf '\033[38;2;%d;%d;%dm' "$1" "$2" "$3"; }
dim()    { printf '\033[2m'; }
bold()   { printf '\033[1m'; }

# Render the mascot from the ANSI art file
render_mascot() {
    if [[ -n "$MASCOT_FILE" && -f "$MASCOT_FILE" ]]; then
        cat "$MASCOT_FILE"
    fi
}

# Render the input field
render_input() {
    local idx=$1
    local agent=${AGENTS[$idx]}
    local ar=${ACCENT_R[$idx]} ag=${ACCENT_G[$idx]} ab=${ACCENT_B[$idx]}
    local accent
    accent=$(fg_rgb "$ar" "$ag" "$ab")

    local text="unleash ${agent}"
    local inner_width=32
    local text_len=${#text}
    local pad=$((inner_width - text_len - 3))  # 3 for "❯ " prefix + space
    local padding
    padding=$(printf '%*s' "$pad" '')

    # Rounded box using Unicode box-drawing
    local top mid bot
    top="  ${accent}╭$(printf '─%.0s' $(seq 1 $inner_width))╮${RESET}"
    mid="  ${accent}│${RESET} ${accent}❯${RESET} $(bold)${accent}${text}${RESET}${padding} ${accent}│${RESET}"
    bot="  ${accent}╰$(printf '─%.0s' $(seq 1 $inner_width))╯${RESET}"

    echo -e "$top"
    echo -e "$mid"
    echo -e "$bot"
}

render_hint() {
    echo ""
    echo -e "  $(dim)←/→ cycle agents    Enter confirm    q quit${RESET}"
}

render_title() {
    local idx=$1
    local ar=${ACCENT_R[$idx]} ag=${ACCENT_G[$idx]} ab=${ACCENT_B[$idx]}
    local accent
    accent=$(fg_rgb "$ar" "$ag" "$ab")

    echo -e "  $(bold)${accent}unleash${RESET}  $(dim)installer${RESET}"
    echo ""
}

# Full render
render() {
    # Move cursor to top-left and clear
    printf '\033[H\033[2J'

    echo ""
    render_title "$current"
    render_mascot
    echo ""
    render_input "$current"
    render_hint
}

# Main interactive loop
# Usage: run_splash [/path/to/mascot.ans]
run_splash() {
    MASCOT_FILE="${1:-}"

    # Hide cursor, save terminal state
    printf '\033[?25l'
    stty -echo -icanon min 1 time 0 2>/dev/null || true

    # Restore on exit
    trap 'printf "\033[?25h"; stty echo icanon 2>/dev/null || true' EXIT

    render

    while true; do
        # Read a single byte
        local key
        IFS= read -rsn1 key

        case "$key" in
            q|Q)
                printf '\033[H\033[2J'
                echo "Installation cancelled."
                exit 0
                ;;
            $'\x1b')
                # Escape sequence — read next 2 chars
                local seq
                IFS= read -rsn2 seq
                case "$seq" in
                    '[C'|'[B')  # Right or Down arrow
                        current=$(( (current + 1) % ${#AGENTS[@]} ))
                        render
                        ;;
                    '[D'|'[A')  # Left or Up arrow
                        current=$(( (current - 1 + ${#AGENTS[@]}) % ${#AGENTS[@]} ))
                        render
                        ;;
                    *)
                        # Bare Escape (no sequence) — quit
                        if [[ -z "$seq" ]]; then
                            printf '\033[H\033[2J'
                            echo "Installation cancelled."
                            exit 0
                        fi
                        ;;
                esac
                ;;
            '')  # Enter
                # Return the selected agent
                printf '\033[H\033[2J'
                return 0
                ;;
        esac
    done
}

# Export selected agent for the installer
get_selected_agent() {
    echo "${AGENTS[$current]}"
}

get_selected_display() {
    echo "${DISPLAY_NAMES[$current]}"
}

# If run directly (not sourced), execute the splash
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    run_splash "${1:-}"
    echo "Selected: $(get_selected_display) ($(get_selected_agent))"
fi
