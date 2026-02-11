#!/usr/bin/env bash
# install.sh - Install agent-unleashed
#
# This script:
# 1. Installs Claude Code via npm (if not present)
# 2. Builds the CLI binaries (if cargo available)
# 3. Creates symlinks in ~/.local/bin/
# 4. Installs plugins to ~/.local/share/agent-unleashed/plugins
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

# GCS bucket for native Claude Code binaries
GCS_BUCKET="https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases"

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

# --- Version filtering (mirrors Rust logic in src/version.rs) ---

# Parse the version filter mode from user config or Cargo.toml default
get_filter_mode() {
    local user_config="$HOME/.config/agent-unleashed/config.toml"
    if [[ -f "$user_config" ]]; then
        local mode
        mode=$(grep -m1 'version_filter_mode' "$user_config" 2>/dev/null | sed 's/.*=//;s/[" '\'']*//g;s/[[:space:]]//g')
        if [[ -n "$mode" ]]; then
            echo "$mode"
            return
        fi
    fi
    # Parse default from Cargo.toml
    sed -n '/\[package\.metadata\.claude-code-versions\]/,/^\[/p' "$REPO_ROOT/Cargo.toml" | \
        grep -m1 'default_mode' | sed 's/.*=//;s/[" '\'']*//g;s/[[:space:]]//g'
}

# Get whitelist versions (user override or Cargo.toml default), one per line
get_whitelist() {
    local user_file="$HOME/.config/agent-unleashed/whitelist.txt"
    if [[ -f "$user_file" ]]; then
        grep -v '^\s*#' "$user_file" | grep -v '^\s*$' | sed 's/[[:space:]]//g'
        return
    fi
    sed -n '/\[package\.metadata\.claude-code-whitelist\]/,/^\[/p' "$REPO_ROOT/Cargo.toml" | \
        grep '^versions\s*=' | sed 's/.*\[//;s/\].*//;s/"//g;s/,/\n/g' | sed 's/[[:space:]]//g' | grep -v '^$'
}

# Get blacklist versions (user override or Cargo.toml default), one per line
get_blacklist() {
    local user_file="$HOME/.config/agent-unleashed/blacklist.txt"
    if [[ -f "$user_file" ]]; then
        grep -v '^\s*#' "$user_file" | grep -v '^\s*$' | sed 's/[[:space:]]//g'
        return
    fi
    sed -n '/\[package\.metadata\.claude-code-blacklist\]/,/^\[/p' "$REPO_ROOT/Cargo.toml" | \
        grep '^versions\s*=' | sed 's/.*\[//;s/\].*//;s/"//g;s/,/\n/g' | sed 's/[[:space:]]//g' | grep -v '^$'
}

# Resolve "latest" to the newest allowed version
# Tries GCS first for version discovery, falls back to npm
# Returns the version string, or empty if none found
resolve_latest_allowed() {
    local mode
    mode=$(get_filter_mode)

    # Try GCS-based version discovery first
    local gcs_latest
    gcs_latest=$(curl -fsSL "$GCS_BUCKET/latest" 2>/dev/null || echo "")

    if [[ -n "$gcs_latest" ]]; then
        # Check if GCS latest is allowed by our filter
        if [[ "$mode" == "blacklist" ]]; then
            local blacklist
            blacklist=$(get_blacklist)
            if ! echo "$blacklist" | grep -qx "$gcs_latest"; then
                echo "$gcs_latest"
                return
            fi
        else
            local whitelist
            whitelist=$(get_whitelist)
            if echo "$whitelist" | grep -qx "$gcs_latest"; then
                echo "$gcs_latest"
                return
            fi
        fi
        # GCS latest not allowed, fall through to npm for full version list
    fi

    # Fallback: get all npm versions (newest first)
    local npm_versions=""
    if command -v npm &> /dev/null; then
        npm_versions=$(npm view @anthropic-ai/claude-code versions --json 2>/dev/null | \
            grep -o '"[^"]*"' | tr -d '"' | tac)
    fi

    if [[ -z "$npm_versions" ]]; then
        return 1
    fi

    if [[ "$mode" == "blacklist" ]]; then
        local blacklist
        blacklist=$(get_blacklist)
        while IFS= read -r ver; do
            [[ -z "$ver" ]] && continue
            if ! echo "$blacklist" | grep -qx "$ver"; then
                echo "$ver"
                return
            fi
        done <<< "$npm_versions"
    else
        # whitelist mode (default)
        local whitelist
        whitelist=$(get_whitelist)
        while IFS= read -r ver; do
            [[ -z "$ver" ]] && continue
            if echo "$whitelist" | grep -qx "$ver"; then
                echo "$ver"
                return
            fi
        done <<< "$npm_versions"
    fi

    return 1
}

# Install Claude Code natively from GCS binary distribution
install_native() {
    local version="$1"
    local os arch platform

    # Detect platform
    case "$(uname -s)" in
        Darwin) os="darwin" ;;
        Linux) os="linux" ;;
        *) error "Unsupported OS: $(uname -s)"; return 1 ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64) arch="x64" ;;
        arm64|aarch64) arch="arm64" ;;
        *) error "Unsupported architecture: $(uname -m)"; return 1 ;;
    esac

    # Check for musl on Linux
    if [[ "$os" == "linux" ]]; then
        if [[ -f /lib/libc.musl-x86_64.so.1 ]] || [[ -f /lib/libc.musl-aarch64.so.1 ]]; then
            platform="${os}-${arch}-musl"
        else
            platform="${os}-${arch}"
        fi
    else
        platform="${os}-${arch}"
    fi

    local url="${GCS_BUCKET}/${version}/${platform}/claude"
    local manifest_url="${GCS_BUCKET}/${version}/manifest.json"
    local version_dir="$HOME/.local/share/claude/versions"
    local binary_path="${version_dir}/${version}"

    mkdir -p "$version_dir"

    info "Downloading Claude Code v${version} (native binary for ${platform})..."
    if ! curl -fsSL -o "${binary_path}.tmp" "$url"; then
        error "Failed to download native binary from GCS"
        rm -f "${binary_path}.tmp"
        return 1
    fi

    # Verify checksum from manifest
    local manifest
    manifest=$(curl -fsSL "$manifest_url" 2>/dev/null || echo "")
    if [[ -n "$manifest" ]]; then
        local expected_checksum
        expected_checksum=$(echo "$manifest" | python3 -c "import sys,json; m=json.load(sys.stdin); print(m.get('platforms',{}).get('$platform',{}).get('checksum',''))" 2>/dev/null || echo "")
        if [[ -n "$expected_checksum" ]]; then
            local actual_checksum
            actual_checksum=$(sha256sum "${binary_path}.tmp" 2>/dev/null | cut -d' ' -f1 || shasum -a 256 "${binary_path}.tmp" 2>/dev/null | cut -d' ' -f1)
            if [[ "$actual_checksum" != "$expected_checksum" ]]; then
                error "Checksum verification failed"
                error "  Expected: $expected_checksum"
                error "  Got:      $actual_checksum"
                rm -f "${binary_path}.tmp"
                return 1
            fi
            success "Checksum verified"
        fi
    fi

    chmod +x "${binary_path}.tmp"
    mv "${binary_path}.tmp" "$binary_path"

    # Create symlink
    ln -sf "$binary_path" "$BIN_DIR/claude"
    success "Claude Code v${version} installed natively"

    # Disable auto-updates since we manage versions
    export DISABLE_AUTOUPDATER=1
}

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
echo "│     Agent Unleashed Installer       │"
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
    # Detect current version
    CURRENT_VERSION=""
    if command -v claude &> /dev/null; then
        CURRENT_VERSION=$(claude --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "unknown")
        info "Claude Code currently installed: v${CURRENT_VERSION}"
    fi

    # Determine target version
    TARGET_VERSION="$CLAUDE_CODE_VERSION"
    if [[ "$TARGET_VERSION" == "latest" ]]; then
        FILTER_MODE=$(get_filter_mode)
        info "Version filter mode: ${FILTER_MODE}"
        RESOLVED=$(resolve_latest_allowed)
        if [[ -n "$RESOLVED" ]]; then
            TARGET_VERSION="$RESOLVED"
            info "Latest allowed version: v${TARGET_VERSION}"
        else
            warn "No allowed version found"
            warn "Check your whitelist in Cargo.toml or ~/.config/agent-unleashed/whitelist.txt"
            TARGET_VERSION=""
        fi
    fi

    # Install
    if [[ -z "$TARGET_VERSION" ]]; then
        warn "Skipping Claude Code install (no target version)"
    elif [[ -n "$CURRENT_VERSION" ]] && [[ "$CURRENT_VERSION" == "$TARGET_VERSION" ]]; then
        success "Claude Code is already up to date (v${CURRENT_VERSION})"
    else
        if [[ -n "$CURRENT_VERSION" ]]; then
            info "Updating Claude Code: v${CURRENT_VERSION} -> v${TARGET_VERSION}..."
        else
            info "Installing Claude Code v${TARGET_VERSION}..."
        fi

        if command -v npm &> /dev/null; then
            # Prefer npm install (produces patchable cli.js for auto-mode)
            npm install -g --force "@anthropic-ai/claude-code@${TARGET_VERSION}"

            # Point symlink to npm-installed cli.js
            NPM_ROOT=$(npm root -g 2>/dev/null || echo "")
            NPM_CLAUDE="$NPM_ROOT/@anthropic-ai/claude-code/cli.js"
            if [[ -f "$NPM_CLAUDE" ]]; then
                ln -sf "$NPM_CLAUDE" "$BIN_DIR/claude"
                success "Symlink: $BIN_DIR/claude -> $NPM_CLAUDE"
            fi
        else
            # Fallback: native install from GCS (no Node.js dependency)
            warn "npm not found, using native binary installer"
            install_native "$TARGET_VERSION"
        fi

        NEW_VERSION=$(claude --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "unknown")
        success "Claude Code installed: v${NEW_VERSION}"
    fi
fi

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

            # Install new binaries (au, aui, aug, autx, autxg)
            for bin in au aui aug autx autxg; do
                if [[ -f "$REPO_ROOT/target/release/$bin" ]]; then
                    cp "$REPO_ROOT/target/release/$bin" "$BIN_DIR/$bin"
                    chmod +x "$BIN_DIR/$bin"
                    success "Installed: $bin"
                fi
            done

            # Install patches to BIN_DIR
            info "Installing patches..."
            mkdir -p "$BIN_DIR/patches/versions"
            cp -r "$REPO_ROOT/scripts/patches/versions/"*.conf "$BIN_DIR/patches/versions/"
            success "Patches installed to $BIN_DIR/patches"
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

# agent-unleashed is an alias for au
ln -sf "$BIN_DIR/au" "$BIN_DIR/agent-unleashed"
success "Symlink: agent-unleashed -> au"

# Helper commands (bash scripts)
ln -sf "$SCRIPT_DIR/restart-claude" "$BIN_DIR/restart-claude"
ln -sf "$SCRIPT_DIR/exit-claude" "$BIN_DIR/exit-claude"
success "Symlink: restart-claude, exit-claude"

# Step 3: Install plugins globally
info "Installing plugins..."
PLUGINS_DIR="${HOME}/.local/share/agent-unleashed/plugins"
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
            success "Claude Code patched (auto-mode)"
        else
            warn "Auto-mode patch failed (Claude Code may not be installed)"
        fi
        # Supercompact EITF patch (replaces LLM compaction with instant EITF)
        SUPERCOMPACT_PATCH="$PLUGINS_DIR/supercompact/scripts/patch-compaction.sh"
        if [[ -f "$SUPERCOMPACT_PATCH" ]]; then
            if "$SUPERCOMPACT_PATCH"; then
                success "Claude Code patched (EITF compaction)"
            else
                warn "EITF compaction patch failed"
            fi
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
echo "  au             - Show help"
echo "  au go / aug    - Start agent with unleashed features"
echo "  au ui / aui    - TUI for profile/version management"
echo "  au tmux / autx - Headless tmux mode"
echo "  autx go / autxg - Start tmux session and attach"
echo "  au auth        - Check authentication status"
echo "  au patch       - Patch Claude Code for auto mode"
echo "  au version     - Manage Claude Code versions"
echo ""
echo "Helper Commands:"
echo "  restart-claude  - Restart agent (preserves session)"
echo "  exit-claude     - Exit agent and wrapper"
echo ""
echo "Quick start:"
echo "  aug              - Start agent with unleashed features"
echo "  aug --auto       - Start in auto mode"
echo "  autxg            - Start agent in tmux and attach"
echo ""

if ! $BUILD_TUI; then
    echo "Note: CLI not built. Install Rust and run:"
    echo "  cd $REPO_ROOT && cargo build --release"
    echo ""
fi

success "Done!"
