#!/usr/bin/env bash
# splash.sh — Interactive agent picker splash screen for unleash installer
#
# Shows the ANSI mascot art recolored per agent with a cycleable input field.
# Arrow keys cycle through: claude, codex, gemini, opencode.
# Enter confirms, q/Esc quits.
#
# Usage: run_splash /path/to/mascot.claude.ans
#
# Colors match the TUI theme presets (see src/theme.rs):
#   claude   → orange  hue_shift=0     sat_scale=1   (identity)
#   codex    → grey    hue_shift=345.2 sat_scale=0   (desaturate)
#   gemini   → purple  hue_shift=260   sat_scale=1
#   opencode → blue    hue_shift=200   sat_scale=1

set -euo pipefail

# Agent definitions
AGENTS=(claude codex gemini opencode)
DISPLAY_NAMES=("Claude Code" "Codex" "Gemini CLI" "OpenCode")

# Accent colors (from TUI ThemePreset::accent_rgb)
ACCENT_R=(217 140 162 87)
ACCENT_G=(119 140 87  142)
ACCENT_B=(87  140 217 217)

# Hue shift per agent (same as theme.rs hue_shift for Orange, Custom grey, Purple, Blue)
HUE_SHIFTS=(0.0 345.23 260.0 200.0)
SAT_SCALES=(1.0 0.0 1.0 1.0)

RESET=$'\033[0m'

current=0
MASCOT_FILE=""
RECOLOR_SCRIPT=""
CACHE_DIR=""

fg_rgb() { printf '\033[38;2;%d;%d;%dm' "$1" "$2" "$3"; }
dim()    { printf '\033[2m'; }
bold()   { printf '\033[1m'; }

# Pre-render all 4 mascot color variants to temp files
prerender_mascots() {
    if [[ -z "$MASCOT_FILE" || ! -f "$MASCOT_FILE" ]]; then
        return
    fi
    CACHE_DIR=$(mktemp -d)
    if [[ -n "$RECOLOR_SCRIPT" && -f "$RECOLOR_SCRIPT" ]]; then
        for i in 0 1 2 3; do
            python3 "$RECOLOR_SCRIPT" "$MASCOT_FILE" "${HUE_SHIFTS[$i]}" "${SAT_SCALES[$i]}" > "${CACHE_DIR}/${i}.ans"
        done
    else
        for i in 0 1 2 3; do
            cp "$MASCOT_FILE" "${CACHE_DIR}/${i}.ans"
        done
    fi
}

# Render the mascot from cached temp file
render_mascot() {
    local idx=$1
    local cached="${CACHE_DIR}/${idx}.ans"
    if [[ -n "$CACHE_DIR" && -f "$cached" ]]; then
        cat "$cached"
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
    render_mascot "$current"
    echo ""
    render_input "$current"
    render_hint
}

# Main interactive loop
# Usage: run_splash [/path/to/mascot.ans]
run_splash() {
    MASCOT_FILE="${1:-}"
    # Locate the recolor helper next to this script
    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    RECOLOR_SCRIPT="${script_dir}/recolor.py"

    # Pre-render all color variants (takes ~200ms total)
    prerender_mascots

    # Hide cursor, save terminal state
    printf '\033[?25l'
    stty -echo -icanon min 1 time 0 2>/dev/null || true

    # Restore on exit and clean up temp files
    trap 'printf "\033[?25h"; stty echo icanon 2>/dev/null || true; [[ -n "${CACHE_DIR:-}" ]] && rm -rf "$CACHE_DIR"' EXIT

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
