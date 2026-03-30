#!/usr/bin/env bash
# Create a sandboxed Docker network for unleash containers.
# Allows internet access but blocks all LAN/private network ranges.
#
# Usage:
#   sudo ./docker/sandbox-network.sh setup    # Create network + firewall rules
#   sudo ./docker/sandbox-network.sh teardown  # Remove network + firewall rules
#   ./docker/sandbox-network.sh status         # Check current state

set -euo pipefail

NETWORK_NAME="unleash-sandbox"
SUBNET="172.30.0.0/16"

setup() {
    echo "Setting up sandboxed network: ${NETWORK_NAME}"

    # Create network if it doesn't exist
    if ! docker network inspect "${NETWORK_NAME}" &>/dev/null; then
        docker network create \
            --driver bridge \
            --subnet "${SUBNET}" \
            --opt com.docker.network.bridge.enable_icc=false \
            "${NETWORK_NAME}"
        echo "  Created network ${NETWORK_NAME} (${SUBNET})"
    else
        echo "  Network ${NETWORK_NAME} already exists"
    fi

    # Block LAN access from sandbox containers (RFC 1918 private ranges)
    # DOCKER-USER chain is processed before Docker's own rules
    #
    # NOTE: The 172.16.0.0/12 rule also covers the sandbox subnet (172.30.0.0/16).
    # This intentionally blocks inter-container communication — each container
    # should only talk to the internet, not to other sandbox containers.
    local rules=(
        "-s ${SUBNET} -d 10.0.0.0/8 -j DROP"
        "-s ${SUBNET} -d 172.16.0.0/12 -j DROP"
        "-s ${SUBNET} -d 192.168.0.0/16 -j DROP"
        "-s ${SUBNET} -d 169.254.0.0/16 -j DROP"
    )

    for rule in "${rules[@]}"; do
        if ! iptables -C DOCKER-USER ${rule} 2>/dev/null; then
            iptables -I DOCKER-USER ${rule}
            echo "  Added firewall rule: DROP ${rule}"
        else
            echo "  Firewall rule already exists: ${rule}"
        fi
    done

    echo ""
    echo "Sandbox ready. Containers on '${NETWORK_NAME}' have:"
    echo "  - Full internet access (APIs, npm, git, etc.)"
    echo "  - No access to LAN (10.x, 172.16-31.x, 192.168.x blocked)"
    echo ""
    echo "Run containers with: --network ${NETWORK_NAME}"
}

teardown() {
    echo "Tearing down sandboxed network: ${NETWORK_NAME}"

    # Remove firewall rules
    local rules=(
        "-s ${SUBNET} -d 10.0.0.0/8 -j DROP"
        "-s ${SUBNET} -d 172.16.0.0/12 -j DROP"
        "-s ${SUBNET} -d 192.168.0.0/16 -j DROP"
        "-s ${SUBNET} -d 169.254.0.0/16 -j DROP"
    )

    for rule in "${rules[@]}"; do
        if iptables -C DOCKER-USER ${rule} 2>/dev/null; then
            iptables -D DOCKER-USER ${rule}
            echo "  Removed firewall rule: ${rule}"
        fi
    done

    # Remove network
    if docker network inspect "${NETWORK_NAME}" &>/dev/null; then
        docker network rm "${NETWORK_NAME}"
        echo "  Removed network ${NETWORK_NAME}"
    else
        echo "  Network ${NETWORK_NAME} not found"
    fi

    echo "Sandbox teardown complete."
}

status() {
    echo "Sandbox Network Status"
    echo ""

    if docker network inspect "${NETWORK_NAME}" &>/dev/null; then
        echo "  Network: ${NETWORK_NAME} (active)"
        local containers
        containers=$(docker network inspect "${NETWORK_NAME}" --format '{{range .Containers}}{{.Name}} {{end}}' 2>/dev/null)
        if [ -n "${containers}" ]; then
            echo "  Connected containers: ${containers}"
        else
            echo "  Connected containers: none"
        fi
    else
        echo "  Network: not created (run 'sudo ./sandbox-network.sh setup')"
    fi

    echo ""
    echo "  Firewall rules (DOCKER-USER):"
    if ! iptables -L DOCKER-USER -n 2>/dev/null; then
        echo "    Cannot read iptables rules (try: sudo $0 status)"
    elif iptables -L DOCKER-USER -n 2>/dev/null | grep -q "${SUBNET%%/*}"; then
        iptables -L DOCKER-USER -n 2>/dev/null | grep "${SUBNET%%/*}" | while read -r line; do
            echo "    ${line}"
        done
    else
        echo "    No sandbox rules found (run: sudo $0 setup)"
    fi
}

case "${1:-status}" in
    setup)    setup ;;
    teardown) teardown ;;
    status)   status ;;
    *)
        echo "Usage: $0 {setup|teardown|status}"
        exit 1
        ;;
esac
