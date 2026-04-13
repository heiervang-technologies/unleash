#!/usr/bin/env bash
# install.sh - Install unleash
#
# This script:
# 1. Detects supported agent CLIs (Claude, Codex, Gemini, OpenCode)
# 2. Builds the CLI binaries (if cargo available)
# 3. Creates symlinks in ~/.local/bin/
# 4. Installs plugins to ~/.local/share/unleash/plugins
#
# Agent CLIs are managed separately via `unleash agents`.
#
# Usage: ./scripts/install.sh [--no-build] [--bin-dir DIR]
#
# Options:
#   --no-build         Skip building the TUI
#   --bin-dir DIR      Install to DIR instead of ~/.local/bin

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info() { echo -e "${BLUE}==>${NC} $1"; }
success() { echo -e "${GREEN}==>${NC} $1"; }
warn() { echo -e "${YELLOW}==>${NC} $1"; }
error() { echo -e "${RED}==>${NC} $1"; }

# Supported agent CLIs and their version flags
declare -A AGENT_BINARIES=(
    [claude]="Claude Code"
    [codex]="Codex"
    [gemini]="Gemini CLI"
    [opencode]="OpenCode"
)

# Detect installed agent CLIs
detect_agents() {
    local found=0
    for bin in claude codex gemini opencode; do
        local display="${AGENT_BINARIES[$bin]}"
        if command -v "$bin" &> /dev/null; then
            local version
            version=$("$bin" --version 2>/dev/null | head -1 || echo "unknown")
            success "$display detected: $version"
            found=$((found + 1))
        fi
    done

    if [[ $found -eq 0 ]]; then
        warn "No supported agent CLIs found"
        echo "    Install agents via: unleash agents update <name>"
        echo "    Supported: claude, codex, gemini, opencode"
    else
        info "$found agent CLI(s) detected"
    fi
    echo ""
}

# Parse arguments
BUILD_TUI=true
BIN_DIR="${HOME}/.local/bin"
INTERACTIVE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --no-build)
            BUILD_TUI=false
            shift
            ;;
        --bin-dir)
            BIN_DIR="$2"
            shift 2
            ;;
        -i|--interactive)
            INTERACTIVE=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --no-build          Skip building the TUI"
            echo "  --bin-dir DIR       Install to DIR instead of ~/.local/bin"
            echo "  -i, --interactive   Pick an agent, install its CLI, and launch it"
            echo "  -h, --help          Show this help"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Interactive splash screen — use the splash binary if available
if $INTERACTIVE; then
    SPLASH_BIN="${REPO_ROOT}/target/release/splash"
    if [[ ! -x "$SPLASH_BIN" ]]; then
        SPLASH_BIN="${BIN_DIR}/splash"
    fi
    if [[ -x "$SPLASH_BIN" ]]; then
        SELECTED_AGENT=$("$SPLASH_BIN") || exit 0
        info "Selected agent: $SELECTED_AGENT"
    else
        # Fallback if splash binary not built yet
        clear
        if [[ -f "$REPO_ROOT/src/assets/mascot.claude.ans" ]]; then
            cat "$REPO_ROOT/src/assets/mascot.claude.ans"
        fi
        echo ""
        echo -e "${GREEN}Press Enter to continue...${NC}"
        read -r
        clear
    fi
fi

echo ""
echo "╭─────────────────────────────────────╮"
echo "│          unleash Installer          │"
echo "╰─────────────────────────────────────╯"
echo ""

# Ensure bin directory exists
mkdir -p "$BIN_DIR"

# Check if bin directory is in PATH
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
    warn "$BIN_DIR is not in your PATH"
    echo "    Add this to your shell config:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
fi

# Step 0: Detect installed agent CLIs
info "Detecting agent CLIs..."
detect_agents

# Step 1: Build CLI binaries
if $BUILD_TUI; then
    if command -v cargo &> /dev/null; then
        info "Building CLI binaries..."
        cd "$REPO_ROOT"

        # Detect fast linker (clang + mold) and use if available
        if command -v clang &> /dev/null && command -v mold &> /dev/null; then
            info "Using fast linker: clang + mold"
            export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang
            export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-C link-arg=-fuse-ld=mold"
        else
            if ! command -v mold &> /dev/null; then
                info "mold not found, using default linker (install mold for faster builds)"
            fi
        fi

        if cargo build --release; then
            success "CLI built successfully"

            # Install unleash binary
            if [[ -f "$REPO_ROOT/target/release/unleash" ]]; then
                cp "$REPO_ROOT/target/release/unleash" "$BIN_DIR/unleash"
                chmod +x "$BIN_DIR/unleash"
                success "Installed: unleash"
            fi

        else
            warn "Build failed, continuing without CLI binaries"
            BUILD_TUI=false
        fi
    else
        warn "Cargo not found, skipping CLI build"
        warn "Install Rust to build the CLI: https://rustup.rs"
        BUILD_TUI=false
    fi
fi

# Step 2: Create symlinks for additional commands
info "Creating symlinks..."

# Helper commands (new canonical names)
ln -sf "$SCRIPT_DIR/unleash-refresh" "$BIN_DIR/unleash-refresh"
ln -sf "$SCRIPT_DIR/unleash-exit" "$BIN_DIR/unleash-exit"
success "Symlink: unleash-refresh, unleash-exit"

# Backward-compat aliases (old names point to new scripts)
ln -sf "$BIN_DIR/unleash-refresh" "$BIN_DIR/restart-claude"
ln -sf "$BIN_DIR/unleash-exit" "$BIN_DIR/exit-claude"
success "Symlink (compat): restart-claude -> unleash-refresh, exit-claude -> unleash-exit"

# Step 3: Install plugins globally
info "Installing plugins..."
PLUGINS_DIR="${HOME}/.local/share/unleash/plugins"
mkdir -p "$PLUGINS_DIR"

if [[ -d "$REPO_ROOT/plugins/bundled" ]]; then
    cp -r "$REPO_ROOT/plugins/bundled/"* "$PLUGINS_DIR/"
    success "Plugins installed to $PLUGINS_DIR"
    echo "  • auto-mode"
    echo "  • mcp-refresh"
    echo "  • process-restart"
else
    warn "Plugin directory not found: $REPO_ROOT/plugins/bundled"
fi

# Step 3b: Install docker files (for sandbox support from any directory)
info "Installing docker files..."
DOCKER_DIR="${HOME}/.local/share/unleash/docker"
mkdir -p "$DOCKER_DIR"

if [[ -d "$REPO_ROOT/docker" ]]; then
    cp "$REPO_ROOT/docker/Dockerfile" "$DOCKER_DIR/"
    cp "$REPO_ROOT/docker/docker-compose.yml" "$DOCKER_DIR/"
    cp "$REPO_ROOT/docker/entrypoint.sh" "$DOCKER_DIR/"
    chmod +x "$DOCKER_DIR/entrypoint.sh"
    cp "$REPO_ROOT/docker/sandbox-network.sh" "$DOCKER_DIR/" 2>/dev/null || true
    chmod +x "$DOCKER_DIR/sandbox-network.sh" 2>/dev/null || true
    # Copy optional files
    for f in example.env starship.toml docker-compose.*.yml; do
        cp "$REPO_ROOT/docker/$f" "$DOCKER_DIR/" 2>/dev/null || true
    done
    success "Docker files installed to $DOCKER_DIR"
else
    warn "Docker directory not found: $REPO_ROOT/docker"
fi

# Step 4: Print summary
echo ""
echo "╭─────────────────────────────────────╮"
echo "│        Installation Complete        │"
echo "╰─────────────────────────────────────╯"
echo ""
echo "CLI Commands:"
echo "  unleash              - Launch TUI for profile/version management"
echo "  unleash <agent>      - Start an agent (claude, codex, gemini, opencode)"
echo "  unleash agents       - Manage agent CLI installations and versions"
echo ""
echo "Helper Commands:"
echo "  unleash-refresh  - Restart agent (preserves session)"
echo "  unleash-exit     - Exit agent and wrapper"
echo "  (old names restart-claude / exit-claude still work)"
echo ""
echo "Quick start:"
echo "  unleash claude         - Start Claude with wrapper features"
echo "  unleash claude --auto  - Start in auto mode"
echo ""

if ! $BUILD_TUI; then
    echo "Note: CLI not built. Install Rust and run:"
    echo "  cd $REPO_ROOT && cargo build --release"
    echo ""
fi

success "Done!"

# Step 5: If an agent was selected via splash, install its CLI and launch
if [[ -n "${SELECTED_AGENT:-}" ]]; then
    UNLEASH_BIN="${BIN_DIR}/unleash"
    if [[ ! -x "$UNLEASH_BIN" ]]; then
        UNLEASH_BIN="${REPO_ROOT}/target/release/unleash"
    fi

    if [[ -x "$UNLEASH_BIN" ]]; then
        # Install the agent CLI if not already present
        if ! command -v "$SELECTED_AGENT" &> /dev/null; then
            info "Installing $SELECTED_AGENT CLI..."
            "$UNLEASH_BIN" install "$SELECTED_AGENT" || warn "Could not install $SELECTED_AGENT CLI"
        fi

        # Set the selected agent as the default profile
        CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/unleash"
        CONFIG_FILE="$CONFIG_DIR/config.toml"
        mkdir -p "$CONFIG_DIR"
        if [[ -f "$CONFIG_FILE" ]]; then
            # Update existing config
            if grep -q '^current_profile' "$CONFIG_FILE"; then
                sed -i "s/^current_profile.*/current_profile = \"$SELECTED_AGENT\"/" "$CONFIG_FILE"
            else
                echo "current_profile = \"$SELECTED_AGENT\"" >> "$CONFIG_FILE"
            fi
        else
            echo "current_profile = \"$SELECTED_AGENT\"" > "$CONFIG_FILE"
        fi
        info "Default profile set to $SELECTED_AGENT"

        # Launch the TUI
        exec "$UNLEASH_BIN"
    fi
fi
