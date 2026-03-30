# Docker Networking Design: Tiered Isolation for Multi-Agent Containers

**Date:** 2026-03-29
**Status:** Draft
**PR:** TBD

## Problem

unleash Docker containers currently support a single networking tier: gVisor + sandbox network with LAN blocking. This prevents agents from reaching private networks but also blocks inter-container communication. Multi-agent teams need a secure channel to coordinate, and advanced setups need agent-to-sidecar connectivity (MCP servers, databases).

## Design: Three Networking Tiers

Each tier adds a network. No tier removes isolation from the previous one.

### Tier 1: Single Agent (existing, default)

- **Networks:** `unleash-sandbox` only
- **Internet:** Yes (APIs, npm, git)
- **LAN:** Blocked (RFC 1918 + link-local via iptables)
- **Inter-container:** Blocked (sandbox subnet falls within blocked 172.16.0.0/12 range)
- **Runtime:** gVisor (runsc)

One change: add `--opt com.docker.network.bridge.enable_icc=false` to the sandbox network creation in `sandbox-network.sh`. This disables inter-container communication at the bridge level (L2), making the isolation guarantee airtight regardless of whether iptables rules catch same-bridge traffic. Without this, two containers on the sandbox bridge could communicate directly.

### Tier 2: Multi-Agent Teams

- **Networks:** `unleash-sandbox` + `unleash-mesh`
- **Internet:** Yes, via sandbox
- **LAN:** Blocked, via sandbox iptables rules
- **Inter-container:** Yes, via mesh only
- **Runtime:** gVisor (runsc)

The mesh network uses Docker `internal: true`, meaning no gateway and no internet routing. A compromised agent on mesh can only reach other mesh containers — never LAN or internet through it. Internet access is strictly via the sandbox network.

#### Running Multiple Agents

Multi-agent requires `docker compose up` (not `run`). The `run` command creates one-off containers that do not register DNS names for other containers to discover. With `up`, each service gets a stable DNS name on the mesh network.

```bash
# Start a team of 2 Claude agents + 1 Codex agent
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  up claude codex
```

Agents discover each other by **service name** on the mesh network (e.g., `claude`, `codex`). For multiple instances of the same agent, use `--scale claude=3` — instances are reachable as `claude-1`, `claude-2`, `claude-3`.

To run a specific command against the team:

```bash
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  exec claude claude --version
```

### Tier 3: Sidecars (advanced)

- **Networks:** `unleash-sandbox` + `unleash-mesh` (or mesh-only for services)
- **Use case:** Agent talks to MCP server, database, or custom service on mesh
- **Pattern:** Sidecar joins mesh but NOT sandbox (no internet for the database)

**Security rule:** Sidecars on mesh MUST NOT also be on host-networked or default-bridge networks. A sidecar with LAN connectivity becomes a pivot point — a compromised agent could use it to reach the LAN indirectly, bypassing the sandbox firewall. Only attach sidecars to `unleash-mesh` (and optionally `unleash-sandbox` if they need internet).

Documented in `docker/NETWORKING.md`, not in the main README.

## File Changes

### Rename: `docker-compose.unsandboxed.yml` -> `docker-compose.runc.yml`

Docker namespace isolation is still a security layer. "Unsandboxed" is inaccurate — `runc` precisely names the runtime and pairs visually with `runsc` in the base file.

Contents unchanged — overrides `runtime: runc` and uses default network. All references updated: README, CI workflow, code comments.

### New: `docker-compose.multi-agent.yml`

```yaml
# Override: adds mesh network for inter-agent communication
#
# Usage:
#   docker compose -f docker/docker-compose.yml \
#     -f docker/docker-compose.multi-agent.yml up claude codex
#
# IMPORTANT: Use 'up', not 'run'. The 'run' command creates one-off
# containers that don't register DNS names for service discovery.
#
# Agents can reach each other by service name on the mesh network.
# Internet access remains via the sandbox network.

services:
  unleash:
    networks:
      sandbox:
      mesh:

  # Each agent service gets a .mesh alias for guaranteed mesh-only DNS resolution
  claude:
    networks:
      mesh:
        aliases:
          - claude.mesh
  codex:
    networks:
      mesh:
        aliases:
          - codex.mesh
  gemini:
    networks:
      mesh:
        aliases:
          - gemini.mesh
  opencode:
    networks:
      mesh:
        aliases:
          - opencode.mesh

networks:
  sandbox:
    external: true
    name: unleash-sandbox
  mesh:
    driver: bridge
    name: unleash-mesh
    internal: true
    ipam:
      config:
        - subnet: 10.100.0.0/16
```

Since all agent services extend `unleash`, they inherit both networks. The `.mesh` aliases ensure agents can reliably reach each other via `claude.mesh`, `codex.mesh`, etc. — these names resolve exclusively to mesh IPs (10.100.x.x), avoiding DNS ambiguity when containers are on multiple networks.

### Network Design Decisions

**Mesh subnet (10.100.0.0/16):** Explicitly pinned outside 172.16.0.0/12 to avoid interaction with the sandbox iptables DROP rules. Mesh-sourced traffic (10.100.x.x source) is not matched by any DROP rule since the rules only target source `172.30.0.0/16`. Note: cross-bridge traffic from sandbox IP (172.30.x.x) to mesh IP (10.100.x.x) WILL match `-s 172.30.0.0/16 -d 10.0.0.0/8 -j DROP` and be dropped. This is a desirable safety net — if DNS resolves incorrectly and returns a mesh IP to a sandbox-sourced lookup, the traffic fails fast rather than succeeding through an unintended path.

**Mesh is inline, sandbox is external:** The sandbox network requires iptables rules managed by `sandbox-network.sh`, so it must be pre-created (external). The mesh network needs no iptables — `internal: true` provides isolation at the Docker level. Compose auto-creates and auto-removes it, which is the right lifecycle for an opt-in overlay.

**DNS and service discovery:** Docker's embedded DNS resolves service names to IPs on shared networks. When containers share multiple networks, the resolution order is implementation-dependent and not guaranteed. If DNS returns the sandbox IP for an inter-container lookup, the connection is dropped by iptables (containers are in separate namespaces, so even same-bridge traffic traverses the FORWARD/DOCKER-USER chain). To eliminate this ambiguity, each agent service gets a `.mesh` network alias (e.g., `claude.mesh`, `codex.mesh`). These aliases exist only on the mesh network, so they always resolve to mesh IPs (10.100.x.x). **Agents should use `.mesh` names for inter-container communication.** The bare service names (`claude`, `codex`) still work for internet-bound traffic via sandbox.

### Updated: `docker/README.md`

Add tier 2 multi-agent section after the existing quick start. Rename all `unsandboxed` references to `runc`. Update security summary table:

| Setup | Kernel Isolation | LAN Blocked | Inter-Container | Internet |
|-------|-----------------|-------------|-----------------|----------|
| Default (gVisor + sandbox) | gVisor syscall filter | Yes | No | Yes |
| Multi-agent (+ mesh) | gVisor syscall filter | Yes | Mesh only | Yes |
| runc override | None (shared kernel) | No | Default Docker | Yes |
| `--network none` | N/A | Yes | No | No |

### New: `docker/NETWORKING.md`

Comprehensive guide covering:

1. **How the networks work** — sandbox vs mesh vs default bridge, what `internal: true` means
2. **Service discovery** — how agents find each other by service name, `up` vs `run` implications
3. **Adding sidecars** — services on mesh without sandbox access, **with security warning about pivot attacks**
4. **Example: Agent + Postgres** — agent gets internet via sandbox, postgres on mesh only
5. **Example: Agent + MCP server** — MCP server as sidecar on mesh
6. **Custom topologies** — third network for service-to-service isolation
7. **Troubleshooting** — DNS resolution, connectivity debugging, iptables inspection

### Updated: `.github/workflows/docker-build.yml`

Update reference from `docker-compose.unsandboxed.yml` to `docker-compose.runc.yml` if referenced.

### Updated: `docs/DOCUMENTATION_MAP.md`

Add Docker/networking section pointing to `docker/README.md` and `docker/NETWORKING.md`.

## Security Properties

| Network | Subnet | Gateway | Internet | LAN | Inter-Container | iptables Rules |
|---------|--------|---------|----------|-----|-----------------|----------------|
| `unleash-sandbox` | 172.30.0.0/16 | Yes | Yes | Blocked | Blocked | DROP RFC 1918 from subnet |
| `unleash-mesh` | 10.100.0.0/16 | None | No | No | Yes | None needed (`internal: true`) |
| Default bridge | Auto | Yes | Yes | Yes | Yes | None |

Key invariant: **adding mesh never weakens sandbox isolation.** The sandbox DROP rules remain unchanged. Mesh is additive only.

**Sidecar pivot prevention:** Sidecars MUST NOT be on host or default-bridge networks. A sidecar with LAN access becomes an indirect path from agent -> mesh -> sidecar -> LAN. This is documented as a hard rule in NETWORKING.md.

## Iptables Persistence Note

Firewall rules do not survive reboots. The Docker networks persist but the DOCKER-USER rules are lost. Users must re-run `sudo ./docker/sandbox-network.sh setup` after reboot or add it to a startup script. This is documented in the README with a prominent warning.

## Compose Merge Compatibility

The three override files must work in any valid combination:

| Combination | Runtime | Networks | Valid |
|-------------|---------|----------|-------|
| base only | runsc | sandbox | Yes (default) |
| base + runc | runc | default | Yes |
| base + multi-agent | runsc | sandbox + mesh | Yes |
| base + runc + multi-agent | runc | default + mesh | Yes |

The runc override redefines the sandbox network to a local bridge. The multi-agent override declares `sandbox: external: true`, and runc shadows it with a local bridge. The mesh network is independently declared and unaffected by the sandbox override. **Expected behavior for runc + multi-agent:** agents use the default Docker bridge for internet (no LAN blocking, no gVisor), and the mesh network for inter-agent communication. This is a lower-security mode suitable for development without gVisor. **Test this combination explicitly.**

## Implementation Order

1. Rename `unsandboxed` -> `runc` (all references)
2. Create `docker-compose.multi-agent.yml` with mesh network
3. Update `docker/README.md` with tier 2, rename refs, and updated security table
4. Create `docker/NETWORKING.md` (tier 3 guide with sidecar security rules)
5. Update `docs/DOCUMENTATION_MAP.md`
6. Update CI workflow if needed
7. Test all combinations per the compose merge table above

## Test Plan

1. **Single agent (tier 1):** `docker compose run --rm claude` — verify internet works, LAN blocked
2. **Multi-agent discovery:** Start 2 agents with `up`, exec into one, verify `nslookup <other-service>` resolves to mesh IP (10.100.x.x), not sandbox IP (172.30.x.x)
3. **Multi-agent connectivity:** From agent A, verify it can reach agent B on mesh (e.g., `ping codex` or TCP connection)
4. **LAN still blocked:** From multi-agent container, verify `curl 192.168.x.x` times out
5. **runc + multi-agent:** Verify base + runc + multi-agent compose merge works without errors
6. **Sidecar isolation:** Add a test service on mesh-only, verify it cannot reach the internet
