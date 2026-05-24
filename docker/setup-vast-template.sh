#!/usr/bin/env bash
# Vast.ai template setup for unleash CUDA image
#
# Prerequisites:
#   1. vastai CLI installed (pip3 install vastai)
#   2. API key set: export VAST_API_KEY="your-key-here"
#   3. Image pushed to Docker Hub: marksverdhei/unleash:cuda
#
# Usage:
#   export VAST_API_KEY="..."
#   bash docker/setup-vast-template.sh
#
# This creates the template and prints next steps for launching instances.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ── Config ──────────────────────────────────────────────────────────
IMAGE="marksverdhei/unleash"
TAG="cuda"
TEMPLATE_NAME="Unleash CUDA"
TEMPLATE_DESC="GPU-enabled dev environment: CUDA 12.8, PyTorch, all 5 agent CLIs (Claude Code, Codex, Gemini, OpenCode, Pi), unleash session crossloader. SSH + Jupyter."
DISK_GB=50
REPO_URL="https://github.com/heiervang-technologies/unleash"

# On-start script runs when the instance boots (as root).
# Sets up the unleash environment, installs any missing models, and
# starts Jupyter if requested.
ONSTART=$(cat <<'SCRIPT'
#!/bin/bash
set -euo pipefail

# Create unleash user data dir if missing
mkdir -p /home/unleash/.config/unleash /home/unleash/.local/share/unleash/plugins
chown -R unleash:unleash /home/unleash 2>/dev/null || true

# Expose CUDA info
echo "=== CUDA ==="
nvidia-smi 2>/dev/null || echo "nvidia-smi not available"
echo ""
echo "=== PyTorch ==="
python3 -c "import torch; print(f'PyTorch {torch.__version__}, CUDA available: {torch.cuda.is_available()}')" 2>/dev/null || echo "PyTorch not installed"

# Print unleash version
echo "=== Unleash ==="
unleash --version 2>/dev/null || echo "unleash not in PATH"
echo ""
echo "Instance ready. Run 'unleash search' to find and crossload sessions."
SCRIPT
)

# ── Check prerequisites ─────────────────────────────────────────────
if [ -z "${VAST_API_KEY:-}" ]; then
  if [ -f ~/.config/vastai/vast_api_key ]; then
    VAST_API_KEY="$(cat ~/.config/vastai/vast_api_key)"
  else
    echo "ERROR: VAST_API_KEY not set and ~/.config/vastai/vast_api_key not found."
    echo "  export VAST_API_KEY='your-key-here'"
    exit 1
  fi
fi

# Check if image is on Docker Hub (optional — helps catch typos early)
echo "Checking image availability..."
if docker manifest inspect "${IMAGE}:${TAG}" >/dev/null 2>&1; then
  echo "  ✓ ${IMAGE}:${TAG} found on Docker Hub"
else
  echo "  ⚠ ${IMAGE}:${TAG} not found on Docker Hub yet."
  echo "    Build and push it first:"
  echo "      docker build -f ${SCRIPT_DIR}/Dockerfile.cuda -t ${IMAGE}:${TAG} ."
  echo "      docker push ${IMAGE}:${TAG}"
  echo ""
  read -rp "Continue anyway? [y/N] " confirm
  if [[ ! "$confirm" =~ ^[yY] ]]; then
    echo "Aborted."
    exit 1
  fi
fi

# ── Create template ─────────────────────────────────────────────────
echo ""
echo "Creating Vast.ai template: ${TEMPLATE_NAME}..."
echo ""

TEMPLATE_OUTPUT=$(vastai create template \
  --name "${TEMPLATE_NAME}" \
  --image "${IMAGE}" \
  --image_tag "${TAG}" \
  --disk_space "${DISK_GB}" \
  --ssh \
  --jupyter \
  --jupyter-lab \
  --direct \
  --repo "${REPO_URL}" \
  --desc "${TEMPLATE_DESC}" \
  --env 'PORTS=8080/tcp,22/tcp' \
  --onstart-cmd "${ONSTART}" \
  --public 2>&1)

echo "${TEMPLATE_OUTPUT}"

# Extract template ID from output
TEMPLATE_ID=$(echo "${TEMPLATE_OUTPUT}" | grep -oE '[0-9]+' | head -1)

if [ -n "${TEMPLATE_ID}" ]; then
  echo ""
  echo "✓ Template created! ID: ${TEMPLATE_ID}"
  echo ""
  echo "Next steps:"
  echo "  1. Launch an instance from the template:"
  echo "     vastai search offers 'gpu_ram>=-24 cpu_cores>=8' --storage 50"
  echo "     vastai create instance ${TEMPLATE_ID} <offer-id> --disk 50"
  echo ""
  echo "  2. Or launch from the Vast.ai web UI:"
  echo "     https://vast.ai/console/create/"
  echo "       → Search for template: '${TEMPLATE_NAME}'"
  echo ""
  echo "  3. Once running, connect via SSH (vastai provides the command)"
  echo "     or open Jupyter lab in your browser."
  echo ""
  echo "  4. Inside the instance:"
  echo "     unleash search  # find and crossload sessions"
  echo "     python3 -c \"import torch; print(torch.cuda.is_available())\""
  echo ""
else
  echo ""
  echo "⚠ Template may have failed. Check the output above."
  echo "  Try: vastai create template --help"
fi