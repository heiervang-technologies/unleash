#!/usr/bin/env bash
# install-remote.sh - Remote installer for Claude Unleashed
#
# Usage (public repo):
#   curl -fsSL https://raw.githubusercontent.com/heiervang-technologies/claude-unleashed/main/scripts/install-remote.sh | bash
#
# Usage (private repo):
#   GH_TOKEN=ghp_xxx curl -fsSL -H "Authorization: token $GH_TOKEN" \
#     https://raw.githubusercontent.com/heiervang-technologies/claude-unleashed/main/scripts/install-remote.sh | bash
#
# Options (via environment variables):
#   GH_TOKEN / GH_PAT / GITHUB_TOKEN - GitHub token for private repo access (any of these work)
#   CLAUDE_UNLEASHED_VERSION - Specific version to install (default: latest)
#   CLAUDE_CODE_VERSION      - Specific Claude Code version (default: latest)
#   INSTALL_DIR              - Installation directory (default: ~/.local/bin)
#   BUILD_FROM_SOURCE        - Set to "1" to build from source instead of downloading binary
#
# This script:
# 1. Checks prerequisites (npm, optionally cargo)
# 2. Installs Claude Code via npm if not present
# 3. Downloads pre-built binary or builds from source
# 4. Sets up cu/cui/cuw/cutx commands
# 5. Runs initial patch

set -euo pipefail

# Support common GitHub token variable names
GITHUB_TOKEN="${GH_TOKEN:-${GH_PAT:-${GITHUB_TOKEN:-}}}"

# Configuration
REPO_OWNER="heiervang-technologies"
REPO_NAME="claude-unleashed"
REPO_URL="https://github.com/${REPO_OWNER}/${REPO_NAME}"
RAW_URL="https://raw.githubusercontent.com/${REPO_OWNER}/${REPO_NAME}/main"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BUILD_FROM_SOURCE="${BUILD_FROM_SOURCE:-0}"
CLAUDE_UNLEASHED_VERSION="${CLAUDE_UNLEASHED_VERSION:-latest}"
CLAUDE_CODE_VERSION="${CLAUDE_CODE_VERSION:-latest}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

info() { echo -e "${BLUE}==>${NC} $1"; }
success() { echo -e "${GREEN}==>${NC} $1"; }
warn() { echo -e "${YELLOW}==>${NC} $1"; }
error() { echo -e "${RED}==>${NC} $1" >&2; }

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)
            PLATFORM="linux"
            ;;
        Darwin)
            PLATFORM="macos"
            ;;
        *)
            error "Unsupported OS: $OS"
            exit 1
            ;;
    esac

    case "$ARCH" in
        x86_64|amd64)
            ARCH="x86_64"
            ;;
        arm64|aarch64)
            ARCH="aarch64"
            ;;
        *)
            error "Unsupported architecture: $ARCH"
            exit 1
            ;;
    esac

    # Construct target triple
    case "$PLATFORM" in
        linux)
            TARGET="${ARCH}-unknown-linux-gnu"
            BINARY_SUFFIX=""
            ;;
        macos)
            TARGET="${ARCH}-apple-darwin"
            BINARY_SUFFIX=""
            ;;
    esac

    info "Detected platform: $PLATFORM ($ARCH)"
}

# Check for required commands
check_prerequisites() {
    local missing=()

    # Check for npm (required for claude-code)
    if ! command -v npm &> /dev/null; then
        missing+=("npm")
    fi

    # Check for curl or wget
    if ! command -v curl &> /dev/null && ! command -v wget &> /dev/null; then
        missing+=("curl or wget")
    fi

    # If building from source, check for cargo
    if [[ "$BUILD_FROM_SOURCE" == "1" ]]; then
        if ! command -v cargo &> /dev/null; then
            missing+=("cargo (for building from source)")
        fi
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        error "Missing required dependencies:"
        for dep in "${missing[@]}"; do
            echo "  - $dep"
        done
        echo ""
        echo "Please install the missing dependencies:"
        echo "  - npm: https://nodejs.org/ or use your package manager"
        echo "  - cargo: https://rustup.rs/"
        exit 1
    fi

    success "All prerequisites found"
}

# Download file using curl or wget (supports private repos via GITHUB_TOKEN)
# Returns 0 on success, 1 on failure (silently)
download() {
    local url="$1"
    local output="$2"
    local auth_header=""

    # Use GITHUB_TOKEN for GitHub URLs if available (for private repos)
    if [[ -n "${GITHUB_TOKEN:-}" ]] && [[ "$url" == *"github.com"* || "$url" == *"githubusercontent.com"* ]]; then
        auth_header="Authorization: token $GITHUB_TOKEN"
    fi

    if command -v curl &> /dev/null; then
        if [[ -n "$auth_header" ]]; then
            curl -fsSL -H "$auth_header" "$url" -o "$output" 2>/dev/null
        else
            curl -fsSL "$url" -o "$output" 2>/dev/null
        fi
    elif command -v wget &> /dev/null; then
        if [[ -n "$auth_header" ]]; then
            wget -q --header="$auth_header" "$url" -O "$output" 2>/dev/null
        else
            wget -q "$url" -O "$output" 2>/dev/null
        fi
    fi
}

# Get latest release version from GitHub (supports private repos via GITHUB_TOKEN)
get_latest_version() {
    local api_url="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest"
    local auth_header=""

    if [[ -n "${GITHUB_TOKEN:-}" ]]; then
        auth_header="Authorization: token $GITHUB_TOKEN"
    fi

    if command -v curl &> /dev/null; then
        if [[ -n "$auth_header" ]]; then
            curl -fsSL -H "$auth_header" "$api_url" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
        else
            curl -fsSL "$api_url" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
        fi
    elif command -v wget &> /dev/null; then
        if [[ -n "$auth_header" ]]; then
            wget -qO- --header="$auth_header" "$api_url" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
        else
            wget -qO- "$api_url" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
        fi
    fi
}

# Install or update Claude Code via npm
install_claude_code() {
    local current_version=""
    local target_version="$CLAUDE_CODE_VERSION"

    if command -v claude &> /dev/null; then
        current_version=$(claude --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "unknown")
        info "Claude Code currently installed: v${current_version}"
    fi

    # Get latest version from npm if targeting latest
    if [[ "$target_version" == "latest" ]]; then
        local npm_latest
        npm_latest=$(npm view @anthropic-ai/claude-code version 2>/dev/null || echo "")
        if [[ -n "$npm_latest" ]]; then
            target_version="$npm_latest"
            info "Latest available version: v${target_version}"
        fi
    fi

    # Check if update needed
    if [[ -n "$current_version" ]] && [[ "$current_version" == "$target_version" ]]; then
        success "Claude Code is already up to date (v${current_version})"
        return 0
    fi

    # Install or update
    if [[ -n "$current_version" ]]; then
        info "Updating Claude Code: v${current_version} -> v${target_version}..."
    else
        info "Installing Claude Code v${target_version}..."
    fi

    if [[ "$CLAUDE_CODE_VERSION" == "latest" ]]; then
        npm install -g @anthropic-ai/claude-code
    else
        npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}"
    fi

    local new_version
    new_version=$(claude --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "unknown")
    success "Claude Code installed: v${new_version}"
}

# Download pre-built binary from GitHub releases
download_binary() {
    local version="$1"
    local temp_dir
    temp_dir=$(mktemp -d)

    info "Checking for pre-built binary..."

    # Construct download URL
    local binary_name="cui-${TARGET}"
    local download_url="${REPO_URL}/releases/download/${version}/${binary_name}"

    # Suppress error output - 404 is expected if no binary exists
    if ! download "$download_url" "${temp_dir}/cui" 2>/dev/null; then
        rm -rf "$temp_dir"
        return 1
    fi

    # Verify we got a real binary, not an error page
    if [[ ! -s "${temp_dir}/cui" ]] || file "${temp_dir}/cui" 2>/dev/null | grep -q "text\|HTML"; then
        rm -rf "$temp_dir"
        return 1
    fi

    chmod +x "${temp_dir}/cui"
    mv "${temp_dir}/cui" "${INSTALL_DIR}/cui"
    rm -rf "$temp_dir"

    success "Binary downloaded and installed"
    return 0
}

# Build from source
build_from_source() {
    info "Building from source..."

    local temp_dir
    temp_dir=$(mktemp -d)

    # Clone repository
    info "Cloning repository..."
    git clone --depth 1 "${REPO_URL}.git" "$temp_dir"

    # Build
    info "Building with cargo..."
    cd "$temp_dir"
    cargo build --release

    # Install binary
    cp "target/release/cui" "${INSTALL_DIR}/cui"
    chmod +x "${INSTALL_DIR}/cui"

    # Cleanup
    rm -rf "$temp_dir"

    success "Built and installed from source"
}

# Download and install shell scripts
install_scripts() {
    info "Installing shell scripts..."

    local scripts=("cu" "cutx" "restart-claude" "exit-claude" "patch-claude.sh" "check-and-patch.sh")
    local lib_scripts=("lib/onboarding.sh")

    # Create lib directory
    mkdir -p "${INSTALL_DIR}/lib"

    # Download main scripts
    for script in "${scripts[@]}"; do
        download "${RAW_URL}/scripts/${script}" "${INSTALL_DIR}/${script}"
        chmod +x "${INSTALL_DIR}/${script}"
    done

    # Download lib scripts
    for script in "${lib_scripts[@]}"; do
        download "${RAW_URL}/scripts/${script}" "${INSTALL_DIR}/${script}"
        chmod +x "${INSTALL_DIR}/${script}"
    done

    # Download patches directory
    mkdir -p "${INSTALL_DIR}/patches/versions"

    # Get list of patch configs from repo (simple approach: download known versions)
    for version in "2.1.3" "2.1.4" "2.1.5"; do
        if download "${RAW_URL}/scripts/patches/versions/${version}.conf" "${INSTALL_DIR}/patches/versions/${version}.conf" 2>/dev/null; then
            :
        fi
    done

    # Create symlinks
    ln -sf "${INSTALL_DIR}/cu" "${INSTALL_DIR}/cuw"
    ln -sf "${INSTALL_DIR}/cu" "${INSTALL_DIR}/claude-unleashed"

    success "Scripts installed"
}

# Update PATH instructions
show_path_instructions() {
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        echo ""
        warn "${INSTALL_DIR} is not in your PATH"
        echo ""
        echo "Add this line to your shell config (~/.bashrc, ~/.zshrc, etc.):"
        echo ""
        echo -e "  ${CYAN}export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}"
        echo ""
        echo "Then restart your shell or run:"
        echo ""
        echo -e "  ${CYAN}source ~/.bashrc${NC}  # or ~/.zshrc"
        echo ""
    fi
}

# Run initial patch
run_patch() {
    if command -v claude &> /dev/null; then
        info "Patching Claude Code for auto mode..."

        # Set SCRIPT_DIR for patch script
        export SCRIPT_DIR="${INSTALL_DIR}"

        if bash "${INSTALL_DIR}/patch-claude.sh" 2>/dev/null; then
            success "Claude Code patched"
        else
            warn "Patch failed (may need to run manually after updating PATH)"
        fi
    fi
}

# Main installation
main() {
    echo ""
    echo "╭──────────────────────────────────────────╮"
    echo "│     Claude Unleashed Remote Installer    │"
    echo "╰──────────────────────────────────────────╯"
    echo ""

    detect_platform
    check_prerequisites

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Install Claude Code
    install_claude_code

    # Determine version to install
    if [[ "$CLAUDE_UNLEASHED_VERSION" == "latest" ]]; then
        CLAUDE_UNLEASHED_VERSION=$(get_latest_version)
        if [[ -z "$CLAUDE_UNLEASHED_VERSION" ]]; then
            warn "Could not determine latest version, using 'main' branch"
            CLAUDE_UNLEASHED_VERSION="main"
        fi
    fi
    info "Installing Claude Unleashed ${CLAUDE_UNLEASHED_VERSION}"

    # Install binary (try download first, fall back to source)
    if [[ "$BUILD_FROM_SOURCE" == "1" ]]; then
        build_from_source
    else
        if ! download_binary "$CLAUDE_UNLEASHED_VERSION"; then
            warn "Binary download failed, building from source..."
            if command -v cargo &> /dev/null; then
                build_from_source
            else
                warn "Cargo not found, skipping TUI binary"
                warn "Install Rust (https://rustup.rs/) to build the TUI"
            fi
        fi
    fi

    # Install scripts
    install_scripts

    # Run patch
    run_patch

    # Show completion message
    echo ""
    echo "╭──────────────────────────────────────────╮"
    echo "│         Installation Complete!           │"
    echo "╰──────────────────────────────────────────╯"
    echo ""
    echo "Installed commands:"
    echo "  cu       - Main entry point (Claude Unleashed)"
    echo "  cuw      - Alias for cu"
    echo "  cutx     - Headless tmux mode"
    if [[ -f "${INSTALL_DIR}/cui" ]]; then
    echo "  cui      - TUI interface"
    fi
    echo ""
    echo "Quick start:"
    echo "  cu                 - Start Claude with unleashed features"
    echo "  cu -p \"prompt\"     - Headless mode with prompt"
    echo "  cui                - Launch TUI for profile management"
    echo ""

    show_path_instructions

    success "Done! Run 'cu' to start Claude Unleashed."
}

main "$@"
