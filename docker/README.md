# unleash Docker Container

Sandboxed container with all 4 supported coder CLIs pre-installed:

- **Claude Code** (Anthropic) — `claude`
- **Codex** (OpenAI) — `codex`
- **Gemini CLI** (Google) — `gemini`
- **OpenCode** — `opencode`

One image, all agents ready to go.

## Quick Start (Sandboxed)

The recommended setup uses [gVisor](https://gvisor.dev/) for syscall-level isolation and a firewall that blocks LAN access while allowing internet.

```bash
# 1. Build the image
docker build -f docker/Dockerfile -t unleash .

# 2. One-time: install gVisor and create sandbox network
sudo runsc install && sudo systemctl restart docker
sudo ./docker/sandbox-network.sh setup

# 3. Run Claude Code (sandboxed, internet-only, no LAN access)
docker compose -f docker/docker-compose.yml run --rm claude
```

That's it. The container has full internet access (API calls, npm, git) but cannot reach your local network.

### Without gVisor

If gVisor is not available, use the runc override:

```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.runc.yml \
  run --rm claude
```

Or with plain docker run:

```bash
docker run -it --rm \
  -e CLAUDE_CODE_OAUTH_TOKEN \
  -v $(pwd):/workspace \
  unleash claude
```

**Warning:** Without gVisor, the container uses standard Docker isolation (runc). This still provides namespace and cgroup isolation, but agents run with `--dangerously-skip-permissions` and have full network access including your LAN.

### Multi-Agent Teams

Run multiple agents that can communicate with each other on an isolated mesh network:

```bash
# Start Claude + Codex agents (they discover each other via .mesh aliases)
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.multi-agent.yml \
  up claude codex
```

Agents find each other using `.mesh` aliases (`claude.mesh`, `codex.mesh`, etc.). These resolve exclusively to mesh network IPs, guaranteeing reliable connectivity. The mesh is internal — no internet access, no LAN access, agent-to-agent only. Internet traffic still routes through the sandbox network.

> **Note:** Use `docker compose up`, not `run`. The `run` command creates one-off containers without DNS registration, so agents cannot discover each other. Use `.mesh` names for inter-agent communication.

For advanced networking (sidecars, custom topologies), see [NETWORKING.md](NETWORKING.md).

## How Sandboxing Works

### gVisor (runsc)

gVisor intercepts every syscall through its own application kernel, so even if an agent runs malicious code, it cannot directly interact with the host kernel. This is stronger than namespace isolation alone.

### Network Isolation

The `sandbox-network.sh` script creates a Docker network with iptables rules that:

- **Allow** all internet traffic (APIs, package registries, git remotes)
- **Block** RFC 1918 private ranges (10.x, 172.16-31.x, 192.168.x)
- **Block** link-local (169.254.x)

```bash
sudo ./docker/sandbox-network.sh setup     # Create network + firewall rules
sudo ./docker/sandbox-network.sh teardown   # Remove everything
sudo ./docker/sandbox-network.sh status    # Check current state
```

> **Important:** The iptables firewall rules do **not** persist across reboots. The Docker network survives restarts, but the LAN-blocking rules are lost — containers will silently regain full LAN access. Re-run `sudo ./docker/sandbox-network.sh setup` after each reboot, or add it to a startup script (e.g. a systemd unit or cron `@reboot`).

### Security Summary

| Setup | Kernel Isolation | LAN Blocked | Inter-Container | Internet |
|-------|-----------------|-------------|-----------------|----------|
| Default compose (gVisor + sandbox) | gVisor syscall filter | Yes | No | Yes |
| Multi-agent (+ mesh) | gVisor syscall filter | Yes | Mesh only | Yes |
| runc override | Standard Docker (namespaces) | No | Default Docker | Yes |
| `--network none` | N/A | Yes | No | No |

## Authentication

Copy the example env file, fill in your keys, and docker compose will pick them up automatically:

```bash
cp docker/example.env docker/.env
# Edit docker/.env with your keys
```

The `.env` file is gitignored — never commit real credentials.

| Agent | Environment Variable | How to Get |
|-------|---------------------|------------|
| Claude Code | `CLAUDE_CODE_OAUTH_TOKEN` | `claude setup-token` |
| Codex | `OPENAI_API_KEY` | platform.openai.com |
| Gemini CLI | `GEMINI_API_KEY` | aistudio.google.com |
| OpenCode | *(uses configured provider)* | Depends on backend |

**Security note:** These keys are passed into a container where AI agents run with full code execution. Use scoped, rotatable keys with spending limits — not your personal all-access keys. See `docker/example.env` for detailed guidance.

## Using a Local OpenAI-Compatible API

If you run a local inference server (vLLM, Ollama, llama.cpp, TGI, etc.) and want the sandboxed container to reach it, you need to open a single IP through the firewall.

### Step 1: Open the IP

```bash
# Allow containers to reach your local API server (e.g., on 192.168.1.100)
sudo ./docker/sandbox-network.sh allow-ip 192.168.1.100
```

This inserts an ACCEPT rule *before* the DROP rules, so only that specific IP is reachable. All other LAN addresses remain blocked.

### Step 2: Set the endpoint

Add to your `docker/.env`:

```bash
LOCAL_API_BASE=http://192.168.1.100:8080/v1
```

### Step 3: Run with the local-api overlay

```bash
docker compose -f docker/docker-compose.yml \
  -f docker/docker-compose.local-api.yml \
  run --rm opencode
```

### Step 4: Revoke when done

```bash
sudo ./docker/sandbox-network.sh revoke-ip 192.168.1.100
```

> **Security warning:** Opening a LAN IP increases the attack surface. A compromised agent could send arbitrary requests to that endpoint or probe other services on the same host. Mitigations:
>
> - Run the local API in its own container or VM — not bare-metal alongside sensitive services
> - Bind the API server to a single interface/port, not `0.0.0.0`
> - Use API-key authentication even for local endpoints
> - Monitor API logs for unexpected requests
>
> The firewall exception does **not** survive reboots. Re-run `allow-ip` after restart.

## Verifying the Sandbox

Run the integration tests to confirm gVisor and LAN isolation are working:

```bash
# Build the image first
docker build -f docker/Dockerfile -t unleash .

# Full test suite (requires sudo for iptables checks + container spawning)
sudo ./tests/test_sandbox_network.sh

# Quick mode (internet + DNS only, skips LAN probe)
sudo ./tests/test_sandbox_network.sh quick
```

The tests verify: internet connectivity, DNS resolution, LAN blocking for all RFC 1918 ranges, gVisor syscall filtering, and that all 5 CLIs are installed.

## Docker Compose

```bash
# Claude Code
docker compose -f docker/docker-compose.yml run --rm claude

# Codex
docker compose -f docker/docker-compose.yml run --rm codex

# Gemini CLI
docker compose -f docker/docker-compose.yml run --rm gemini

# OpenCode
docker compose -f docker/docker-compose.yml run --rm opencode
```

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

Mount a different project:

```bash
WORKSPACE_DIR=/path/to/project docker compose -f docker/docker-compose.yml run --rm claude
```

## How Onboarding is Skipped

Claude Code normally shows three interactive dialogs on first run. The Dockerfile pre-seeds `~/.claude.json` to skip all three:

```json
{
  "numStartups": 1,
  "hasCompletedOnboarding": true,
  "bypassPermissionsModeAccepted": true,
  "projects": {
    "/workspace": {
      "hasTrustDialogAccepted": true,
      "allowedTools": []
    }
  }
}
```

Workaround for [anthropics/claude-code#8938](https://github.com/anthropics/claude-code/issues/8938).

## Architecture

Two-stage Docker build:

1. **Rust builder** (`rust:1.88-bookworm`) — compiles unleash with dependency caching
2. **Runtime** (`ubuntu:24.04`) — GLIBC 2.39 (required by Codex prebuilt binaries), Node 22, GitHub CLI, all agent CLIs installed via `unleash update`

### CLI Install Paths

| Agent | Install Method |
|-------|---------------|
| Claude Code | Native GCS binary (not npm) |
| Codex | Prebuilt binary from GitHub releases |
| Gemini CLI | npm |
| OpenCode | npm |

### Included Tools

`unleash`, `claude`, `codex`, `gemini`, `opencode`, `gh`, `git`, `tmux`, `jq`, `curl`
