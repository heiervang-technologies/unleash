#!/usr/bin/env bash
# install-remote.sh - Remote installer for unleash
#
# Usage:
#   curl -fsSL unleash.software/install | bash
#
# Options (via environment variables):
#   GH_TOKEN / GH_PAT / GITHUB_TOKEN - GitHub token for private repo access (any of these work)
#   UNLEASH_VERSION - Specific version to install (default: latest)
#   CLAUDE_CODE_VERSION      - Specific Claude Code version (default: latest)
#   INSTALL_DIR              - Installation directory (default: ~/.local/bin)
#   BUILD_FROM_SOURCE        - Set to "1" to build from source instead of downloading binary
#
# This script:
# 1. Checks prerequisites (curl/wget, optionally cargo)
# 2. Installs Claude Code (native binary preferred, npm fallback)
# 3. Downloads pre-built binary or builds from source
# 4. Sets up unleash command

set -euo pipefail

# Support common GitHub token variable names
GITHUB_TOKEN="${GH_TOKEN:-${GH_PAT:-${GITHUB_TOKEN:-}}}"

# Configuration
REPO_OWNER="heiervang-technologies"
REPO_NAME="unleash"
REPO_URL="https://github.com/${REPO_OWNER}/${REPO_NAME}"
RAW_URL="https://raw.githubusercontent.com/${REPO_OWNER}/${REPO_NAME}/main"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BUILD_FROM_SOURCE="${BUILD_FROM_SOURCE:-0}"
UNLEASH_VERSION="${UNLEASH_VERSION:-latest}"
CLAUDE_CODE_VERSION="${CLAUDE_CODE_VERSION:-latest}"

# GCS bucket for native Claude Code binaries
GCS_BUCKET="https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases"

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

    # Construct artifact name to match release workflow (only linux-x86_64 is built in CI)
    if [[ "$PLATFORM" == "linux" && "$ARCH" == "x86_64" ]]; then
        ARTIFACT_NAME="unleash-linux-x86_64"
    else
        warn "No pre-built binary available for $PLATFORM-$ARCH. Forcing build from source."
        BUILD_FROM_SOURCE="1"
        ARTIFACT_NAME=""
    fi

    info "Detected platform: $PLATFORM ($ARCH)"
}

# Check for required commands
check_prerequisites() {
    local missing=()

    # curl or wget is always required
    if ! command -v curl &> /dev/null && ! command -v wget &> /dev/null; then
        missing+=("curl or wget")
    fi

    # npm is optional (native binary is preferred, npm is fallback)
    if ! command -v npm &> /dev/null; then
        warn "npm not found - Claude Code will be installed via native binary"
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
        echo "  - curl: use your package manager"
        echo "  - cargo: https://rustup.rs/"
        exit 1
    fi

    success "Prerequisites check passed"
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

    # Default to blacklist mode
    echo "blacklist"
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

# Get latest allowed Claude Code version based on filter mode
# Tries GCS first for version discovery, falls back to npm
get_recommended_claude_code_version() {
    local mode
    mode=$(get_default_mode)

    # Try GCS-based version discovery first
    local gcs_latest=""
    if command -v curl &> /dev/null; then
        gcs_latest=$(curl -fsSL "$GCS_BUCKET/latest" 2>/dev/null || echo "")
    elif command -v wget &> /dev/null; then
        gcs_latest=$(wget -qO- "$GCS_BUCKET/latest" 2>/dev/null || echo "")
    fi

    if [[ -n "$gcs_latest" ]]; then
        # Check if GCS latest is allowed by our filter
        if [[ "$mode" == "blacklist" ]]; then
            if ! is_version_blacklisted "$gcs_latest"; then
                echo "$gcs_latest"
                return 0
            fi
        else
            if is_version_whitelisted "$gcs_latest"; then
                echo "$gcs_latest"
                return 0
            fi
        fi
        # GCS latest not allowed, fall through to npm for full version list
    fi

    # Fallback: query npm for version list (requires npm)
    if ! command -v npm &> /dev/null; then
        # No npm available, can't enumerate versions
        return 1
    fi

    local versions
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

# Install Claude Code natively from GCS binary distribution
install_native_claude_code() {
    local version="$1"
    local os arch platform

    # Detect platform for GCS binary naming
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
    if command -v curl &> /dev/null; then
        if ! curl -fsSL -o "${binary_path}.tmp" "$url"; then
            error "Failed to download native binary from GCS"
            rm -f "${binary_path}.tmp"
            return 1
        fi
    elif command -v wget &> /dev/null; then
        if ! wget -q -O "${binary_path}.tmp" "$url"; then
            error "Failed to download native binary from GCS"
            rm -f "${binary_path}.tmp"
            return 1
        fi
    fi

    # Verify checksum from manifest
    local manifest=""
    if command -v curl &> /dev/null; then
        manifest=$(curl -fsSL "$manifest_url" 2>/dev/null || echo "")
    elif command -v wget &> /dev/null; then
        manifest=$(wget -qO- "$manifest_url" 2>/dev/null || echo "")
    fi

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
    ln -sf "$binary_path" "${INSTALL_DIR}/claude"
    success "Claude Code v${version} installed natively"

    # Disable auto-updates since we manage versions
    export DISABLE_AUTOUPDATER=1
}

# Install or update Claude Code
# Prefers native binary from GCS, falls back to npm
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
            info "Recommended version: v${target_version}"
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

    # Prefer native binary from GCS (no Node.js dependency)
    if install_native_claude_code "$target_version" 2>/dev/null; then
        : # install_native_claude_code already prints success
    elif command -v npm &> /dev/null; then
        # Fallback: npm install
        warn "Native binary install failed, falling back to npm"
        npm install -g "@anthropic-ai/claude-code@${target_version}"
    else
        error "Neither native binary nor npm install succeeded"
        return 1
    fi

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
            if gh api "repos/${REPO_OWNER}/${REPO_NAME}/releases/assets/${asset_id}" -H "Accept: application/octet-stream" > "${temp_dir}/unleash" 2>/dev/null; then
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
                    if curl -fsSL -H "Authorization: token $GITHUB_TOKEN" -H "Accept: application/octet-stream" "$asset_api_url" -o "${temp_dir}/unleash" 2>/dev/null; then
                        downloaded=true
                    fi
                elif command -v wget &> /dev/null; then
                    if wget -q --header="Authorization: token $GITHUB_TOKEN" --header="Accept: application/octet-stream" "$asset_api_url" -O "${temp_dir}/unleash" 2>/dev/null; then
                        downloaded=true
                    fi
                fi
            fi
        fi
    fi

    # Method 3: Direct download URL (works for public repos only)
    if [[ "$downloaded" != "true" ]]; then
        local download_url="${REPO_URL}/releases/download/${version}/${ARTIFACT_NAME}"
        if download "$download_url" "${temp_dir}/unleash" 2>/dev/null; then
            downloaded=true
        fi
    fi

    if [[ "$downloaded" != "true" ]]; then
        rm -rf "$temp_dir"
        return 1
    fi

    # Verify we got a real binary, not an error page
    if [[ ! -s "${temp_dir}/unleash" ]] || file "${temp_dir}/unleash" 2>/dev/null | grep -q "text\|HTML"; then
        rm -rf "$temp_dir"
        return 1
    fi

    # Try to verify checksum
    local checksums_url="${REPO_URL}/releases/download/${version}/checksums.txt"
    if download "$checksums_url" "${temp_dir}/checksums.txt" 2>/dev/null && [[ -s "${temp_dir}/checksums.txt" ]] && ! file "${temp_dir}/checksums.txt" 2>/dev/null | grep -q "HTML"; then
        local expected_checksum
        expected_checksum=$(grep "${ARTIFACT_NAME}" "${temp_dir}/checksums.txt" | awk '{print $1}')
        if [[ -n "$expected_checksum" ]]; then
            local actual_checksum
            actual_checksum=$(sha256sum "${temp_dir}/unleash" 2>/dev/null | cut -d' ' -f1 || shasum -a 256 "${temp_dir}/unleash" 2>/dev/null | cut -d' ' -f1)
            if [[ "$actual_checksum" != "$expected_checksum" ]]; then
                error "Checksum verification failed for ${ARTIFACT_NAME}"
                error "  Expected: $expected_checksum"
                error "  Got:      $actual_checksum"
                rm -rf "$temp_dir"
                return 1
            fi
            success "Checksum verified for ${ARTIFACT_NAME}"
        fi
    fi

    chmod +x "${temp_dir}/unleash"
    mv "${temp_dir}/unleash" "${INSTALL_DIR}/unleash"
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

    # Install unleash binary
    if [[ -f "target/release/unleash" ]]; then
        cp "target/release/unleash" "${INSTALL_DIR}/unleash"
        chmod +x "${INSTALL_DIR}/unleash"
    fi

    # Cleanup
    rm -rf "$temp_dir"

    success "Built and installed from source"
}

# Download and install support files
install_support_files() {
    info "Installing support files..."

    # Download restart/exit helper scripts
    for script in "unleash-refresh" "unleash-exit"; do
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

# Ensure onboarding is completed (bypasses interactive prompts)
ensure_onboarding_complete() {
    local claude_json="${HOME}/.claude.json"
    local claude_dir="${HOME}/.claude"

    # Ensure .claude directory exists
    mkdir -p "$claude_dir"

    # Get current Claude version for lastOnboardingVersion
    local claude_version
    claude_version=$(claude --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "2.1.0")

    if [[ -f "$claude_json" ]]; then
        # File exists - update required fields using jq or sed
        if command -v jq &>/dev/null; then
            local tmp_file
            tmp_file=$(mktemp)
            if jq --arg ver "$claude_version" '
                .hasCompletedOnboarding = true |
                .bypassPermissionsModeAccepted = true |
                .lastOnboardingVersion = $ver
            ' "$claude_json" > "$tmp_file" 2>/dev/null; then
                mv "$tmp_file" "$claude_json"
            else
                rm -f "$tmp_file"
            fi
        else
            # Fallback: use python3 if available for safer JSON processing
            if command -v python3 &>/dev/null; then
                local tmp_file
                tmp_file=$(mktemp)
                if python3 -c "
import sys, json
try:
    with open('$claude_json', 'r') as f:
        data = json.load(f)
    data['hasCompletedOnboarding'] = True
    data['bypassPermissionsModeAccepted'] = True
    data['lastOnboardingVersion'] = '$claude_version'
    with open('$tmp_file', 'w') as f:
        json.dump(data, f, indent=2)
except Exception as e:
    sys.exit(1)
" 2>/dev/null; then
                    mv "$tmp_file" "$claude_json"
                else
                    rm -f "$tmp_file"
                fi
            else
                # Final fallback to sed (less safe)
                if grep -q '"hasCompletedOnboarding"' "$claude_json"; then
                    sed -i.bak 's/"hasCompletedOnboarding":\s*false/"hasCompletedOnboarding": true/g' "$claude_json" 2>/dev/null || true
                fi
                if grep -q '"bypassPermissionsModeAccepted"' "$claude_json"; then
                    sed -i.bak 's/"bypassPermissionsModeAccepted":\s*false/"bypassPermissionsModeAccepted": true/g' "$claude_json" 2>/dev/null || true
                fi
                rm -f "${claude_json}.bak"
            fi
        fi
    else
        # Create new file with required fields
        cat > "$claude_json" << EOF
{
  "hasCompletedOnboarding": true,
  "lastOnboardingVersion": "${claude_version}",
  "bypassPermissionsModeAccepted": true,
  "numStartups": 1,
  "installMethod": "unleash"
}
EOF
    fi

    success "Onboarding bypass configured"
}

# Main installation
main() {
    echo ""
    echo "╭──────────────────────────────────────────╮"
    echo "│         unleash Remote Installer         │"
    echo "╰──────────────────────────────────────────╯"
    echo ""

    detect_platform
    check_prerequisites

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Install Claude Code
    install_claude_code

    # Determine version to install
    if [[ "$UNLEASH_VERSION" == "latest" ]]; then
        UNLEASH_VERSION=$(get_latest_version)
        if [[ -z "$UNLEASH_VERSION" ]]; then
            warn "Could not determine latest version, using 'main' branch"
            UNLEASH_VERSION="main"
        fi
    fi
    info "Installing unleash ${UNLEASH_VERSION}"

    # Install binary (try download first, fall back to source)
    if [[ "$BUILD_FROM_SOURCE" == "1" ]]; then
        build_from_source
    else
        if ! download_binary "$UNLEASH_VERSION"; then
            warn "Binary download failed, building from source..."
            if command -v cargo &> /dev/null; then
                build_from_source
            else
                error "Cargo not found. Install Rust (https://rustup.rs/) to build from source."
                exit 1
            fi
        fi
    fi

    # Install support files (helper scripts)
    install_support_files

    # Configure onboarding bypass
    ensure_onboarding_complete

    # Show completion message
    echo ""
    echo "╭──────────────────────────────────────────╮"
    echo "│         Installation Complete!           │"
    echo "╰──────────────────────────────────────────╯"
    echo ""
    echo "Installed commands:"
    echo "  unleash              - TUI interface (profiles & version management)"
    echo "  unleash claude       - Start Claude with wrapper features"
    echo ""
    echo "Quick start:"
    echo "  unleash                 - Launch TUI"
    echo "  unleash claude          - Launch Claude"
    echo "  unleash claude --auto   - Launch with auto mode"
    echo ""

    show_path_instructions

    success "Done! Run 'unleash' to start unleash."
}

main "$@"
