# Docker Networking Tiers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add multi-agent mesh networking and comprehensive networking documentation to Unleash Docker containers.

**Architecture:** Layered Docker Compose overrides — base (sandbox only), multi-agent (adds mesh), runc (swaps runtime). Mesh uses `internal: true` with pinned subnet 10.100.0.0/16. Documentation split: tiers 1-2 in README, tier 3 in NETWORKING.md.

**Tech Stack:** Docker Compose, iptables, gVisor (runsc), Docker internal networks

**Spec:** `docs/superpowers/specs/2026-03-29-docker-networking-design.md`

---

### Task 1: Rename unsandboxed to runc

**Files:**
- Rename: `docker/docker-compose.unsandboxed.yml` -> `docker/docker-compose.runc.yml`
- Modify: `docker/docker-compose.yml:8` (comment reference)
- Modify: `docker/README.md:32-39` (unsandboxed references)

- [ ] **Step 1: Rename the file**

```bash
git mv docker/docker-compose.unsandboxed.yml docker/docker-compose.runc.yml
```

- [ ] **Step 2: Update contents of the renamed file**

In `docker/docker-compose.runc.yml`, update the comment and network name:

```yaml
# Override: run with standard Docker runtime (runc) instead of gVisor (runsc)
#
# Usage:
#   docker compose -f docker/docker-compose.yml -f docker/docker-compose.runc.yml run --rm claude

services:
  unleash:
    runtime: runc
    networks:
      - default

networks:
  sandbox:
    external: false
    name: unleash-runc-default
    driver: bridge
```

- [ ] **Step 3: Update base compose comment**

In `docker/docker-compose.yml`, line 8, change:
```
-f docker/docker-compose.unsandboxed.yml
```
to:
```
-f docker/docker-compose.runc.yml
```

- [ ] **Step 4: Update README references**

In `docker/README.md`, replace the "Without Sandbox" section heading and content:

Change heading from `### Without Sandbox` to `### Without gVisor`.

Change the description and command:
```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.runc.yml \
  run --rm claude
```

Update the warning text: replace "Without gVisor" for "Without gVisor, the container uses standard Docker isolation (runc). This still provides namespace and cgroup isolation, but agents run with `--dangerously-skip-permissions` and have full network access including your LAN."

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: rename unsandboxed to runc for accuracy

Docker namespace isolation is still a security layer. 'runc' precisely
names the runtime and pairs visually with 'runsc' in the base file."
```

---

### Task 2: Add icc=false to sandbox network

**Files:**
- Modify: `docker/sandbox-network.sh`

- [ ] **Step 1: Update network creation to disable inter-container communication**

In the `setup()` function, update the `docker network create` command:

```bash
docker network create \
    --driver bridge \
    --subnet "${SUBNET}" \
    --opt com.docker.network.bridge.enable_icc=false \
    "${NETWORK_NAME}"
```

The `enable_icc=false` option disables inter-container communication at the bridge level, making sandbox isolation airtight regardless of iptables behavior for same-bridge traffic.

- [ ] **Step 2: Commit**

```bash
git add docker/sandbox-network.sh
git commit -m "security: disable inter-container communication on sandbox bridge

Adds --opt com.docker.network.bridge.enable_icc=false to the sandbox
network. This blocks same-bridge inter-container traffic at L2,
ensuring isolation even without iptables FORWARD chain rules."
```

---

### Task 3: Create docker-compose.multi-agent.yml

**Files:**
- Create: `docker/docker-compose.multi-agent.yml`

- [ ] **Step 1: Create the multi-agent override file**

Write `docker/docker-compose.multi-agent.yml`:

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
# Agents discover each other via .mesh aliases (e.g., claude.mesh, codex.mesh).
# These resolve exclusively to mesh IPs, avoiding DNS ambiguity.
# Internet access remains via the sandbox network.

services:
  unleash:
    networks:
      sandbox:
      mesh:

  # .mesh aliases guarantee resolution to mesh IPs only
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

- [ ] **Step 2: Verify compose config parses**

Run: `docker compose -f docker/docker-compose.yml -f docker/docker-compose.multi-agent.yml config --quiet`
Expected: exits 0 with no errors (requires unleash-sandbox network to exist)

- [ ] **Step 3: Verify runc + multi-agent merge**

Run: `docker compose -f docker/docker-compose.yml -f docker/docker-compose.runc.yml -f docker/docker-compose.multi-agent.yml config --quiet`
Expected: exits 0 — confirms the three-file merge works

- [ ] **Step 4: Commit**

```bash
git add docker/docker-compose.multi-agent.yml
git commit -m "feat: add multi-agent mesh network compose override

Adds unleash-mesh (internal, 10.100.0.0/16) for inter-container
communication. Agents join both sandbox and mesh networks.
Use with 'docker compose up', not 'run'."
```

---

### Task 4: Update docker/README.md

**Files:**
- Modify: `docker/README.md`

- [ ] **Step 1: Add Multi-Agent section after the "Without gVisor" section**

After the `### Without gVisor` section and before `## How Sandboxing Works`, add:

```markdown
### Multi-Agent Teams

Run multiple agents that can communicate with each other on an isolated mesh network:

```bash
# Start Claude + Codex agents (they can discover each other by service name)
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  up claude codex
```

Agents find each other using `.mesh` aliases (`claude.mesh`, `codex.mesh`, etc.). These resolve exclusively to mesh network IPs, guaranteeing reliable connectivity. The mesh is internal — no internet access, no LAN access, agent-to-agent only. Internet traffic still routes through the sandbox network.

> **Note:** Use `docker compose up`, not `run`. The `run` command creates one-off containers without DNS registration, so agents cannot discover each other. Use `.mesh` names (not bare service names) for inter-agent communication.

For advanced networking (sidecars, custom topologies), see [NETWORKING.md](NETWORKING.md).
```

- [ ] **Step 2: Update security summary table**

Replace the existing security summary table with:

```markdown
| Setup | Kernel Isolation | LAN Blocked | Inter-Container | Internet |
|-------|-----------------|-------------|-----------------|----------|
| Default compose (gVisor + sandbox) | gVisor syscall filter | Yes | No | Yes |
| Multi-agent (+ mesh) | gVisor syscall filter | Yes | Mesh only | Yes |
| runc override | Standard Docker (namespaces) | No | Default Docker | Yes |
| `--network none` | N/A | Yes | No | No |
```

- [ ] **Step 3: Update Docker Compose examples section**

After the per-agent compose examples, add multi-agent example:

```markdown
### Multi-Agent Team

```bash
# Claude + Codex team
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  up claude codex

# Without gVisor
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.runc.yml \
  -f docker/docker-compose.multi-agent.yml \
  up claude codex
```
```

- [ ] **Step 4: Commit**

```bash
git add docker/README.md
git commit -m "docs: add multi-agent networking and update security table in README"
```

---

### Task 5: Create docker/NETWORKING.md

**Files:**
- Create: `docker/NETWORKING.md`

- [ ] **Step 1: Write the networking guide**

Write `docker/NETWORKING.md` with these sections:

```markdown
# Unleash Docker Networking Guide

Deep dive into container networking for multi-agent teams and advanced setups.

For quick start, see [README.md](README.md).

## Network Architecture

Unleash uses up to two Docker networks for container isolation:

### unleash-sandbox (external, managed by sandbox-network.sh)

- **Subnet:** 172.30.0.0/16
- **Gateway:** Yes (internet access)
- **iptables:** DROP rules in DOCKER-USER block RFC 1918 (LAN) and link-local
- **Purpose:** Internet access with LAN isolation

### unleash-mesh (inline, created by docker compose)

- **Subnet:** 10.100.0.0/16 (pinned)
- **Gateway:** None (`internal: true`)
- **iptables:** None needed
- **Purpose:** Agent-to-agent communication only

The mesh subnet is pinned to 10.100.0.0/16 (outside 172.16.0.0/12) to avoid
interaction with the sandbox iptables DROP rules.

## Traffic Flow

| From | To | Path | Allowed |
|------|----|------|---------|
| Agent | Internet | sandbox gateway | Yes |
| Agent | LAN (192.168.x) | sandbox gateway -> iptables | Blocked (DROP) |
| Agent | Agent (mesh) | mesh bridge | Yes |
| Agent | Agent (sandbox) | sandbox bridge | Blocked (DROP) |
| Sidecar (mesh-only) | Internet | no gateway | Blocked (no route) |
| Sidecar (mesh-only) | Agent | mesh bridge | Yes |

## Service Discovery

Each agent gets a `.mesh` alias on the mesh network: `claude.mesh`,
`codex.mesh`, `gemini.mesh`, `opencode.mesh`. These aliases resolve
exclusively to mesh IPs (10.100.x.x), avoiding DNS ambiguity when
containers are on multiple networks.

    docker compose -f docker/docker-compose.yml \
      -f docker/docker-compose.multi-agent.yml up claude codex

From the claude container, reach codex via `codex.mesh`.

**Why .mesh aliases?** When containers are on both sandbox and mesh networks,
Docker DNS may return either IP for the bare service name. If it returns the
sandbox IP, the connection is dropped by iptables (containers are in separate
namespaces, so even same-bridge traffic traverses the FORWARD/DOCKER-USER
chain). The `.mesh` alias only exists on the mesh network, so it always
resolves to the correct IP.

**Important:** Use `docker compose up`, not `run`. The `run` command creates
one-off containers that do not register DNS names.

### Verifying DNS

From inside a running container:

    docker compose ... exec claude nslookup codex.mesh

Should resolve to a 10.100.x.x address (mesh network).

## Adding Sidecars (Tier 3)

Sidecars are services that agents need but that are not agents themselves:
MCP servers, databases, caches, message brokers.

### Pattern: Mesh-Only Sidecar

The sidecar joins the mesh network but NOT the sandbox network. It has no
internet access and no LAN access. Agents reach it by service name on mesh.

```yaml
# docker-compose.sidecar.yml
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

    docker compose -f docker/docker-compose.yml \
      -f docker/docker-compose.multi-agent.yml \
      -f docker-compose.sidecar.yml \
      up claude postgres

The agent can reach postgres at `postgres:5432` on the mesh network.
Postgres cannot reach the internet or LAN.

### Pattern: Sidecar With Internet

If the sidecar needs internet (e.g., an MCP server that calls external APIs),
add it to both networks:

```yaml
services:
  mcp-server:
    image: my-mcp-server
    networks:
      - mesh
      - sandbox
```

### Security: Pivot Attack Prevention

**Hard rule:** Sidecars on mesh MUST NOT also be on host-networked or
default-bridge networks.

A sidecar with LAN connectivity becomes a pivot point. A compromised agent
could route traffic: agent -> mesh -> sidecar -> LAN, bypassing the sandbox
firewall entirely.

Safe configurations:
- Sidecar on `mesh` only (no internet, no LAN) — safest
- Sidecar on `mesh` + `sandbox` (internet, no LAN) — safe
- Sidecar on `mesh` + `host`/`default` — **UNSAFE, breaks isolation**

## Compose Override Combinations

| Files | Runtime | Networks | Use Case |
|-------|---------|----------|----------|
| base | runsc | sandbox | Single agent (default) |
| base + runc | runc | default | No gVisor available |
| base + multi-agent | runsc | sandbox + mesh | Agent team |
| base + runc + multi-agent | runc | default + mesh | Agent team, no gVisor |

## Troubleshooting

### Agent cannot reach another agent

1. Verify you used `up`, not `run`
2. Check DNS: `docker compose exec claude nslookup codex`
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
```

- [ ] **Step 2: Commit**

```bash
git add docker/NETWORKING.md
git commit -m "docs: add comprehensive networking guide for advanced setups

Covers network architecture, traffic flows, service discovery,
sidecar patterns with security rules, compose combinations,
and troubleshooting."
```

---

### Task 6: Update docs/DOCUMENTATION_MAP.md

**Files:**
- Modify: `docs/DOCUMENTATION_MAP.md`

- [ ] **Step 1: Add Docker section to the documentation tree**

In the `## Documentation Tree` section, after the `extensions/` tree, add:

```
├── docker/                            # Docker Container Guides
│   ├── README.md                      # Quick start, tiers 1-2
│   └── NETWORKING.md                  # Advanced networking (tier 3)
```

- [ ] **Step 2: Add Docker entries to the decision tree**

Add after the "I want to configure settings" block:

```
┌─ I want to run agents in Docker
│  └─> START: docker/README.md
│      ├─ Single agent? → docker/README.md (Quick Start)
│      ├─ Multi-agent team? → docker/README.md (Multi-Agent Teams)
│      ├─ Sidecars/advanced? → docker/NETWORKING.md
│      └─ Networking issues? → docker/NETWORKING.md (Troubleshooting)
```

- [ ] **Step 3: Add Docker rows to the By Task table**

```markdown
| Run agent in Docker | docker/README.md | docker/NETWORKING.md |
| Multi-agent networking | docker/NETWORKING.md | docker/README.md |
```

- [ ] **Step 4: Commit**

```bash
git add docs/DOCUMENTATION_MAP.md
git commit -m "docs: add Docker networking to documentation map"
```

---

### Task 7: Verify compose merge compatibility

**Files:** None (testing only)

- [ ] **Step 1: Test base only (tier 1)**

Run: `docker compose -f docker/docker-compose.yml config --quiet`
Expected: exits 0

- [ ] **Step 2: Test base + runc**

Run: `docker compose -f docker/docker-compose.yml -f docker/docker-compose.runc.yml config --quiet`
Expected: exits 0

- [ ] **Step 3: Test base + multi-agent**

Run: `docker compose -f docker/docker-compose.yml -f docker/docker-compose.multi-agent.yml config --quiet`
Expected: exits 0 (requires unleash-sandbox network)

- [ ] **Step 4: Test base + runc + multi-agent**

Run: `docker compose -f docker/docker-compose.yml -f docker/docker-compose.runc.yml -f docker/docker-compose.multi-agent.yml config --quiet`
Expected: exits 0

- [ ] **Step 5: Test multi-agent up (if Docker available)**

```bash
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  up -d claude codex
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  exec claude nslookup codex.mesh
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  down
```

Expected: nslookup resolves to 10.100.x.x

- [ ] **Step 6: Push and verify CI**

```bash
git push origin HEAD
```

Expected: CI passes (docker-build workflow)
