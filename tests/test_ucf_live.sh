#!/usr/bin/env bash
# MANUAL INTEGRATION TEST — requires installed CLIs with API keys.
# Not suitable for CI. Run locally to validate UCF crossload end-to-end.
set -euo pipefail

SESSION_NAME="ucf-live-test-$$"
UCF_FILE="$HOME/.local/share/unleash/sessions/${SESSION_NAME}.ucf.jsonl"
GEMINI_OUT=$(mktemp /tmp/gemini_out.XXXXXX)
CODEX_OUT=$(mktemp /tmp/codex_out.XXXXXX)
OPENCODE_OUT=$(mktemp /tmp/opencode_out.XXXXXX)

echo "=== UCF Live Test ==="
echo "Session Name: $SESSION_NAME"

cleanup() {
    rm -f "$GEMINI_OUT" "$CODEX_OUT" "$OPENCODE_OUT"
    rm -f "$UCF_FILE"
}
trap cleanup EXIT

# Use the debug binary if it exists
BIN="./target/debug/unleash"
if [[ ! -x "$BIN" ]]; then
    echo "ERROR: Please run 'cargo build' first"
    exit 1
fi

PASS=0
FAIL=0
SKIP=0

# Step 1: Claude Code
echo "[1] Claude Code: Creating native UCF session ($SESSION_NAME) and setting context"
"$BIN" claude -u "$SESSION_NAME" -p "The secret passphrase is 'PINEAPPLE'. Acknowledge by saying exactly 'ACK PINEAPPLE'."

if [[ ! -f "$UCF_FILE" ]]; then
    echo "  FAIL: UCF file was not created at $UCF_FILE"
    exit 1
fi
echo "  PASS: UCF file created"
((PASS++))

# Step 2: Gemini
echo "[2] Gemini: Resuming native UCF session and querying context"
"$BIN" gemini -u "$SESSION_NAME" -p "What was the secret passphrase? Reply with exactly 'The passphrase is <passphrase>'." > "$GEMINI_OUT" 2>&1 || true

if grep -iq "PINEAPPLE" "$GEMINI_OUT"; then
    echo "  PASS: Gemini successfully read Claude's history from UCF"
    ((PASS++))
else
    echo "  FAIL: Gemini did not output the expected passphrase. Output was:"
    cat "$GEMINI_OUT"
    ((FAIL++))
fi

# Step 3: Codex (Optional if installed)
echo "[3] Codex: Resuming native UCF session and querying context"
if "$BIN" agents status | grep -q "codex.*Installed"; then
    "$BIN" codex -u "$SESSION_NAME" -p "Repeat the secret passphrase again. Reply with exactly 'Still <passphrase>'." > "$CODEX_OUT" 2>&1 || true
    if grep -iq "PINEAPPLE" "$CODEX_OUT"; then
        echo "  PASS: Codex successfully read history from UCF"
        ((PASS++))
    else
        echo "  WARN: Codex did not output the expected passphrase or failed."
        cat "$CODEX_OUT"
        ((FAIL++))
    fi
else
    echo "  SKIP: Codex not installed"
    ((SKIP++))
fi

# Step 4: OpenCode (Optional if installed)
echo "[4] OpenCode: Resuming native UCF session and querying context"
if "$BIN" agents status | grep -q "opencode.*Installed"; then
    "$BIN" opencode -u "$SESSION_NAME" -p "What is the secret? Reply with exactly 'Final <passphrase>'." > "$OPENCODE_OUT" 2>&1 || true
    if grep -iq "PINEAPPLE" "$OPENCODE_OUT"; then
        echo "  PASS: OpenCode successfully read history from UCF"
        ((PASS++))
    else
        echo "  WARN: OpenCode did not output the expected passphrase or failed."
        cat "$OPENCODE_OUT"
        ((FAIL++))
    fi
else
    echo "  SKIP: OpenCode not installed"
    ((SKIP++))
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed, $SKIP skipped ==="
if [[ $FAIL -gt 0 ]]; then
    echo "Some tests FAILED."
    exit 1
fi
echo "All executed tests passed!"
