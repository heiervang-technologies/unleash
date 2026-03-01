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
            echo "  -i, --interactive   Show splash screen before installation"
            echo "  -h, --help          Show this help"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Interactive splash screen
if $INTERACTIVE; then
    clear
    # Display the muscular Claude ANSI art
    if [[ -f "$REPO_ROOT/ct4-right.ans" ]]; then
        cat "$REPO_ROOT/ct4-right.ans"
    elif [[ -f "$REPO_ROOT/src/assets/ct4-right.ans" ]]; then
        cat "$REPO_ROOT/src/assets/ct4-right.ans"
    else
        # Fallback ASCII art if ANSI art file not found
        echo ""
        echo "   ╭─────────────────────────────────────╮"
        echo "   │                                     │"
        echo "   │      ⚡ AGENT UNLEASHED ⚡          │"
        echo "   │                                     │"
        echo "   │      Breaking free from limits      │"
        echo "   │                                     │"
        echo "   ╰─────────────────────────────────────╯"
        echo ""
    fi
    echo ""
    echo -e "${GREEN}Press Enter to unleash the agent...${NC}"
    read -r
    clear
fi

echo ""
echo "╭─────────────────────────────────────╮"
echo "│     Unleash Installer       │"
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

            # Install new binaries (unleash, unleashed, u)
            for bin in unleash unleashed u; do
                if [[ -f "$REPO_ROOT/target/release/$bin" ]]; then
                    cp "$REPO_ROOT/target/release/$bin" "$BIN_DIR/$bin"
                    chmod +x "$BIN_DIR/$bin"
                    success "Installed: $bin"
                fi
            done

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

# Helper commands (bash scripts)
ln -sf "$SCRIPT_DIR/restart-claude" "$BIN_DIR/restart-claude"
ln -sf "$SCRIPT_DIR/exit-claude" "$BIN_DIR/exit-claude"
success "Symlink: restart-claude, exit-claude"

# Step 3: Install plugins globally
info "Installing plugins..."
PLUGINS_DIR="${HOME}/.local/share/unleash/plugins"
mkdir -p "$PLUGINS_DIR"

if [[ -d "$REPO_ROOT/plugins/unleashed" ]]; then
    cp -r "$REPO_ROOT/plugins/unleashed/"* "$PLUGINS_DIR/"
    success "Plugins installed to $PLUGINS_DIR"
    echo "  • auto-mode"
    echo "  • mcp-refresh"
    echo "  • process-restart"
    echo "  • voice-output"
else
    warn "Plugin directory not found: $REPO_ROOT/plugins/unleashed"
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
echo "  unleashed            - Direct wrapper without TUI (shorthand: u)"
echo ""
echo "Helper Commands:"
echo "  restart-claude   - Restart agent (preserves session)"
echo "  exit-claude      - Exit agent and wrapper"
echo ""
echo "Quick start:"
echo "  unleashed              - Start agent with unleashed features"
echo "  unleashed --auto       - Start in auto mode"
echo ""

if ! $BUILD_TUI; then
    echo "Note: CLI not built. Install Rust and run:"
    echo "  cd $REPO_ROOT && cargo build --release"
    echo ""
fi

success "Done!"
