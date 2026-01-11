#!/usr/bin/env bash
# install.sh - Install claude-unleashed
#
# This script:
# 1. Builds the TUI binary (if cargo available)
# 2. Creates symlinks in ~/.local/bin/
# 3. Runs initial Claude Code patch
#
# Usage: ./scripts/install.sh [--no-build] [--no-patch]

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

# Parse arguments
BUILD_TUI=true
RUN_PATCH=true
BIN_DIR="${HOME}/.local/bin"

while [[ $# -gt 0 ]]; do
    case $1 in
        --no-build)
            BUILD_TUI=false
            shift
            ;;
        --no-patch)
            RUN_PATCH=false
            shift
            ;;
        --bin-dir)
            BIN_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --no-build    Skip building the TUI"
            echo "  --no-patch    Skip patching Claude Code"
            echo "  --bin-dir DIR Install to DIR instead of ~/.local/bin"
            echo "  -h, --help    Show this help"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo ""
echo "╭─────────────────────────────────────╮"
echo "│     Claude Unleashed Installer      │"
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

# Step 1: Build TUI (optional)
if $BUILD_TUI; then
    if command -v cargo &> /dev/null; then
        info "Building TUI binary..."
        cd "$REPO_ROOT"
        if cargo build --release; then
            success "TUI built successfully"

            # Install the binary as cui
            if [[ -f "$REPO_ROOT/target/release/cui" ]]; then
                cp "$REPO_ROOT/target/release/cui" "$BIN_DIR/cui"
                chmod +x "$BIN_DIR/cui"
                success "Installed: cui (TUI interface)"
            fi
        else
            warn "TUI build failed, continuing without TUI"
            BUILD_TUI=false
        fi
    else
        warn "Cargo not found, skipping TUI build"
        warn "Install Rust to build the TUI: https://rustup.rs"
        BUILD_TUI=false
    fi
fi

# Step 2: Create symlinks for CLI tools
info "Creating symlinks..."

# Main entry point: cu (Claude Unleashed)
ln -sf "$SCRIPT_DIR/cu" "$BIN_DIR/cu"
success "Symlink: cu (main CLI)"

# Backwards compatibility: cuw -> cu
ln -sf "$BIN_DIR/cu" "$BIN_DIR/cuw"
success "Symlink: cuw -> cu (backwards compat)"

# Legacy alias: claude-unleashed -> cu
ln -sf "$BIN_DIR/cu" "$BIN_DIR/claude-unleashed"
success "Symlink: claude-unleashed -> cu"

# Headless tmux mode
ln -sf "$SCRIPT_DIR/cutx" "$BIN_DIR/cutx"
success "Symlink: cutx (headless tmux mode)"

# Helper commands
ln -sf "$SCRIPT_DIR/restart-claude" "$BIN_DIR/restart-claude"
ln -sf "$SCRIPT_DIR/exit-claude" "$BIN_DIR/exit-claude"
success "Symlink: restart-claude, exit-claude"

# Step 3: Patch Claude Code (optional)
if $RUN_PATCH; then
    if command -v claude &> /dev/null; then
        info "Patching Claude Code..."
        if "$SCRIPT_DIR/patch-claude.sh"; then
            success "Claude Code patched"
        else
            warn "Patch failed (Claude Code may not be installed)"
        fi
    else
        warn "Claude Code not found, skipping patch"
        warn "Install Claude Code first: npm install -g @anthropic-ai/claude-code"
    fi
fi

# Step 4: Print summary
echo ""
echo "╭─────────────────────────────────────╮"
echo "│        Installation Complete        │"
echo "╰─────────────────────────────────────╯"
echo ""
echo "CLI Commands:"
echo "  cu       - Main entry point (Claude Unleashed)"
echo "  cuw      - Alias for cu (backwards compat)"
echo "  cutx     - Headless tmux mode"
if $BUILD_TUI; then
echo "  cui      - TUI interface"
fi
echo ""
echo "Helper Commands:"
echo "  restart-claude  - Restart Claude (preserves session)"
echo "  exit-claude     - Exit Claude and wrapper"
echo ""
echo "Quick start:"
echo "  cu               - Start Claude with unleashed features"
echo "  cu -p \"prompt\"   - Headless mode with prompt"
echo ""

if ! $BUILD_TUI; then
    echo "Note: TUI not built. Install Rust and run:"
    echo "  cd $REPO_ROOT && cargo build --release"
    echo ""
fi

success "Done!"
