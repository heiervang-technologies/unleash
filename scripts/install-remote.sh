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
# 4. Sets up cu/cui/cug/cutx commands
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

    # Construct artifact name to match release workflow
    case "$PLATFORM" in
        linux)
            if [[ "$ARCH" == "x86_64" ]]; then
                ARTIFACT_NAME="cu-linux-x86_64"
            else
                ARTIFACT_NAME="cu-linux-${ARCH}"
            fi
            ;;
        macos)
            if [[ "$ARCH" == "x86_64" ]]; then
                ARTIFACT_NAME="cu-macos-x86_64"
            elif [[ "$ARCH" == "aarch64" ]]; then
                ARTIFACT_NAME="cu-macos-arm64"
            else
                ARTIFACT_NAME="cu-macos-${ARCH}"
            fi
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

# Cache for Cargo.toml content
CARGO_TOML_CACHE=""

# Fetch Cargo.toml from repo (cached)
fetch_cargo_toml() {
    if [[ -n "$CARGO_TOML_CACHE" ]]; then
        echo "$CARGO_TOML_CACHE"
        return
    fi

    if command -v curl &> /dev/null; then
        if [[ -n "${GITHUB_TOKEN:-}" ]]; then
            CARGO_TOML_CACHE=$(curl -fsSL -H "Authorization: token $GITHUB_TOKEN" "${RAW_URL}/Cargo.toml" 2>/dev/null)
        else
            CARGO_TOML_CACHE=$(curl -fsSL "${RAW_URL}/Cargo.toml" 2>/dev/null)
        fi
    elif command -v wget &> /dev/null; then
        if [[ -n "${GITHUB_TOKEN:-}" ]]; then
            CARGO_TOML_CACHE=$(wget -qO- --header="Authorization: token $GITHUB_TOKEN" "${RAW_URL}/Cargo.toml" 2>/dev/null)
        else
            CARGO_TOML_CACHE=$(wget -qO- "${RAW_URL}/Cargo.toml" 2>/dev/null)
        fi
    fi

    echo "$CARGO_TOML_CACHE"
}

# Fetch whitelisted versions from Cargo.toml in the repo
get_whitelist() {
    local cargo_toml
    cargo_toml=$(fetch_cargo_toml)

    if [[ -n "$cargo_toml" ]]; then
        # Extract versions array from [package.metadata.claude-code-whitelist] section
        echo "$cargo_toml" | grep -A1 '\[package.metadata.claude-code-whitelist\]' | \
            grep 'versions' | sed 's/.*\[\([^]]*\)\].*/\1/' | tr -d '"' | tr ',' '\n' | tr -d ' '
    fi
}

# Fetch blacklisted versions from Cargo.toml in the repo
get_blacklist() {
    local cargo_toml
    cargo_toml=$(fetch_cargo_toml)

    if [[ -n "$cargo_toml" ]]; then
        # Extract versions array from [package.metadata.claude-code-blacklist] section
        echo "$cargo_toml" | grep -A1 '\[package.metadata.claude-code-blacklist\]' | \
            grep 'versions' | sed 's/.*\[\([^]]*\)\].*/\1/' | tr -d '"' | tr ',' '\n' | tr -d ' '
    fi
}

# Get the default filter mode from Cargo.toml
get_default_mode() {
    local cargo_toml
    cargo_toml=$(fetch_cargo_toml)

    if [[ -n "$cargo_toml" ]]; then
        # Extract default_mode from [package.metadata.claude-code-versions] section
        local mode
        mode=$(echo "$cargo_toml" | grep -A1 '\[package.metadata.claude-code-versions\]' | \
            grep 'default_mode' | sed 's/.*= *"\([^"]*\)".*/\1/')
        if [[ -n "$mode" ]]; then
            echo "$mode"
            return
        fi
    fi

    # Default to whitelist mode
    echo "whitelist"
}

# Check if a version is whitelisted
is_version_whitelisted() {
    local version="$1"
    local whitelist
    whitelist=$(get_whitelist)

    echo "$whitelist" | grep -qx "$version"
}

# Check if a version is blacklisted
is_version_blacklisted() {
    local version="$1"
    local blacklist
    blacklist=$(get_blacklist)

    echo "$blacklist" | grep -qx "$version"
}

# Check if a version is allowed based on the filter mode
is_version_allowed() {
    local version="$1"
    local mode
    mode=$(get_default_mode)

    if [[ "$mode" == "blacklist" ]]; then
        # In blacklist mode, allow if NOT blacklisted
        ! is_version_blacklisted "$version"
    else
        # In whitelist mode (default), allow if whitelisted
        is_version_whitelisted "$version"
    fi
}

# Get latest allowed Claude Code version from npm based on filter mode
get_recommended_claude_code_version() {
    local versions
    local mode
    mode=$(get_default_mode)

    # Get available versions from npm (newest first)
    # Use tac on Linux, tail -r on macOS for reverse order
    local reverse_cmd="tac"
    if [[ "$OSTYPE" == "darwin"* ]] && ! command -v tac &> /dev/null; then
        reverse_cmd="tail -r"
    fi
    # sed -e '$a\' ensures trailing newline to prevent line concatenation when reversing
    versions=$(npm view @anthropic-ai/claude-code versions --json 2>/dev/null | \
        tr -d '[]"\n ' | tr ',' '\n' | sed -e '$a\' | $reverse_cmd)

    if [[ "$mode" == "blacklist" ]]; then
        local blacklist
        blacklist=$(get_blacklist)

        # Find first non-blacklisted version (newest first)
        for version in $versions; do
            if ! echo "$blacklist" | grep -qx "$version"; then
                echo "$version"
                return 0
            fi
        done

        # Fallback to npm latest if all are blacklisted (unlikely)
        npm view @anthropic-ai/claude-code version 2>/dev/null
    else
        local whitelist
        whitelist=$(get_whitelist)

        # Find first whitelisted version (newest first)
        for version in $versions; do
            if echo "$whitelist" | grep -qx "$version"; then
                echo "$version"
                return 0
            fi
        done

        # Fallback to first whitelisted version if none available on npm
        echo "$whitelist" | head -1
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
    local mode
    mode=$(get_default_mode)

    if command -v claude &> /dev/null; then
        current_version=$(claude --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "unknown")
        info "Claude Code currently installed: v${current_version}"
    fi

    info "Version filter mode: ${mode}"

    # Get recommended version if targeting latest
    if [[ "$target_version" == "latest" ]]; then
        info "Checking for recommended versions..."
        target_version=$(get_recommended_claude_code_version)

        if [[ -n "$target_version" ]]; then
            local npm_latest
            npm_latest=$(npm view @anthropic-ai/claude-code version 2>/dev/null || echo "")

            if [[ "$npm_latest" != "$target_version" ]]; then
                if [[ "$mode" == "blacklist" ]]; then
                    warn "Latest version v${npm_latest} is blacklisted, using v${target_version} instead"
                else
                    warn "Latest version v${npm_latest} is not whitelisted, using v${target_version} instead"
                fi
            else
                info "Recommended version: v${target_version}"
            fi
        else
            if [[ "$mode" == "blacklist" ]]; then
                warn "No allowed version found (all are blacklisted)"
            else
                warn "No whitelisted version found, please check whitelist in Cargo.toml"
            fi
            return 1
        fi
    else
        # Check if explicitly requested version is allowed
        if ! is_version_allowed "$target_version"; then
            if [[ "$mode" == "blacklist" ]]; then
                warn "Version v${target_version} is blacklisted (known issues), proceeding anyway..."
            else
                warn "Version v${target_version} is not whitelisted (may have issues), proceeding anyway..."
            fi
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

    npm install -g "@anthropic-ai/claude-code@${target_version}"

    local new_version
    new_version=$(claude --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "unknown")
    success "Claude Code installed: v${new_version}"
}

# Download pre-built binary from GitHub releases
# Tries: gh cli (best for private repos) -> GitHub API -> direct download
download_binary() {
    local version="$1"
    local temp_dir
    temp_dir=$(mktemp -d)

    info "Checking for pre-built binary..."

    local downloaded=false

    # Method 1: Use gh cli if available (best for private repos, handles auth automatically)
    if command -v gh &> /dev/null; then
        # Get asset ID for our artifact
        local asset_id
        asset_id=$(gh api "repos/${REPO_OWNER}/${REPO_NAME}/releases/tags/${version}" --jq ".assets[] | select(.name==\"${ARTIFACT_NAME}\") | .id" 2>/dev/null)

        if [[ -n "$asset_id" ]]; then
            if gh api "repos/${REPO_OWNER}/${REPO_NAME}/releases/assets/${asset_id}" -H "Accept: application/octet-stream" > "${temp_dir}/cu" 2>/dev/null; then
                downloaded=true
            fi
        fi
    fi

    # Method 2: Use GitHub API with token (for private repos without gh cli)
    if [[ "$downloaded" != "true" ]] && [[ -n "${GITHUB_TOKEN:-}" ]]; then
        local api_url="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/tags/${version}"
        local release_json

        if command -v curl &> /dev/null; then
            release_json=$(curl -fsSL -H "Authorization: token $GITHUB_TOKEN" "$api_url" 2>/dev/null)
        elif command -v wget &> /dev/null; then
            release_json=$(wget -qO- --header="Authorization: token $GITHUB_TOKEN" "$api_url" 2>/dev/null)
        fi

        if [[ -n "$release_json" ]]; then
            # Extract asset ID using grep/sed (works without jq)
            local asset_id
            # Find the asset block for our artifact and extract its ID
            asset_id=$(echo "$release_json" | grep -o "\"id\":[0-9]*,\"node_id\":\"[^\"]*\",\"name\":\"${ARTIFACT_NAME}\"" | grep -o "\"id\":[0-9]*" | sed 's/"id"://')

            if [[ -n "$asset_id" ]]; then
                local asset_api_url="https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/assets/${asset_id}"

                if command -v curl &> /dev/null; then
                    if curl -fsSL -H "Authorization: token $GITHUB_TOKEN" -H "Accept: application/octet-stream" "$asset_api_url" -o "${temp_dir}/cu" 2>/dev/null; then
                        downloaded=true
                    fi
                elif command -v wget &> /dev/null; then
                    if wget -q --header="Authorization: token $GITHUB_TOKEN" --header="Accept: application/octet-stream" "$asset_api_url" -O "${temp_dir}/cu" 2>/dev/null; then
                        downloaded=true
                    fi
                fi
            fi
        fi
    fi

    # Method 3: Direct download URL (works for public repos only)
    if [[ "$downloaded" != "true" ]]; then
        local download_url="${REPO_URL}/releases/download/${version}/${ARTIFACT_NAME}"
        if download "$download_url" "${temp_dir}/cu" 2>/dev/null; then
            downloaded=true
        fi
    fi

    if [[ "$downloaded" != "true" ]]; then
        rm -rf "$temp_dir"
        return 1
    fi

    # Verify we got a real binary, not an error page
    if [[ ! -s "${temp_dir}/cu" ]] || file "${temp_dir}/cu" 2>/dev/null | grep -q "text\|HTML"; then
        rm -rf "$temp_dir"
        return 1
    fi

    chmod +x "${temp_dir}/cu"
    mv "${temp_dir}/cu" "${INSTALL_DIR}/cu"
    rm -rf "$temp_dir"

    # Create symlinks for cui, cug, and cutx
    ln -sf "${INSTALL_DIR}/cu" "${INSTALL_DIR}/cui"
    ln -sf "${INSTALL_DIR}/cu" "${INSTALL_DIR}/cug"
    ln -sf "${INSTALL_DIR}/cu" "${INSTALL_DIR}/cutx"
    ln -sf "${INSTALL_DIR}/cu" "${INSTALL_DIR}/claude-unleashed"

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

    # Install all binaries (cu, cui, cug, cutx)
    for bin in cu cui cug cutx; do
        if [[ -f "target/release/$bin" ]]; then
            cp "target/release/$bin" "${INSTALL_DIR}/$bin"
            chmod +x "${INSTALL_DIR}/$bin"
        fi
    done

    # claude-unleashed is an alias for cu
    ln -sf "${INSTALL_DIR}/cu" "${INSTALL_DIR}/claude-unleashed"

    # Cleanup
    rm -rf "$temp_dir"

    success "Built and installed from source"
}

# Download and install support files
install_support_files() {
    info "Installing support files..."

    # Download patches directory (for auto-mode patching)
    mkdir -p "${INSTALL_DIR}/patches/versions"

    # Get list of patch configs from repo
    for version in "2.1.0" "2.1.2" "2.1.3" "2.1.4" "2.1.5" "2.1.12"; do
        if download "${RAW_URL}/scripts/patches/versions/${version}.conf" "${INSTALL_DIR}/patches/versions/${version}.conf" 2>/dev/null; then
            :
        fi
    done

    # Download restart/exit helper scripts (for MCP tools)
    for script in "restart-claude" "exit-claude"; do
        if download "${RAW_URL}/scripts/${script}" "${INSTALL_DIR}/${script}" 2>/dev/null; then
            chmod +x "${INSTALL_DIR}/${script}"
        fi
    done

    success "Support files installed"
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

        # Use the cu binary's built-in patch command
        if "${INSTALL_DIR}/cu" patch 2>/dev/null; then
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
                error "Cargo not found. Install Rust (https://rustup.rs/) to build from source."
                exit 1
            fi
        fi
    fi

    # Install support files (patches, helper scripts)
    install_support_files

    # Run patch
    run_patch

    # Show completion message
    echo ""
    echo "╭──────────────────────────────────────────╮"
    echo "│         Installation Complete!           │"
    echo "╰──────────────────────────────────────────╯"
    echo ""
    echo "Installed commands:"
    echo "  cu       - Main CLI (run Claude with unleashed features)"
    echo "  cui      - TUI mode (profile & version management)"
    echo "  cutx     - Headless tmux mode"
    echo ""
    echo "Quick start:"
    echo "  cu                 - Launch Claude directly"
    echo "  cu --tui           - Launch TUI"
    echo "  cu --auto          - Launch with auto mode"
    echo "  cu -p \"prompt\"     - Headless mode with prompt"
    echo "  cu tmux start      - Start tmux session"
    echo ""

    show_path_instructions

    success "Done! Run 'cu' to start Claude Unleashed."
}

main "$@"
