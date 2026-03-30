#!/usr/bin/env bash
# uninstall.sh - Uninstall unleash
#
# This script removes:
# 1. CLI binary and symlinks from ~/.local/bin/
# 2. Plugins from ~/.local/share/unleash/
# 3. Optionally: config from ~/.config/unleash/
#
# Usage: ./scripts/uninstall.sh [--yes] [--keep-config]
#
# Options:
#   --yes          Skip confirmation prompts
#   --keep-config  Keep configuration files
#   --bin-dir DIR  Uninstall from DIR instead of ~/.local/bin

set -euo pipefail

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
SKIP_CONFIRM=false
KEEP_CONFIG=""
BIN_DIR="${HOME}/.local/bin"

while [[ $# -gt 0 ]]; do
    case $1 in
        --yes|-y)
            SKIP_CONFIRM=true
            shift
            ;;
        --keep-config)
            KEEP_CONFIG=true
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
            echo "  --yes, -y       Skip confirmation prompts"
            echo "  --keep-config   Keep configuration files"
            echo "  --bin-dir DIR   Uninstall from DIR instead of ~/.local/bin"
            echo "  -h, --help      Show this help"
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
echo "│         unleash Uninstaller         │"
echo "╰─────────────────────────────────────╯"
echo ""

# Check what's installed
BINARIES=("unleash" "unleash-refresh" "unleash-exit" "restart-claude" "exit-claude")
INSTALLED_BINS=()

for bin in "${BINARIES[@]}"; do
    if [[ -e "$BIN_DIR/$bin" ]]; then
        INSTALLED_BINS+=("$bin")
    fi
done

DATA_DIR="${HOME}/.local/share/unleash"
CONFIG_DIR="${HOME}/.config/unleash"
NATIVE_VERSIONS_DIR="${HOME}/.local/share/claude/versions"

HAS_DATA=false
HAS_CONFIG=false
HAS_NATIVE_VERSIONS=false

[[ -d "$DATA_DIR" ]] && HAS_DATA=true
[[ -d "$CONFIG_DIR" ]] && HAS_CONFIG=true
[[ -d "$NATIVE_VERSIONS_DIR" ]] && HAS_NATIVE_VERSIONS=true

# Show what will be removed
if [[ ${#INSTALLED_BINS[@]} -eq 0 ]] && ! $HAS_DATA && ! $HAS_CONFIG && ! $HAS_NATIVE_VERSIONS; then
    warn "Nothing to uninstall"
    exit 0
fi

info "The following will be removed:"
echo ""

if [[ ${#INSTALLED_BINS[@]} -gt 0 ]]; then
    echo "  Binaries/symlinks in $BIN_DIR:"
    for bin in "${INSTALLED_BINS[@]}"; do
        echo "    • $bin"
    done
    echo ""
fi

if $HAS_DATA; then
    echo "  Data directory:"
    echo "    • $DATA_DIR"
    echo ""
fi

if $HAS_CONFIG; then
    echo "  Configuration directory:"
    echo "    • $CONFIG_DIR"
    echo ""
fi

if $HAS_NATIVE_VERSIONS; then
    echo "  Native Claude Code binaries:"
    echo "    • $NATIVE_VERSIONS_DIR"
    echo ""
fi

# Confirm uninstall
if ! $SKIP_CONFIRM; then
    echo -n "Proceed with uninstall? [y/N] "
    read -r response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        info "Uninstall cancelled"
        exit 0
    fi
    echo ""
fi

# Remove binaries and symlinks
if [[ ${#INSTALLED_BINS[@]} -gt 0 ]]; then
    info "Removing binaries and symlinks..."
    for bin in "${INSTALLED_BINS[@]}"; do
        rm -f "$BIN_DIR/$bin"
        success "Removed: $bin"
    done
fi

# Remove data directory (plugins)
if $HAS_DATA; then
    info "Removing data directory..."
    rm -rf "$DATA_DIR"
    success "Removed: $DATA_DIR"
fi

# Handle config directory
if $HAS_CONFIG; then
    if [[ "$KEEP_CONFIG" == "true" ]]; then
        info "Keeping configuration (--keep-config specified)"
    elif [[ "$KEEP_CONFIG" == "false" ]] || $SKIP_CONFIRM; then
        info "Removing configuration..."
        rm -rf "$CONFIG_DIR"
        success "Removed: $CONFIG_DIR"
    else
        # Ask user interactively
        echo ""
        echo -n "Keep configuration files in $CONFIG_DIR? [Y/n] "
        read -r response
        if [[ "$response" =~ ^[Nn]$ ]]; then
            info "Removing configuration..."
            rm -rf "$CONFIG_DIR"
            success "Removed: $CONFIG_DIR"
        else
            info "Keeping configuration files"
        fi
    fi
fi

# Remove native Claude Code binaries
if $HAS_NATIVE_VERSIONS; then
    info "Removing native Claude Code binaries..."
    rm -rf "$NATIVE_VERSIONS_DIR"
    success "Removed: $NATIVE_VERSIONS_DIR"
fi

# Also check for cargo-installed binary
CARGO_BIN="${HOME}/.cargo/bin/unleash"
if [[ -e "$CARGO_BIN" ]]; then
    echo ""
    warn "Found cargo-installed binary at $CARGO_BIN"
    if ! $SKIP_CONFIRM; then
        echo -n "Remove it too? [y/N] "
        read -r response
        if [[ "$response" =~ ^[Yy]$ ]]; then
            rm -f "$CARGO_BIN"
            success "Removed: $CARGO_BIN"
        fi
    fi
fi

echo ""
echo "╭─────────────────────────────────────╮"
echo "│       Uninstall Complete            │"
echo "╰─────────────────────────────────────╯"
echo ""
info "unleash has been uninstalled"
echo ""
echo "Note: Claude Code (npm package) was not removed."
echo "To remove it: npm uninstall -g @anthropic-ai/claude-code"
echo ""
echo "Note: The claude symlink in $BIN_DIR may point to an npm or native binary."
echo "Check with: ls -la $BIN_DIR/claude"
echo ""

success "Done!"
