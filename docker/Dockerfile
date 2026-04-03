# unleash Docker Image
#
# Sandboxed container with all 4 supported coder CLIs:
#   - Claude Code (Anthropic)
#   - Codex (OpenAI)
#   - Gemini CLI (Google)
#   - OpenCode
#
# Build:
#   docker build -f docker/Dockerfile -t unleash .
#
# Run:
#   docker run -it --rm \
#     -e ANTHROPIC_API_KEY \
#     -v $(pwd):/workspace \
#     unleash
#
# Two-stage build:
#   1. rust-builder  - Compiles unleash from source
#   2. runtime       - Ubuntu 24.04 with Node 22, all CLIs installed
#
# Ubuntu 24.04 is used as runtime base because it ships GLIBC 2.39,
# required by Codex prebuilt binaries (Debian Bookworm only has 2.36).

# ============================================================
# Stage 1: Build unleash from Rust source
# ============================================================
FROM rust:1.88-bookworm AS rust-builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# -- Build unleash (with TUI support) --
# Dependency caching: copy manifests first, build deps, then copy source
COPY Cargo.toml Cargo.lock build.rs ./
COPY data/ data/
COPY assets/ assets/
RUN mkdir src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release 2>/dev/null || true \
    && rm -rf src

COPY src/ src/
RUN cargo build --release \
    && strip target/release/unleash

# ============================================================
# Stage 2: Runtime image (Ubuntu 24.04 for GLIBC 2.39)
# ============================================================
FROM ubuntu:24.04 AS runtime

# Metadata
LABEL maintainer="Heiervang Technologies"
LABEL description="unleash - Sandboxed container with Claude, Codex, Gemini, and OpenCode CLIs"

# Install Node 22 via nodesource + GitHub CLI + runtime dependencies
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       curl ca-certificates gnupg \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
       | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
       > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
       nodejs \
       gh \
       git \
       openssh-client \
       tmux \
       jq \
       mesa-vulkan-drivers \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for sandboxing
RUN useradd -m -s /bin/bash unleash \
    && mkdir -p /home/unleash/.local/bin /home/unleash/.npm-global

# -- Install unleash binary from builder stage --
COPY --from=rust-builder /build/target/release/unleash /usr/local/bin/unleash

# -- Install plugins --
COPY plugins/bundled/ /usr/local/share/unleash/plugins/

# -- Install helper scripts --
COPY scripts/unleash-refresh /usr/local/bin/unleash-refresh
COPY scripts/unleash-exit /usr/local/bin/unleash-exit
RUN chmod +x /usr/local/bin/unleash-refresh /usr/local/bin/unleash-exit

# Setup directories for unleash user
RUN mkdir -p /home/unleash/.config/unleash \
    /home/unleash/.cache/unleash \
    /home/unleash/.local/share/unleash/plugins \
    /home/unleash/.claude \
    /workspace \
    && chown -R unleash:unleash /home/unleash /workspace

# Pre-seed ~/.claude.json to skip onboarding wizard, workspace trust, and
# bypass-permissions confirmation dialogs.
# Workaround for https://github.com/anthropics/claude-code/issues/8938
RUN echo '{"numStartups":1,"hasCompletedOnboarding":true,"bypassPermissionsModeAccepted":true,"projects":{"/workspace":{"hasTrustDialogAccepted":true,"allowedTools":[]}}}' \
    > /home/unleash/.claude.json \
    && chown unleash:unleash /home/unleash/.claude.json

# Symlink plugins to user data dir
RUN ln -sf /usr/local/share/unleash/plugins/* /home/unleash/.local/share/unleash/plugins/ 2>/dev/null || true

# Environment defaults (set before USER so they apply to unleash update)
ENV AGENT_UNLEASH=1
ENV HOME=/home/unleash
ENV PATH="/home/unleash/.local/bin:/home/unleash/.npm-global/bin:${PATH}"
ENV NPM_CONFIG_PREFIX=/home/unleash/.npm-global

# Switch to non-root user
USER unleash
WORKDIR /workspace

# -- Install all 4 agent CLIs using unleash's own install paths --
# Claude Code: native GCS binary (not npm)
# Codex: prebuilt binary from GitHub releases
# Gemini CLI: npm
# OpenCode: npm (installed separately — unleash install doesn't support it yet)
RUN unleash install claude codex gemini \
    && npm install -g opencode-ai@latest

# Verify all CLIs are installed (build will fail if any are missing)
RUN claude --version && echo "✓ Claude Code" \
    && codex --version && echo "✓ Codex" \
    && gemini --version && echo "✓ Gemini CLI" \
    && opencode --version && echo "✓ OpenCode" \
    && unleash --version && echo "✓ unleash"

# Default entrypoint: run unleash TUI (interactive) or unleashed (headless)
ENTRYPOINT ["unleash"]
