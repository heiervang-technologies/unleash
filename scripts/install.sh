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
            echo "  --no-build    Skip building the TUI (use wrapper.sh only)"
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

            # Install the binary
            if [[ -f "$REPO_ROOT/target/release/claude-unleashed" ]]; then
                cp "$REPO_ROOT/target/release/claude-unleashed" "$BIN_DIR/claude-unleashed-tui"
                chmod +x "$BIN_DIR/claude-unleashed-tui"
                success "Installed: $BIN_DIR/claude-unleashed-tui"
            fi
        else
            warn "TUI build failed, falling back to wrapper-only mode"
            BUILD_TUI=false
        fi
    else
        warn "Cargo not found, skipping TUI build"
        warn "Install Rust to build the TUI: https://rustup.rs"
        BUILD_TUI=false
    fi
fi

# Step 2: Create symlinks for scripts
info "Creating symlinks..."

# Main entry point
if $BUILD_TUI && [[ -f "$BIN_DIR/claude-unleashed-tui" ]]; then
    # TUI is the main entry point
    ln -sf "$BIN_DIR/claude-unleashed-tui" "$BIN_DIR/claude-unleashed"
    success "Symlink: claude-unleashed -> TUI binary"
else
    # Wrapper is the main entry point
    ln -sf "$SCRIPT_DIR/wrapper.sh" "$BIN_DIR/claude-unleashed"
    success "Symlink: claude-unleashed -> wrapper.sh"
fi

# Wrapper script (always available for TUI to use)
ln -sf "$SCRIPT_DIR/wrapper.sh" "$BIN_DIR/cuw"
success "Symlink: cuw -> wrapper.sh (with plugins)"

# Headless tmux mode
ln -sf "$SCRIPT_DIR/cutx" "$BIN_DIR/cutx"
success "Symlink: cutx -> headless tmux mode"

# Helper commands
ln -sf "$SCRIPT_DIR/restart-claude" "$BIN_DIR/restart-claude"
ln -sf "$SCRIPT_DIR/exit-claude" "$BIN_DIR/exit-claude"
success "Symlink: restart-claude"
success "Symlink: exit-claude"

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
echo "Installed commands:"
echo "  claude-unleashed  - Start Claude with unleashed features"
echo "  cuw               - Short alias (wrapper with plugins)"
echo "  cutx              - Headless tmux mode"
echo "  restart-claude    - Restart Claude (preserves session)"
echo "  exit-claude       - Exit Claude and wrapper"
echo ""
echo "Headless mode usage:"
echo "  cutx start        - Start Claude in tmux session"
echo "  cutx send \"msg\"   - Send message to Claude"
echo "  cutx attach       - Attach to session interactively"
echo ""

if ! $BUILD_TUI; then
    echo "Note: TUI not built. Run with cargo to enable:"
    echo "  cd $REPO_ROOT && cargo build --release"
    echo ""
fi

success "Done!"
