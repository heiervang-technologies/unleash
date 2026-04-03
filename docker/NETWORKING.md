# unleash Docker Networking Guide

Deep dive into container networking for multi-agent teams and advanced setups.

For quick start, see [README.md](README.md).

## Network Architecture

unleash uses up to two Docker networks for container isolation:

### unleash-sandbox (external, managed by sandbox-network.sh)

- **Subnet:** 172.30.0.0/16
- **Gateway:** Yes (internet access)
- **ICC:** Disabled (`enable_icc=false` — no inter-container communication)
- **iptables:** DROP rules in DOCKER-USER block RFC 1918 (LAN) and link-local
- **Purpose:** Internet access with LAN isolation

### unleash-mesh (inline, created by docker compose)

- **Subnet:** 10.100.0.0/16 (pinned)
- **Gateway:** None (`internal: true`)
- **iptables:** None needed — Docker handles isolation
- **Purpose:** Agent-to-agent communication only

The mesh subnet is pinned to 10.100.0.0/16 (outside 172.16.0.0/12) to avoid interaction with the sandbox iptables DROP rules. Cross-bridge traffic from sandbox to mesh IPs is dropped by `-s 172.30.0.0/16 -d 10.0.0.0/8 -j DROP` — this is a desirable safety net that prevents unintended cross-network routing.

## Traffic Flow

| From | To | Path | Allowed |
|------|----|------|---------|
| Agent | Internet | sandbox gateway | Yes |
| Agent | LAN (192.168.x) | sandbox gateway -> iptables | Blocked (DROP) |
| Agent | Agent (via .mesh alias) | mesh bridge | Yes |
| Agent | Agent (via sandbox) | sandbox bridge | Blocked (icc=false + DROP) |
| Sidecar (mesh-only) | Internet | no gateway | Blocked (no route) |
| Sidecar (mesh-only) | Agent | mesh bridge | Yes |

## Service Discovery

Each agent gets a `.mesh` alias on the mesh network: `claude.mesh`, `codex.mesh`, `gemini.mesh`, `opencode.mesh`. These aliases resolve exclusively to mesh IPs (10.100.x.x), avoiding DNS ambiguity when containers are on multiple networks.

```bash
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml up claude codex
```

From the claude container, reach codex via `codex.mesh`.

### Why .mesh aliases?

When containers are on both sandbox and mesh networks, Docker DNS may return either IP for the bare service name. If it returns the sandbox IP, the connection is blocked (sandbox has `icc=false` and iptables DROP rules). The `.mesh` alias only exists on the mesh network, so it always resolves to the correct IP.

### Important: up vs run

Use `docker compose up`, not `run`. The `run` command creates one-off containers that do not register DNS names. Other containers cannot discover them by service name.

### Verifying DNS

From inside a running container:

```bash
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  exec claude nslookup codex.mesh
```

Should resolve to a 10.100.x.x address (mesh network).

## Adding Sidecars (Tier 3)

Sidecars are services that agents need but that are not agents themselves: MCP servers, databases, caches, message brokers.

### Pattern: Mesh-Only Sidecar

The sidecar joins the mesh network but NOT the sandbox network. It has no internet access and no LAN access. Agents reach it by service name on mesh.

> **Prerequisite:** The multi-agent override must be running so the `unleash-mesh` network exists. Start the multi-agent compose first, then add sidecars.

Create a `docker-compose.sidecar.yml` in your project (this file is not shipped with Unleash — you write it for your specific services):

```yaml
# docker-compose.sidecar.yml (user-created)
services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_PASSWORD: localdev
    networks:
      - mesh
    # NOT on sandbox — no internet, no LAN

networks:
  mesh:
    name: unleash-mesh
    external: true
```

Usage:

```bash
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  -f docker-compose.sidecar.yml \
  up claude postgres
```

The agent can reach postgres at `postgres:5432` on the mesh network. Postgres cannot reach the internet or LAN.

### Pattern: Sidecar With Internet

If the sidecar needs internet (e.g., an MCP server that calls external APIs), add it to both networks:

```yaml
services:
  mcp-server:
    image: my-mcp-server
    networks:
      - mesh
      - sandbox
```

### Security: Pivot Attack Prevention

**Hard rule:** Sidecars on mesh MUST NOT also be on host-networked or default-bridge networks.

A sidecar with LAN connectivity becomes a pivot point. A compromised agent could route traffic: agent -> mesh -> sidecar -> LAN, bypassing the sandbox firewall entirely.

Safe configurations:

| Sidecar Networks | Internet | LAN | Safety |
|-----------------|----------|-----|--------|
| `mesh` only | No | No | Safest |
| `mesh` + `sandbox` | Yes | No | Safe |
| `mesh` + `host`/`default` | Yes | **Yes** | **UNSAFE — breaks isolation** |

## Compose Override Combinations

| Files | Runtime | Networks | Use Case |
|-------|---------|----------|----------|
| base | runsc | sandbox | Single agent (default) |
| base + runc | runc | default | No gVisor available |
| base + gpu | runc | sandbox | Single agent with GPU/Vulkan |
| base + multi-agent | runsc | sandbox + mesh | Agent team |
| base + gpu + multi-agent | runc | sandbox + mesh | Agent team with GPU/Vulkan |
| base + runc + multi-agent | runc | default + mesh | Agent team, no gVisor |

The runc + multi-agent combination uses the default Docker bridge for internet (no LAN blocking, no gVisor) and the mesh network for inter-agent communication. This is a lower-security mode suitable for development.

## Troubleshooting

### Agent cannot reach another agent

1. Verify you used `up`, not `run`
2. Check DNS: `docker compose ... exec claude nslookup codex.mesh`
3. Check network attachment: `docker network inspect unleash-mesh`
4. Verify mesh network exists: `docker network ls | grep mesh`

### Agent cannot reach the internet

1. Verify sandbox network exists: `docker network inspect unleash-sandbox`
2. Check iptables rules: `sudo ./docker/sandbox-network.sh status`
3. Rules may be missing after reboot — re-run: `sudo ./docker/sandbox-network.sh setup`

### Agent can reach LAN (should be blocked)

1. Check iptables rules are in place: `sudo iptables -L DOCKER-USER -n`
2. Look for DROP rules with source 172.30.0.0/16
3. If missing, re-run: `sudo ./docker/sandbox-network.sh setup`
4. Remember: iptables rules do not survive reboots

### Compose merge errors

If you get network-related errors combining overrides:

1. Ensure `unleash-sandbox` network exists: `docker network ls`
2. If not, create it: `sudo ./docker/sandbox-network.sh setup`
3. The mesh network is auto-created by compose — no manual setup needed
