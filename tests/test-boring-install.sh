#!/usr/bin/env bash
# test-boring-install.sh - Prove that `--boring` is a genuine non-interactive
# install: it must NOT run the interactive splash/agent picker or launch the TUI.
#
# Hermetic: sources the production installer with UNLEASH_INSTALL_TEST=1 so
# main() does not run, points INSTALL_DIR at a temp dir with a *fake* splash
# that records whether it was invoked, and stubs the side-effecting helpers.
# No network, no downloads, no writes outside the temp dir.

set -uo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$REPO/scripts/install-remote.sh"

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'
FAILS=0
pass() { echo -e "${GREEN}PASS${NC}: $1"; }
fail() {
    echo -e "${RED}FAIL${NC}: $1"
    FAILS=$((FAILS + 1))
}

# Runs the interactive-vs-boring decision in an isolated subshell and prints
# "INVOKED" if the fake splash ran, "SKIPPED" otherwise.
# $1: "boring" to pass --boring, anything else for the interactive path.
run_case() {
    local mode="$1"
    local tmp install_dir sentinel
    tmp="$(mktemp -d)"
    install_dir="$tmp/bin"
    mkdir -p "$install_dir"
    sentinel="$tmp/splash-invoked"

    # Fake splash: records invocation, prints a chosen agent like the real one.
    cat >"$install_dir/splash" <<EOF
#!/usr/bin/env bash
touch "$sentinel"
echo "claude"
EOF
    chmod +x "$install_dir/splash"

    (
        export UNLEASH_INSTALL_TEST=1
        export INSTALL_DIR="$install_dir"
        if [[ "$mode" == "boring" ]]; then
            # shellcheck disable=SC1090
            source "$SCRIPT" --boring
        else
            # shellcheck disable=SC1090
            source "$SCRIPT"
        fi
        # Override side-effecting helpers *after* sourcing so real config writes
        # and prereq checks don't run; splash invocation stays observable.
        set_default_profile() { :; }
        check_agent_prereqs() { :; }
        info() { :; }
        warn() { :; }
        maybe_run_interactive_setup
    ) >/dev/null 2>&1 || true

    if [[ -f "$sentinel" ]]; then
        echo "INVOKED"
    else
        echo "SKIPPED"
    fi
    rm -rf "$tmp"
}

# 1. The contract: --boring must not run the picker or TUI.
result="$(run_case boring)"
if [[ "$result" == "SKIPPED" ]]; then
    pass "--boring skips the interactive splash/picker"
else
    fail "--boring invoked the splash picker (expected SKIPPED, got $result)"
fi

# 2. Discriminating sanity check: without --boring the picker *is* run, proving
#    the test above would actually catch a regression.
result="$(run_case interactive)"
if [[ "$result" == "INVOKED" ]]; then
    pass "interactive install runs the splash picker"
else
    fail "interactive install did not run the splash picker (expected INVOKED, got $result)"
fi

echo ""
if [[ "$FAILS" -gt 0 ]]; then
    echo -e "${RED}FAILED: $FAILS test(s)${NC}"
    exit 1
fi
echo -e "${GREEN}All boring-install tests passed.${NC}"
