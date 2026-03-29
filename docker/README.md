# Unleash Docker Container

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

### Without Sandbox

If gVisor is not available, use the unsandboxed override:

```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.unsandboxed.yml \
  run --rm claude
```

Or with plain docker run:

```bash
docker run -it --rm \
  -e CLAUDE_CODE_OAUTH_TOKEN \
  -v $(pwd):/workspace \
  unleash claude
```

**Warning:** Without gVisor, the container shares the host kernel. Agents run with `--dangerously-skip-permissions` and have full network access including your LAN.

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

| Setup | Kernel Isolation | LAN Blocked | Internet |
|-------|-----------------|-------------|----------|
| Default compose (gVisor + sandbox network) | gVisor syscall filter | Yes | Yes |
| Unsandboxed override | None (shared kernel) | No | Yes |
| `--network none` | N/A | Yes | No |

## Authentication

Generate a token on your host machine, then pass it to the container:

```bash
claude setup-token
export CLAUDE_CODE_OAUTH_TOKEN=<your-token>
```

| Agent | Environment Variable |
|-------|---------------------|
| Claude Code | `CLAUDE_CODE_OAUTH_TOKEN` |
| Codex | `OPENAI_API_KEY` |
| Gemini CLI | `GEMINI_API_KEY` |
| OpenCode | *(uses configured provider)* |

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

1. **Rust builder** (`rust:1.88-bookworm`) — compiles Unleash with dependency caching
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
