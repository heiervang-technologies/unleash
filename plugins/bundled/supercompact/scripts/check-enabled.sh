#!/usr/bin/env bash
# check-enabled.sh — exit 0 if supercompact is enabled in unleash config,
# 1 otherwise. Used by hook scripts as a self-disable guard against stale
# registrations in ~/.claude/settings.json that the wrapper failed to prune.
#
# Empty enabled_plugins list = "all enabled" (backwards-compat semantics).

set -uo pipefail

PLUGIN_NAME="${1:-supercompact}"
UNLEASH_CONFIG="${HOME}/.config/unleash/config.toml"

# No config = treat as all enabled (first run, before TUI written anything).
[[ ! -f "${UNLEASH_CONFIG}" ]] && exit 0

# Extract the enabled_plugins array body.
enabled_block=$(awk '
  /^[[:space:]]*enabled_plugins[[:space:]]*=[[:space:]]*\[/ { in_block=1 }
  in_block { print }
  in_block && /\]/ { exit }
' "${UNLEASH_CONFIG}")

# No enabled_plugins key at all = treat as all enabled.
[[ -z "${enabled_block}" ]] && exit 0

# Block exists but contains no quoted entries = empty list = all enabled.
if ! grep -q '"' <<<"${enabled_block}"; then
  exit 0
fi

# Block has entries — must contain our plugin name explicitly.
grep -q "\"${PLUGIN_NAME}\"" <<<"${enabled_block}"
