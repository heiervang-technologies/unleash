#!/usr/bin/env bash
set -eo pipefail

echo "Running docs drift check..."

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$REPO_ROOT"

# Check for deprecated binary names
DEPRECATED="(unleashi|unleashg|unleashtx|claude_unleashed_version|\\bunleashed\\b)"
FILES_TO_CHECK="scripts/install-remote.sh scripts/install.sh README.md CLAUDE.md docs/JSON_OUTPUT.md docs/auth-check-command.md"

FAILED=0

for file in $FILES_TO_CHECK; do
    if [[ -f "$file" ]]; then
        if grep -qE "$DEPRECATED" "$file"; then
            echo "Error: Found deprecated terms in $file"
            grep -nE "$DEPRECATED" "$file"
            FAILED=1
        fi
    fi
done

# Check if unleash agents --help output still mentions Aider instead of Gemini
# Store the output first to fail if the command itself fails
AGENTS_HELP=$(cargo run --profile fast --bin unleash -- agents --help 2>/dev/null) || {
    echo "Error: Failed to run cargo run --profile fast --bin unleash -- agents --help"
    FAILED=1
}

if [[ $FAILED -eq 0 ]] && echo "$AGENTS_HELP" | grep -qi "Aider"; then
    echo "Error: CLI help mentions Aider. Update to current supported agents."
    FAILED=1
fi

if [[ $FAILED -eq 1 ]]; then
    echo "Docs drift check failed."
    exit 1
fi

echo "Docs drift check passed."
exit 0
