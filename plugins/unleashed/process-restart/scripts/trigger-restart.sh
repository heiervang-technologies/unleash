#!/usr/bin/env bash
# Trigger restart script
#
# This script is called by the /restart command to create a restart trigger file.
# The Stop hook will detect this file and initiate the restart process.

set -euo pipefail

# Configuration
CACHE_DIR="${HOME}/.cache/claude-unleashed/process-restart"
TRIGGER_FILE="${CACHE_DIR}/restart-trigger"

# Parse command line arguments
FORCE=false
CLEAN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)
      FORCE=true
      shift
      ;;
    --clean)
      CLEAN=true
      shift
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Create cache directory
mkdir -p "${CACHE_DIR}"

# If clean restart, remove any existing state file
if [[ "${CLEAN}" == "true" ]]; then
  rm -f "${CACHE_DIR}/restart-state.json"
fi

# Create trigger file
touch "${TRIGGER_FILE}"

# Output confirmation message
if [[ "${FORCE}" == "true" ]]; then
  echo "🔄 Restart triggered (forced)"
else
  echo "🔄 Restart triggered"
fi

if [[ "${CLEAN}" == "true" ]]; then
  echo "   Clean restart: Session state will NOT be preserved"
else
  echo "   Session will be preserved and restored automatically"
fi

echo ""
echo "Exiting current Claude Code process..."
echo "New process will start automatically."

# The actual exit will be handled by Claude Code's exit command
# The Stop hook will detect the trigger file and handle the restart

exit 0
