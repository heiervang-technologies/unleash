#!/usr/bin/env bash
# lifecycle-setup.sh - Called by launcher on wrapper startup
#
# Sets Hyprland window rules (float + transparency) and shows startup notification.
# Skipped automatically on non-Hyprland systems.

set -euo pipefail

# Skip if not running under Hyprland
[[ -z "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]] && exit 0

# Apply window rules for unleash windows
hyprctl --batch \
    "keyword windowrule float on, match:class ^(unleash)$ ; \
     keyword windowrule opacity 0.95 0.9, match:class ^(unleash)$" \
    2>/dev/null || true

# Startup notification
hyprctl notify 1 5000 0 "unleash started" 2>/dev/null || true
