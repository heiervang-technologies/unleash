# Unleash Docker Image
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
#     -e OPENAI_API_KEY \
#     -e GEMINI_API_KEY \
#     -v $(pwd):/workspace \
#     unleash
#
# Multi-stage build:
#   1. rust-builder  - Compiles Unleash + Codex from source
#   2. runtime       - Slim image with all CLIs installed

# ============================================================
# Stage 1: Build Unleash and Codex from Rust source
# ============================================================
FROM rust:1.87-bookworm AS rust-builder

WORKDIR /build

# Install build dependencies (libcap-dev needed for Codex bubblewrap sandbox)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libcap-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# -- Build Unleash (headless, no TUI) --
COPY Cargo.toml Cargo.lock build.rs ./
COPY src/ src/
COPY data/ data/
COPY assets/ assets/

RUN cargo build --release --no-default-features \
    && strip target/release/unleash target/release/unleashed target/release/u

# -- Build Codex from source --
RUN git clone --depth 1 https://github.com/openai/codex.git /build/codex-src \
    && cd /build/codex-src/codex-rs \
    && cargo build --release -p codex-cli \
    && strip /build/codex-src/codex-rs/target/release/codex

# ============================================================
# Stage 2: Runtime image with all CLIs
# ============================================================
FROM node:22-bookworm-slim AS runtime

# Metadata
LABEL maintainer="Heiervang Technologies"
LABEL description="Unleash - Sandboxed container with Claude, Codex, Gemini, and OpenCode CLIs"

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    curl \
    ca-certificates \
    openssh-client \
    tmux \
    jq \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for sandboxing
RUN useradd -m -s /bin/bash unleash

# -- Install Rust binaries from builder stage --
COPY --from=rust-builder /build/target/release/unleash /usr/local/bin/unleash
COPY --from=rust-builder /build/target/release/unleashed /usr/local/bin/unleashed
COPY --from=rust-builder /build/target/release/u /usr/local/bin/u
COPY --from=rust-builder /build/codex-src/codex-rs/target/release/codex /usr/local/bin/codex

# -- Install npm-based CLIs globally --
RUN npm install -g --no-fund --no-audit \
    @anthropic-ai/claude-code@latest \
    @google/gemini-cli@latest \
    opencode-ai@latest

# -- Install plugins --
COPY plugins/unleashed/ /usr/local/share/unleash/plugins/

# -- Install helper scripts --
COPY scripts/restart-claude /usr/local/bin/restart-claude
COPY scripts/exit-claude /usr/local/bin/exit-claude
RUN chmod +x /usr/local/bin/restart-claude /usr/local/bin/exit-claude

# Setup directories for unleash user
RUN mkdir -p /home/unleash/.config/unleash \
    /home/unleash/.cache/unleash \
    /home/unleash/.local/share/unleash/plugins \
    /workspace \
    && chown -R unleash:unleash /home/unleash /workspace

# Symlink plugins to user data dir
RUN ln -sf /usr/local/share/unleash/plugins/* /home/unleash/.local/share/unleash/plugins/ 2>/dev/null || true

# Switch to non-root user
USER unleash
WORKDIR /workspace

# Environment defaults
ENV AGENT_UNLEASHED=1
ENV HOME=/home/unleash

# Verify all CLIs are installed (build will fail if any are missing)
RUN claude --version && echo "✓ Claude Code" \
    && codex --version && echo "✓ Codex" \
    && gemini --version && echo "✓ Gemini CLI" \
    && opencode --version && echo "✓ OpenCode" \
    && unleash --version && echo "✓ Unleash"

# Default entrypoint: run unleash wrapper in headless mode
ENTRYPOINT ["unleashed"]
CMD ["--dangerously-skip-permissions"]
