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

    # Block LAN + host access from sandbox containers using the `raw` PREROUTING
    # chain. This runs BEFORE any NAT/DNAT, so it catches traffic that would
    # otherwise be rewritten by k8s NodePorts, Docker port-maps, CNI plugins,
    # etc. (Simply adding rules to DOCKER-USER/FORWARD races against KUBE-FORWARD
    # and loses for any DNAT'd service.)
    #
    # Covered ranges:
    #   10.0.0.0/8         — RFC 1918 / common LAN / k8s pod CIDRs
    #   172.16.0.0/12      — RFC 1918 / Docker bridges incl. our own subnet
    #                        (intentional: blocks inter-container + gateway access)
    #   192.168.0.0/16     — RFC 1918 / home LANs
    #   169.254.0.0/16     — Link-local + cloud metadata (169.254.169.254)
    #   100.64.0.0/10      — CGNAT / Tailscale overlays
    local raw_rules=(
        "-s ${SUBNET} -d 10.0.0.0/8 -j DROP"
        "-s ${SUBNET} -d 172.16.0.0/12 -j DROP"
        "-s ${SUBNET} -d 192.168.0.0/16 -j DROP"
        "-s ${SUBNET} -d 169.254.0.0/16 -j DROP"
        "-s ${SUBNET} -d 100.64.0.0/10 -j DROP"
    )

    for rule in "${raw_rules[@]}"; do
        if ! iptables -t raw -C PREROUTING ${rule} 2>/dev/null; then
            iptables -t raw -I PREROUTING ${rule}
            echo "  Added raw/PREROUTING rule: DROP ${rule}"
        else
            echo "  raw/PREROUTING rule already exists: ${rule}"
        fi
    done

    # Belt-and-suspenders: keep the legacy DOCKER-USER rules too (for any packet
    # path that skips raw PREROUTING, e.g. locally generated traffic).
    local docker_rules=(
        "-s ${SUBNET} -d 10.0.0.0/8 -j DROP"
        "-s ${SUBNET} -d 172.16.0.0/12 -j DROP"
        "-s ${SUBNET} -d 192.168.0.0/16 -j DROP"
        "-s ${SUBNET} -d 169.254.0.0/16 -j DROP"
        "-s ${SUBNET} -d 100.64.0.0/10 -j DROP"
    )
    for rule in "${docker_rules[@]}"; do
        if ! iptables -C DOCKER-USER ${rule} 2>/dev/null; then
            iptables -I DOCKER-USER ${rule}
            echo "  Added DOCKER-USER rule: DROP ${rule}"
        fi
    done

    # And an INPUT chain drop as a final safety net for anything we missed.
    local input_rule="-s ${SUBNET} -j DROP"
    if ! iptables -C INPUT ${input_rule} 2>/dev/null; then
        iptables -I INPUT ${input_rule}
        echo "  Added INPUT rule: DROP ${input_rule}"
    fi

    echo ""
    echo "Sandbox ready. Containers on '${NETWORK_NAME}' have:"
    echo "  - Full internet access (APIs, npm, git, etc.)"
    echo "  - No access to any RFC 1918 / CGNAT address (LAN, k8s pods, Tailscale, host)"
    echo "  - Enforced before DNAT (k8s NodePorts, Docker port-maps can't be reached)"
    echo ""
    echo "Run containers with: --network ${NETWORK_NAME}"
}

teardown() {
    echo "Tearing down sandboxed network: ${NETWORK_NAME}"

    # Remove raw/PREROUTING rules (primary defense)
    local raw_rules=(
        "-s ${SUBNET} -d 10.0.0.0/8 -j DROP"
        "-s ${SUBNET} -d 172.16.0.0/12 -j DROP"
        "-s ${SUBNET} -d 192.168.0.0/16 -j DROP"
        "-s ${SUBNET} -d 169.254.0.0/16 -j DROP"
        "-s ${SUBNET} -d 100.64.0.0/10 -j DROP"
    )

    for rule in "${raw_rules[@]}"; do
        # Loop in case multiple copies were inserted over time
        while iptables -t raw -C PREROUTING ${rule} 2>/dev/null; do
            iptables -t raw -D PREROUTING ${rule}
            echo "  Removed raw/PREROUTING rule: ${rule}"
        done
    done

    # Remove DOCKER-USER (FORWARD) rules
    local rules=(
        "-s ${SUBNET} -d 10.0.0.0/8 -j DROP"
        "-s ${SUBNET} -d 172.16.0.0/12 -j DROP"
        "-s ${SUBNET} -d 192.168.0.0/16 -j DROP"
        "-s ${SUBNET} -d 169.254.0.0/16 -j DROP"
        "-s ${SUBNET} -d 100.64.0.0/10 -j DROP"
    )

    for rule in "${rules[@]}"; do
        while iptables -C DOCKER-USER ${rule} 2>/dev/null; do
            iptables -D DOCKER-USER ${rule}
            echo "  Removed DOCKER-USER rule: ${rule}"
        done
    done

    # Remove INPUT rule (container-to-host block)
    local input_rule="-s ${SUBNET} -j DROP"
    while iptables -C INPUT ${input_rule} 2>/dev/null; do
        iptables -D INPUT ${input_rule}
        echo "  Removed INPUT rule: ${input_rule}"
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
    echo "  Firewall rules (raw/PREROUTING → pre-DNAT, blocks k8s NodePorts):"
    if ! iptables -t raw -L PREROUTING -n 2>/dev/null; then
        echo "    Cannot read raw/PREROUTING (try: sudo $0 status)"
    elif iptables -t raw -L PREROUTING -n 2>/dev/null | grep -q "${SUBNET%%/*}"; then
        iptables -t raw -L PREROUTING -n 2>/dev/null | grep "${SUBNET%%/*}" | while read -r line; do
            echo "    ${line}"
        done
    else
        echo "    Missing — containers can reach DNAT'd LAN services! Run: sudo $0 setup"
    fi

    echo ""
    echo "  Firewall rules (DOCKER-USER → other LAN hosts):"
    if ! iptables -L DOCKER-USER -n 2>/dev/null; then
        echo "    Cannot read iptables rules (try: sudo $0 status)"
    elif iptables -L DOCKER-USER -n 2>/dev/null | grep -q "${SUBNET%%/*}"; then
        iptables -L DOCKER-USER -n 2>/dev/null | grep "${SUBNET%%/*}" | while read -r line; do
            echo "    ${line}"
        done
    else
        echo "    No sandbox rules found (run: sudo $0 setup)"
    fi

    echo ""
    echo "  Firewall rule (INPUT → Docker host):"
    if ! iptables -L INPUT -n 2>/dev/null; then
        echo "    Cannot read INPUT chain (try: sudo $0 status)"
    elif iptables -L INPUT -n 2>/dev/null | grep -qE "DROP.*${SUBNET//./\\.}"; then
        iptables -L INPUT -n 2>/dev/null | grep -E "DROP.*${SUBNET//./\\.}" | while read -r line; do
            echo "    ${line}"
        done
    else
        echo "    Missing — containers can reach the host! Run: sudo $0 setup"
    fi
}

allow_ip() {
    local input="${1:-}"
    if [[ -z "$input" ]]; then
        echo "Usage: $0 allow-ip <IP_ADDRESS>[:<PORT>]"
        echo ""
        echo "Opens a single LAN IP (optionally restricted to a specific port)"
        echo "through the sandbox firewall so containers can reach a local service."
        echo ""
        echo "Examples:"
        echo "  $0 allow-ip 192.168.1.100        # open all ports on that IP"
        echo "  $0 allow-ip 192.168.1.100:8080   # open only port 8080"
        echo ""
        echo "WARNING: This increases the attack surface. See docker/README.md"
        echo "for security guidance. Using a port restriction is strongly recommended."
        exit 1
    fi

    # Parse IP and optional port
    local ip port=""
    if [[ "$input" == *:* ]]; then
        ip="${input%%:*}"
        port="${input##*:}"
    else
        ip="$input"
    fi

    # Validate it's actually a private IP
    if ! echo "$ip" | grep -qE '^(10\.|172\.(1[6-9]|2[0-9]|3[01])\.|192\.168\.)'; then
        echo "ERROR: $ip does not look like a private (RFC 1918) address."
        echo "Only private IPs need firewall exceptions — public IPs are already reachable."
        exit 1
    fi

    # Validate port if specified
    if [[ -n "$port" ]] && ! [[ "$port" =~ ^[0-9]+$ && "$port" -ge 1 && "$port" -le 65535 ]]; then
        echo "ERROR: Invalid port '$port'. Must be 1-65535."
        exit 1
    fi

    # Build iptables rule args
    local rule_args=(-s "${SUBNET}" -d "${ip}/32")
    if [[ -n "$port" ]]; then
        rule_args+=(-p tcp --dport "$port")
    fi
    rule_args+=(-j ACCEPT)

    # Insert ACCEPT rule BEFORE the DROP rules in DOCKER-USER
    if iptables -C DOCKER-USER "${rule_args[@]}" 2>/dev/null; then
        echo "Rule already exists: ACCEPT ${SUBNET} -> ${input}"
    else
        iptables -I DOCKER-USER "${rule_args[@]}"
        if [[ -n "$port" ]]; then
            echo "Added firewall exception: containers can reach ${ip} on port ${port} only"
        else
            echo "Added firewall exception: containers can reach ${ip} on ALL ports"
            echo "  (consider restricting to a specific port: $0 allow-ip ${ip}:<PORT>)"
        fi
        echo ""
        echo "To revoke: sudo $0 revoke-ip ${input}"
        echo ""
        echo "NOTE: This rule does NOT survive reboots. Re-run after restart."
    fi
}

revoke_ip() {
    local input="${1:-}"
    if [[ -z "$input" ]]; then
        echo "Usage: $0 revoke-ip <IP_ADDRESS>[:<PORT>]"
        exit 1
    fi

    # Parse IP and optional port
    local ip port=""
    if [[ "$input" == *:* ]]; then
        ip="${input%%:*}"
        port="${input##*:}"
    else
        ip="$input"
    fi

    local rule_args=(-s "${SUBNET}" -d "${ip}/32")
    if [[ -n "$port" ]]; then
        rule_args+=(-p tcp --dport "$port")
    fi
    rule_args+=(-j ACCEPT)

    if iptables -C DOCKER-USER "${rule_args[@]}" 2>/dev/null; then
        iptables -D DOCKER-USER "${rule_args[@]}"
        echo "Revoked firewall exception for ${input}"
    else
        echo "No exception found for ${input}"
    fi
}

case "${1:-status}" in
    setup)      setup ;;
    teardown)   teardown ;;
    status)     status ;;
    allow-ip)   allow_ip "${2:-}" ;;
    revoke-ip)  revoke_ip "${2:-}" ;;
    *)
        echo "Usage: $0 {setup|teardown|status|allow-ip <IP>|revoke-ip <IP>}"
        exit 1
        ;;
esac
