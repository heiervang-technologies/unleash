#!/usr/bin/env bash
# Sandbox Network Integration Tests
#
# Verifies that gVisor + LAN isolation is working correctly:
#   1. Internet connectivity (can reach external APIs)
#   2. LAN blocking (cannot reach private RFC 1918 ranges)
#   3. DNS resolution works inside the container
#   4. Inter-container communication is blocked (icc=false)
#
# Prerequisites:
#   - Docker with gVisor (runsc) installed
#   - sandbox-network.sh setup has been run
#   - unleash image built: docker build -f docker/Dockerfile -t unleash .
#
# Usage:
#   sudo ./tests/test_sandbox_network.sh          # full suite
#   sudo ./tests/test_sandbox_network.sh quick     # internet + DNS only (no LAN probe)

set -euo pipefail

IMAGE="unleash:latest"
NETWORK="unleash-sandbox"
PASS=0
FAIL=0
SKIP=0

pass() { echo "  PASS: $1"; ((PASS++)); }
fail() { echo "  FAIL: $1"; ((FAIL++)); }
skip() { echo "  SKIP: $1"; ((SKIP++)); }

# Helper: run a command inside a fresh sandbox container, return exit code
sandbox_run() {
    docker run --rm --network "${NETWORK}" --runtime runsc \
        --entrypoint /bin/bash "${IMAGE}" -c "$1" 2>&1
}

sandbox_run_rc() {
    docker run --rm --network "${NETWORK}" --runtime runsc \
        --entrypoint /bin/bash "${IMAGE}" -c "$1" >/dev/null 2>&1
    return $?
}

echo "=== Sandbox Network Integration Tests ==="
echo ""

# ─── Preflight ───────────────────────────────────────────────
echo "[0] Preflight checks"

if ! docker image inspect "${IMAGE}" >/dev/null 2>&1; then
    echo "  ERROR: Image '${IMAGE}' not found. Build it first:"
    echo "    docker build -f docker/Dockerfile -t unleash ."
    exit 1
fi
pass "Image exists"

if ! docker network inspect "${NETWORK}" >/dev/null 2>&1; then
    echo "  ERROR: Network '${NETWORK}' not found. Run:"
    echo "    sudo ./docker/sandbox-network.sh setup"
    exit 1
fi
pass "Sandbox network exists"

if ! command -v runsc >/dev/null 2>&1; then
    echo "  WARNING: gVisor (runsc) not found on host. Tests will use it inside Docker."
fi

# Check iptables rules are in place
if iptables -L DOCKER-USER -n 2>/dev/null | grep -q "172.30.0.0/16"; then
    pass "iptables LAN-blocking rules present"
else
    echo "  WARNING: iptables rules may be missing (need sudo or rules not set up)"
    echo "  Run: sudo ./docker/sandbox-network.sh setup"
    skip "iptables rules check (insufficient privileges or not set up)"
fi

echo ""

# ─── Test 1: Internet Connectivity ──────────────────────────
echo "[1] Internet connectivity"

# Test HTTPS to a reliable endpoint
INTERNET_OUT=$(sandbox_run "curl -sf --max-time 10 -o /dev/null -w '%{http_code}' https://api.anthropic.com/ 2>/dev/null || echo 'FAILED'")
if [[ "$INTERNET_OUT" == *"FAILED"* ]]; then
    fail "Cannot reach api.anthropic.com (HTTPS)"
else
    pass "HTTPS to api.anthropic.com (HTTP $INTERNET_OUT)"
fi

# Test DNS resolution
DNS_OUT=$(sandbox_run "nslookup api.openai.com 2>/dev/null | grep -c 'Address' || echo 0")
if [[ "$DNS_OUT" -ge 2 ]]; then
    pass "DNS resolution works (api.openai.com)"
else
    # nslookup might not be installed; try getent
    DNS_OUT2=$(sandbox_run "getent hosts api.openai.com 2>/dev/null | head -1 || echo ''")
    if [[ -n "$DNS_OUT2" ]]; then
        pass "DNS resolution works (api.openai.com via getent)"
    else
        fail "DNS resolution failed for api.openai.com"
    fi
fi

# Test npm registry (package installs need this)
NPM_OUT=$(sandbox_run "curl -sf --max-time 10 -o /dev/null -w '%{http_code}' https://registry.npmjs.org/ 2>/dev/null || echo 'FAILED'")
if [[ "$NPM_OUT" == *"FAILED"* ]]; then
    fail "Cannot reach registry.npmjs.org"
else
    pass "npm registry reachable (HTTP $NPM_OUT)"
fi

echo ""

# ─── Test 2: LAN Blocking ───────────────────────────────────
echo "[2] LAN isolation (RFC 1918 ranges blocked)"

if [[ "${1:-full}" == "quick" ]]; then
    skip "LAN tests (quick mode)"
    echo ""
else
    # These should all time out / be unreachable.
    # We use short timeouts since they should fail fast.

    # 192.168.x.x (most common home LAN)
    if sandbox_run_rc "curl -sf --max-time 3 http://192.168.1.1/ >/dev/null 2>&1"; then
        fail "192.168.1.1 is REACHABLE (should be blocked!)"
    else
        pass "192.168.1.1 blocked"
    fi

    # 10.x.x.x
    if sandbox_run_rc "curl -sf --max-time 3 http://10.0.0.1/ >/dev/null 2>&1"; then
        fail "10.0.0.1 is REACHABLE (should be blocked!)"
    else
        pass "10.0.0.1 blocked"
    fi

    # 172.16.x.x
    if sandbox_run_rc "curl -sf --max-time 3 http://172.16.0.1/ >/dev/null 2>&1"; then
        fail "172.16.0.1 is REACHABLE (should be blocked!)"
    else
        pass "172.16.0.1 blocked"
    fi

    # Link-local
    if sandbox_run_rc "curl -sf --max-time 3 http://169.254.169.254/ >/dev/null 2>&1"; then
        fail "169.254.169.254 (link-local/metadata) is REACHABLE (should be blocked!)"
    else
        pass "169.254.169.254 (link-local) blocked"
    fi

    # Docker host gateway (common at 172.30.0.1 on our subnet)
    if sandbox_run_rc "curl -sf --max-time 3 http://172.30.0.1/ >/dev/null 2>&1"; then
        fail "172.30.0.1 (gateway) is REACHABLE (should be blocked by icc rules!)"
    else
        pass "172.30.0.1 (gateway) blocked"
    fi

    echo ""
fi

# ─── Test 3: gVisor Runtime ─────────────────────────────────
echo "[3] gVisor runtime verification"

DMESG_OUT=$(sandbox_run "dmesg 2>&1 || echo 'BLOCKED'")
if [[ "$DMESG_OUT" == *"BLOCKED"* ]] || [[ "$DMESG_OUT" == *"Operation not permitted"* ]] || [[ "$DMESG_OUT" == *"gvisor"* ]]; then
    pass "gVisor syscall filtering active (dmesg restricted)"
else
    fail "dmesg accessible — may not be running under gVisor"
fi

# gVisor blocks /proc/kcore access
KCORE_OUT=$(sandbox_run "cat /proc/kcore 2>&1 || echo 'BLOCKED'")
if [[ "$KCORE_OUT" == *"BLOCKED"* ]] || [[ "$KCORE_OUT" == *"Permission denied"* ]] || [[ "$KCORE_OUT" == *"No such file"* ]]; then
    pass "/proc/kcore inaccessible (gVisor kernel isolation)"
else
    fail "/proc/kcore accessible — gVisor may not be active"
fi

echo ""

# ─── Test 4: Agent CLIs Available ────────────────────────────
echo "[4] Agent CLIs installed and accessible"

for cli in claude codex gemini opencode unleash; do
    VER=$(sandbox_run "command -v ${cli} >/dev/null 2>&1 && ${cli} --version 2>&1 | head -1 || echo 'NOT FOUND'")
    if [[ "$VER" == *"NOT FOUND"* ]]; then
        fail "${cli} not found in container"
    else
        pass "${cli} installed (${VER})"
    fi
done

echo ""

# ─── Results ─────────────────────────────────────────────────
echo "=== Results: ${PASS} passed, ${FAIL} failed, ${SKIP} skipped ==="
if [[ $FAIL -gt 0 ]]; then
    echo ""
    echo "FAILURES DETECTED. Review the output above."
    exit 1
fi
echo "All sandbox tests passed!"
