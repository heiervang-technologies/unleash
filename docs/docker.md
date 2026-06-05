# Docker Setup

Run agents in sandboxed containers with network isolation.

## Quick Start (via `unleash sandbox`)

The `unleash sandbox` subcommand wraps Docker + gVisor + network isolation into one workflow:

```bash
# One-time setup: installs gVisor, creates network, pulls image, sets up .env
sudo unleash sandbox setup

# Run Claude Code in the sandbox
unleash sandbox run claude

# Open a bash shell in the sandbox
unleash sandbox run

# Check sandbox health
unleash sandbox status
```

**Supported agents in the sandbox:** `claude`, `codex`, `gemini`,
`opencode`, `pi`, plus `bash` and `unleash` itself. Hermes and
Antigravity (`agy`) are not yet wired into the sandbox image / compose
services — run those on the host or build a custom image.

### Named Sandboxes

Run multiple independent sandboxes with `--name`:

```bash
unleash sandbox run --name research claude
unleash sandbox run --name testing bash
```

Each named sandbox gets its own hostname and can run any agent.

### Local API Access

To reach a local inference server (vLLM, Ollama, llama.cpp) from the sandbox:

```bash
sudo unleash sandbox allow-ip 192.168.1.100:8080   # port-restricted (recommended)
sudo unleash sandbox revoke-ip 192.168.1.100:8080   # close when done
```

## Advanced: Direct Docker Compose

For more control, use Docker Compose directly:

### Single Agent

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

## CUDA / GPU Cloud

For GPU workloads (AI training, inference), use the CUDA variant:

```bash
docker run --gpus all -it --rm marksverdhei/unleash:cuda
```

This image adds CUDA 12.8 toolkit, PyTorch with GPU support, and Python 3 on
top of the standard unleash image with all agent CLIs.

### Build Locally

```bash
docker build -f docker/Dockerfile.cuda -t marksverdhei/unleash:cuda .
```

### Docker Compose with GPU

```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.cuda.yml \
  run --rm claude
```

> **Note:** GPU passthrough requires `nvidia-container-toolkit` and the `runc`
> runtime. gVisor cannot expose GPU devices — do not combine with
> `docker-compose.gpu.yml` (Vulkan) or gVisor overrides.

### Vast.ai Deployment

A setup script creates a Vast.ai template from the CUDA image:

```bash
export VAST_API_KEY="your-key"
bash docker/setup-vast-template.sh
```

This creates a template with SSH + Jupyter access. Then launch instances:

```bash
vastai search offers 'gpu_ram>=24 cuda_vers>=12.0' --limit 5
vastai create instance <template-id> <offer-id> --disk 50
```

Inside the instance, all agent CLIs and PyTorch are ready:

```bash
unleash claude                                    # start coding
python3 -c "import torch; print(torch.cuda.is_available())"  # verify GPU
```

## Further Reading

- [docker/README.md](../docker/README.md) -- full Docker reference
- [docker/NETWORKING.md](../docker/NETWORKING.md) -- network architecture
