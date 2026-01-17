#!/usr/bin/env bash
# install.sh - Install claude-unleashed
#
# This script:
# 1. Installs Claude Code via npm (if not present)
# 2. Builds the TUI binary (if cargo available)
# 3. Creates symlinks in ~/.local/bin/
# 4. Installs plugins to ~/.local/share/claude-unleashed/plugins
# 5. Runs initial Claude Code patch
#
# Usage: ./scripts/install.sh [--no-build] [--no-patch] [--no-claude-code]
#
# Options:
#   --no-build         Skip building the TUI
#   --no-patch         Skip patching Claude Code
#   --no-claude-code   Skip installing Claude Code
#   --claude-version   Install specific Claude Code version
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

# Parse arguments
BUILD_TUI=true
RUN_PATCH=true
INSTALL_CLAUDE_CODE=true
CLAUDE_CODE_VERSION="latest"
BIN_DIR="${HOME}/.local/bin"
INTERACTIVE=false

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
        --no-claude-code)
            INSTALL_CLAUDE_CODE=false
            shift
            ;;
        --claude-version)
            CLAUDE_CODE_VERSION="$2"
            shift 2
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
            echo "  --no-patch          Skip patching Claude Code"
            echo "  --no-claude-code    Skip installing Claude Code"
            echo "  --claude-version V  Install specific Claude Code version"
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
        echo "   │      ⚡ CLAUDE UNLEASHED ⚡         │"
        echo "   │                                     │"
        echo "   │      Breaking free from limits      │"
        echo "   │                                     │"
        echo "   ╰─────────────────────────────────────╯"
        echo ""
    fi
    echo ""
    echo -e "${GREEN}Press Enter to unleash Claude...${NC}"
    read -r
    clear
fi

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

# Step 0: Install or update Claude Code (if requested)
if $INSTALL_CLAUDE_CODE; then
    if command -v npm &> /dev/null; then
        CURRENT_VERSION=""
        if command -v claude &> /dev/null; then
            CURRENT_VERSION=$(claude --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "unknown")
            info "Claude Code currently installed: v${CURRENT_VERSION}"
        fi

        # Determine target version
        TARGET_VERSION="$CLAUDE_CODE_VERSION"
        if [[ "$TARGET_VERSION" == "latest" ]]; then
            NPM_LATEST=$(npm view @anthropic-ai/claude-code version 2>/dev/null || echo "")
            if [[ -n "$NPM_LATEST" ]]; then
                TARGET_VERSION="$NPM_LATEST"
                info "Latest available version: v${TARGET_VERSION}"
            fi
        fi

        # Check if update needed
        if [[ -n "$CURRENT_VERSION" ]] && [[ "$CURRENT_VERSION" == "$TARGET_VERSION" ]]; then
            success "Claude Code is already up to date (v${CURRENT_VERSION})"
        else
            if [[ -n "$CURRENT_VERSION" ]]; then
                info "Updating Claude Code: v${CURRENT_VERSION} -> v${TARGET_VERSION}..."
            else
                info "Installing Claude Code v${TARGET_VERSION}..."
            fi

            if [[ "$CLAUDE_CODE_VERSION" == "latest" ]]; then
                npm install -g @anthropic-ai/claude-code
            else
                npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}"
            fi

            NEW_VERSION=$(claude --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "unknown")
            success "Claude Code installed: v${NEW_VERSION}"
        fi
    else
        warn "npm not found, skipping Claude Code installation"
        warn "Install Node.js from https://nodejs.org/ to install Claude Code"
    fi
fi

# Step 1: Build CLI binaries
if $BUILD_TUI; then
    if command -v cargo &> /dev/null; then
        info "Building CLI binaries..."
        cd "$REPO_ROOT"
        if cargo build --release; then
            success "CLI built successfully"

            # Install all binaries (cu, cui, cug, cutx, cutxg)
            for bin in cu cui cug cutx cutxg; do
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

# claude-unleashed is an alias for cu
ln -sf "$BIN_DIR/cu" "$BIN_DIR/claude-unleashed"
success "Symlink: claude-unleashed -> cu"

# Helper commands (bash scripts)
ln -sf "$SCRIPT_DIR/restart-claude" "$BIN_DIR/restart-claude"
ln -sf "$SCRIPT_DIR/exit-claude" "$BIN_DIR/exit-claude"
success "Symlink: restart-claude, exit-claude"

# Step 3: Install plugins globally
info "Installing plugins..."
PLUGINS_DIR="${HOME}/.local/share/claude-unleashed/plugins"
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

# Step 4: Patch Claude Code (optional)
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

# Step 5: Print summary
echo ""
echo "╭─────────────────────────────────────╮"
echo "│        Installation Complete        │"
echo "╰─────────────────────────────────────╯"
echo ""
echo "CLI Commands:"
echo "  cu             - Show help"
echo "  cu go / cug    - Start Claude with unleashed features"
echo "  cu ui / cui    - TUI for profile/version management"
echo "  cu tmux / cutx - Headless tmux mode"
echo "  cutx go / cutxg - Start tmux session and attach"
echo "  cu auth        - Check authentication status"
echo "  cu patch       - Patch Claude Code for auto mode"
echo "  cu version     - Manage Claude Code versions"
echo ""
echo "Helper Commands:"
echo "  restart-claude  - Restart Claude (preserves session)"
echo "  exit-claude     - Exit Claude and wrapper"
echo ""
echo "Quick start:"
echo "  cug              - Start Claude with unleashed features"
echo "  cug --auto       - Start in auto mode"
echo "  cutxg            - Start Claude in tmux and attach"
echo ""

if ! $BUILD_TUI; then
    echo "Note: CLI not built. Install Rust and run:"
    echo "  cd $REPO_ROOT && cargo build --release"
    echo ""
fi

success "Done!"
