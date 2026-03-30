# Docker Setup

Run agents in sandboxed containers with network isolation. Three tiers
depending on your needs.

## Quick Start

```bash
# Build the image
docker build -f docker/Dockerfile -t unleash .

# Run a single sandboxed agent
docker compose -f docker/docker-compose.yml run --rm claude
```

## Tiers

### 1. Single Agent (Sandboxed)

One agent in an isolated container with network filtering.

```bash
docker compose -f docker/docker-compose.yml run --rm claude
```

### 2. Multi-Agent Mesh

Multiple agents that can discover each other via `.mesh` DNS aliases.

```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  up claude codex
```

Agents find each other at `claude.mesh`, `codex.mesh`, etc.

### 3. Sidecars

Add services (databases, APIs) alongside agents using additional compose
files or custom overrides.

## Runtime Options

### Without gVisor

By default, compose files assume gVisor (`runsc`) is available for syscall
isolation. If you don't have gVisor installed, overlay the runc fallback:

```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.runc.yml \
  run --rm claude
```

gVisor is recommended for production use. Install it from
[gvisor.dev](https://gvisor.dev/docs/user_guide/install/).

## Network Isolation

The sandbox network blocks LAN traffic (RFC 1918 ranges) while allowing
internet access. Setup:

```bash
sudo ./docker/sandbox-network.sh setup
```

**WARNING:** iptables rules do NOT persist across reboots. Re-run the setup
script after every reboot.

```bash
# Verify rules are active
sudo ./docker/sandbox-network.sh status
```

See [docker/NETWORKING.md](../docker/NETWORKING.md) for advanced network
configuration, custom rules, and multi-host setups.

## Volumes and API Keys

Mount your API keys and config directory into the container. The compose files
handle this by default -- check `docker/docker-compose.yml` for the volume
mappings and adjust paths as needed.

## Further Reading

- [docker/README.md](../docker/README.md) -- full Docker reference
- [docker/NETWORKING.md](../docker/NETWORKING.md) -- network architecture
