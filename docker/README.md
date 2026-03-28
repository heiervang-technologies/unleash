# Unleash Docker Container

Sandboxed container with all 4 supported coder CLIs pre-installed:

- **Claude Code** (Anthropic) — `claude`
- **Codex** (OpenAI) — `codex`
- **Gemini CLI** (Google) — `gemini`
- **OpenCode** — `opencode`

One image, all agents ready to go.

## Quick Start

```bash
# Build the image
docker build -f docker/Dockerfile -t unleash .

# Run Claude Code interactively
docker run -it --rm \
  -e CLAUDE_CODE_OAUTH_TOKEN \
  -v $(pwd):/workspace \
  unleash claude

# Run the Unleash TUI
docker run -it --rm \
  -e CLAUDE_CODE_OAUTH_TOKEN \
  -v $(pwd):/workspace \
  unleash
```

## Sandboxed Mode (Sysbox)

By default, containers run with standard `runc` which shares the host kernel. For proper sandboxing, use [sysbox](https://github.com/nestybox/sysbox) as the container runtime.

### What Sysbox Provides

- **User namespace isolation** — inner root is unprivileged on the host
- **Process isolation** — container cannot see host processes
- **Filesystem isolation** — proper `/proc` and `/sys` virtualization
- **Full network access** — agents can call APIs, pull packages, and interact with services normally

### Setup

```bash
# Install sysbox (see https://github.com/nestybox/sysbox for your distro)
# Then enable the service:
sudo systemctl enable --now sysbox
```

### Running with Sysbox

```bash
# Direct docker run
docker run --runtime=sysbox-runc -it --rm \
  -e CLAUDE_CODE_OAUTH_TOKEN \
  -v $(pwd):/workspace \
  unleash claude

# Via docker compose
CONTAINER_RUNTIME=sysbox-runc docker compose -f docker/docker-compose.yml run --rm claude

# Codex with sysbox
docker run --runtime=sysbox-runc -it --rm \
  -e OPENAI_API_KEY \
  -v $(pwd):/workspace \
  unleash codex
```

### Security Note

Even without sysbox, the container runs as a non-root user (`unleash`), which limits damage from accidental operations. However, without sysbox:

- Agents run with `--dangerously-skip-permissions` and could modify anything in the mounted workspace
- A container escape via kernel vulnerability is theoretically possible
- Network access is unrestricted

**For production or untrusted code review, always use sysbox.** The combination of sysbox isolation + full network access gives agents the API connectivity they need while preventing host compromise.

## Authentication

You need a valid auth token. Generate one on your host machine:

```bash
claude setup-token
export CLAUDE_CODE_OAUTH_TOKEN=<your-token>
```

Then pass it to the container via `-e CLAUDE_CODE_OAUTH_TOKEN`.

For other agents, pass their respective API keys:

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

By default, mounts the current directory as `/workspace`. Override with:

```bash
WORKSPACE_DIR=/path/to/project docker compose -f docker/docker-compose.yml run --rm claude
```

## How Onboarding is Skipped

Claude Code normally shows three interactive dialogs on first run:
1. **Theme picker** — choose dark/light mode
2. **Workspace trust** — confirm you trust the project directory
3. **Bypass permissions warning** — accept the `--dangerously-skip-permissions` risk

The Dockerfile pre-seeds `~/.claude.json` with flags to skip all three:

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

This is a workaround for [anthropics/claude-code#8938](https://github.com/anthropics/claude-code/issues/8938). If Claude Code changes how onboarding state is stored, this may need updating.

## Architecture

Two-stage Docker build:

1. **Rust builder** (`rust:1.88-bookworm`) — compiles Unleash from source with dependency caching
2. **Runtime** (`ubuntu:24.04`) — Ubuntu 24.04 for GLIBC 2.39 (required by Codex prebuilt binaries), Node 22 via nodesource, GitHub CLI, all agent CLIs installed via `unleash update`

### Why Ubuntu 24.04?

Codex prebuilt binaries from GitHub releases require GLIBC 2.38+. Debian Bookworm only ships GLIBC 2.36. Ubuntu 24.04 ships GLIBC 2.39.

### CLI Install Paths

The Dockerfile uses `unleash update` to install CLIs via the same paths as the host tool:

| Agent | Install Method |
|-------|---------------|
| Claude Code | Native GCS binary (not npm) |
| Codex | Prebuilt binary from GitHub releases |
| Gemini CLI | npm |
| OpenCode | npm |

### Included Tools

- `unleash` — Unleash wrapper with TUI, polyfill, plugins
- `claude`, `codex`, `gemini`, `opencode` — Agent CLIs
- `gh` — GitHub CLI
- `git`, `tmux`, `jq`, `curl` — Standard dev tools
