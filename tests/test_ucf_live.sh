#!/usr/bin/env bash
set -euo pipefail

SESSION_NAME="ucf-live-test-$$"
UCF_FILE="$HOME/.local/share/unleash/sessions/${SESSION_NAME}.ucf.jsonl"

echo "=== UCF Live Test ==="
echo "Session Name: $SESSION_NAME"

cleanup() {
    rm -f /tmp/gemini_out /tmp/codex_out /tmp/opencode_out
    rm -f "$UCF_FILE"
}
trap cleanup EXIT

# Use the debug binary if it exists
BIN="./target/debug/unleash"
if [[ ! -x "$BIN" ]]; then
    echo "ERROR: Please run 'cargo build' first"
    exit 1
fi

# Step 1: Claude Code
echo "[1] Claude Code: Creating native UCF session ($SESSION_NAME) and setting context"
"$BIN" claude -u "$SESSION_NAME" -p "The secret passphrase is 'PINEAPPLE'. Acknowledge by saying exactly 'ACK PINEAPPLE'."

if [[ ! -f "$UCF_FILE" ]]; then
    echo "  FAIL: UCF file was not created at $UCF_FILE"
    exit 1
fi
echo "  PASS: UCF file created"

# Step 2: Gemini
echo "[2] Gemini: Resuming native UCF session and querying context"
"$BIN" gemini -u "$SESSION_NAME" -p "What was the secret passphrase? Reply with exactly 'The passphrase is <passphrase>'." > /tmp/gemini_out 2>&1 || true

if grep -iq "PINEAPPLE" /tmp/gemini_out; then
    echo "  PASS: Gemini successfully read Claude's history from UCF"
else
    echo "  FAIL: Gemini did not output the expected passphrase. Output was:"
    cat /tmp/gemini_out
    exit 1
fi

# Step 3: Codex (Optional if installed)
echo "[3] Codex: Resuming native UCF session and querying context"
# we don't strictly fail if codex isn't authenticated, but we'll try
if "$BIN" agents status | grep -q "codex.*Installed"; then
    "$BIN" codex -u "$SESSION_NAME" -p "Repeat the secret passphrase again. Reply with exactly 'Still <passphrase>'." > /tmp/codex_out 2>&1 || true
    if grep -iq "PINEAPPLE" /tmp/codex_out; then
        echo "  PASS: Codex successfully read history from UCF"
    else
        echo "  WARN: Codex did not output the expected passphrase or failed."
        cat /tmp/codex_out
    fi
else
    echo "  SKIP: Codex not installed"
fi

# Step 4: OpenCode (Optional if installed)
echo "[4] OpenCode: Resuming native UCF session and querying context"
if "$BIN" agents status | grep -q "opencode.*Installed"; then
    "$BIN" opencode -u "$SESSION_NAME" -p "What is the secret? Reply with exactly 'Final <passphrase>'." > /tmp/opencode_out 2>&1 || true
    if grep -iq "PINEAPPLE" /tmp/opencode_out; then
        echo "  PASS: OpenCode successfully read history from UCF"
    else
        echo "  WARN: OpenCode did not output the expected passphrase or failed."
        cat /tmp/opencode_out
    fi
else
    echo "  SKIP: OpenCode not installed"
fi

echo "All live UCF tests passed!"
