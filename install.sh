#!/usr/bin/env bash
# bbrew installer - Inspired by Homebrew's installer
# Usage: /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/bbrew/bbrew/main/install.sh)"
#        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/bbrew/bbrew/main/install.sh)" -- --system
#        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/bbrew/bbrew/main/install.sh)" -- --channel nightly

set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================
REPO="metasecdev/bbrew"
BINARY_NAME="bbrew"
DEFAULT_INSTALL_DIR="${HOME}/.local/bin"
SYSTEM_INSTALL_DIR="/opt/bbrew/bin"
GH_API="https://api.github.com/repos/${REPO}"
GH_RELEASES_API="${GH_API}/releases"
GH_RAW_BASE="https://raw.githubusercontent.com/${REPO}/main"

# Colors
if [[ -t 1 ]] && command -v tput >/dev/null 2>&1 && [[ $(tput colors 2>/dev/null || echo 0) -ge 8 ]]; then
    BOLD=$(tput bold)
    RED=$(tput setaf 1)
    GREEN=$(tput setaf 2)
    YELLOW=$(tput setaf 3)
    BLUE=$(tput setaf 4)
    MAGENTA=$(tput setaf 5)
    CYAN=$(tput setaf 6)
    RESET=$(tput sgr0)
else
    BOLD=""
    RED=""
    GREEN=""
    YELLOW=""
    BLUE=""
    MAGENTA=""
    CYAN=""
    RESET=""
fi

# =============================================================================
# Logging Functions
# =============================================================================
log_info() { echo -e "${BLUE}==>${RESET} ${BOLD}$*${RESET}" >&2; }
log_success() { echo -e "${GREEN}==>${RESET} ${BOLD}$*${RESET}" >&2; }
log_warn() { echo -e "${YELLOW}==>${RESET} ${BOLD}$*${RESET}" >&2; }
log_error() { echo -e "${RED}==>${RESET} ${BOLD}$*${RESET}" >&2; }
log_debug() { [[ "${DEBUG:-}" == "1" ]] && echo -e "${MAGENTA}[DEBUG]${RESET} $*" >&2; }
log_step() { echo -e "${CYAN}==>${RESET} ${BOLD}$*${RESET}" >&2; }

# Progress spinner
spinner_pid=""
start_spinner() {
    local msg="$1"
    echo -n "${BLUE}==>${RESET} ${BOLD}${msg}${RESET} " >&2
    (
        local spin='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
        local i=0
        while true; do
            printf "\r${BLUE}==>${RESET} ${BOLD}${msg}${RESET} ${spin:i++%${#spin}:1}" >&2
            sleep 0.1
        done
    ) &
    spinner_pid=$!
}
stop_spinner() {
    if [[ -n "${spinner_pid}" ]]; then
        kill "${spinner_pid}" 2>/dev/null || true
        wait "${spinner_pid}" 2>/dev/null || true
        spinner_pid=""
        echo -ne "\r\033[K" >&2
    fi
}

# =============================================================================
# Platform Detection
# =============================================================================
detect_arch() {
    local arch
    arch=$(uname -m)
    case "${arch}" in
        x86_64) echo "x86_64" ;;
        arm64|aarch64) echo "aarch64" ;;
        *) log_error "Unsupported architecture: ${arch}"; exit 1 ;;
    esac
}

detect_os() {
    local os
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    case "${os}" in
        darwin) echo "apple-darwin" ;;
        linux) echo "unknown-linux-gnu" ;;
        *) log_error "Unsupported OS: ${os}. bbrew supports macOS and Linux."; exit 1 ;;
    esac
}

detect_platform() {
    local arch os
    arch=$(detect_arch)
    os=$(detect_os)
    echo "${arch}-${os}"
}

# Check if we're on macOS
is_macos() { [[ "$(uname -s)" == "Darwin" ]]; }

# Get macOS version
macos_version() {
    if is_macos; then
        sw_vers -productVersion 2>/dev/null | cut -d. -f1
    else
        echo "0"
    fi
}

# Check if running on Apple Silicon
is_apple_silicon() {
    is_macos && [[ "$(uname -m)" == "arm64" ]]
}

# =============================================================================
# Version/Release Handling
# =============================================================================
get_latest_version() {
    local channel="${1:-stable}"
    local url="${GH_RELEASES_API}"

    if [[ "${channel}" == "nightly" ]]; then
        url="${url}?per_page=1"
    else
        url="${url}/latest"
    fi

    curl -fsSL "${url}" 2>/dev/null | \
        grep '"tag_name":' | head -1 | \
        sed -E 's/.*"tag_name": "([^"]+)".*/\1/'
}

get_asset_url() {
    local version="$1"
    local pattern="$2"

    curl -fsSL "${GH_RELEASES_API}/tags/${version}" 2>/dev/null | \
        grep "browser_download_url" | \
        grep "${pattern}" | \
        head -1 | \
        sed -E 's/.*"browser_download_url": "([^"]+)".*/\1/'
}

get_checksums_url() {
    local version="$1"
    curl -fsSL "${GH_RELEASES_API}/tags/${version}" 2>/dev/null | \
        grep "browser_download_url" | \
        grep "checksums.txt" | \
        head -1 | \
        sed -E 's/.*"browser_download_url": "([^"]+)".*/\1/'
}

# =============================================================================
# Verification
# =============================================================================
verify_sha256() {
    local file="$1"
    local expected="$2"
    local actual
    actual=$(shasum -a 256 "${file}" 2>/dev/null | awk '{print $1}')
    [[ "${actual}" == "${expected}" ]]
}

verify_cosign() {
    local file="$1"
    local version="$2"
    local sig_file="${file}.sig"
    local pubkey_url="https://github.com/${REPO}/releases/download/${version}/cosign.pub"

    if ! command -v cosign >/dev/null 2>&1; then
        return 1
    fi

    # Download signature if not present
    if [[ ! -f "${sig_file}" ]]; then
        local sig_url
        sig_url=$(get_asset_url "${version}" "${BINARY_NAME}.*\.sig")
        if [[ -n "${sig_url}" ]]; then
            curl -fsSL -o "${sig_file}" "${sig_url}" || return 1
        else
            return 1
        fi
    fi

    cosign verify-blob --signature "${sig_file}" --key "${pubkey_url}" "${file}" 2>/dev/null
}

# =============================================================================
# Download & Extract
# =============================================================================
download_release() {
    local version="$1"
    local platform="$2"
    local pattern="${BINARY_NAME}-${platform}.tar.gz"
    local asset_name="${pattern}"

    log_debug "Looking for asset: ${pattern}"

    local download_url
    download_url=$(get_asset_url "${version}" "${pattern}")

    if [[ -z "${download_url}" ]]; then
        log_error "No release asset found for ${platform} in version ${version}"
        log_info "Available assets for ${version}:"
        curl -fsSL "${GH_RELEASES_API}/tags/${version}" 2>/dev/null | \
            grep "browser_download_url" | \
            sed -E 's/.*"browser_download_url": "([^"]+)".*/  \1/'
        return 1
    fi

    local tmp_dir
    tmp_dir=$(mktemp -d -t bbrew-install.XXXXXX)
    local asset_path="${tmp_dir}/${asset_name}"

    start_spinner "Downloading ${asset_name}"
    if ! curl -fsSL -o "${asset_path}" "${download_url}"; then
        stop_spinner
        log_error "Failed to download from ${download_url}"
        rm -rf "${tmp_dir}"
        return 1
    fi
    stop_spinner
    log_success "Downloaded ${asset_name}"

    # Verify SHA256
    local checksums_url
    checksums_url=$(get_checksums_url "${version}")
    if [[ -n "${checksums_url}" ]]; then
        start_spinner "Verifying SHA256"
        local expected_sha256
        expected_sha256=$(curl -fsSL "${checksums_url}" 2>/dev/null | \
            grep "${asset_name}" | awk '{print $1}')

        if [[ -n "${expected_sha256}" ]] && verify_sha256 "${asset_path}" "${expected_sha256}"; then
            stop_spinner
            log_success "SHA256 verified"
        else
            stop_spinner
            if [[ -z "${expected_sha256}" ]]; then
                log_warn "SHA256 not found in checksums, skipping"
            else
                log_error "SHA256 mismatch!"
                rm -rf "${tmp_dir}"
                return 1
            fi
        fi
    else
        log_warn "No checksums file found, skipping SHA256 verification"
    fi

    # Verify cosign signature
    if verify_cosign "${asset_path}" "${version}"; then
        log_success "Cosign signature verified"
    else
        log_debug "Cosign verification skipped or failed"
    fi

    # Extract
    start_spinner "Extracting archive"
    local extract_dir="${tmp_dir}/extract"
    mkdir -p "${extract_dir}"
    if ! tar -xzf "${asset_path}" -C "${extract_dir}" 2>/dev/null; then
        stop_spinner
        log_error "Failed to extract archive"
        rm -rf "${tmp_dir}"
        return 1
    fi
    stop_spinner

    # Find binary
    local binary_path
    binary_path=$(find "${extract_dir}" -name "${BINARY_NAME}" -type f -perm -u+x 2>/dev/null | head -1)

    if [[ -z "${binary_path}" ]]; then
        log_error "Binary not found in extracted archive"
        rm -rf "${tmp_dir}"
        return 1
    fi

    echo "${binary_path}"
}

# =============================================================================
# Build from Source
# =============================================================================
build_from_source() {
    local version="${1:-main}"
    local tmp_dir
    tmp_dir=$(mktemp -d -t bbrew-build.XXXXXX)

    log_info "Building bbrew from source (${version})..."

    # Check for Rust
    if ! command -v cargo >/dev/null 2>&1; then
        log_error "Rust toolchain not found. Install from https://rustup.rs/"
        log_info "Or install the binary release instead (default)"
        return 1
    fi

    # Check for required tools
    local missing_tools=()
    command -v git >/dev/null 2>&1 || missing_tools+=("git")
    command -v cmake >/dev/null 2>&1 || missing_tools+=("cmake")
    command -v pkg-config >/dev/null 2>&1 || missing_tools+=("pkg-config")

    if [[ ${#missing_tools[@]} -gt 0 ]]; then
        log_warn "Missing build tools: ${missing_tools[*]}"
        if is_macos; then
            log_info "Install with: brew install ${missing_tools[*]}"
        else
            log_info "Install with your package manager (apt, dnf, pacman, etc.)"
        fi
        return 1
    fi

    start_spinner "Cloning repository"
    if ! git clone --depth 1 --branch "${version}" "https://github.com/${REPO}.git" "${tmp_dir}/bbrew" 2>/dev/null; then
        stop_spinner
        log_error "Failed to clone repository"
        rm -rf "${tmp_dir}"
        return 1
    fi
    stop_spinner

    start_spinner "Building (this may take a few minutes)"
    if ! (cd "${tmp_dir}/bbrew" && cargo build --release --locked 2>&1 | tail -20); then
        stop_spinner
        log_error "Build failed"
        log_info "Build log saved to ${tmp_dir}/build.log"
        return 1
    fi
    stop_spinner
    log_success "Build complete"

    local binary_path="${tmp_dir}/bbrew/target/release/${BINARY_NAME}"
    if [[ ! -f "${binary_path}" ]]; then
        log_error "Binary not found after build"
        return 1
    fi

    echo "${binary_path}"
}

# =============================================================================
# Installation
# =============================================================================
install_binary() {
    local binary_path="$1"
    local system_install="${2:-false}"
    local target_dir

    if [[ "${system_install}" == "true" ]]; then
        target_dir="${SYSTEM_INSTALL_DIR}"
        log_info "Installing to ${target_dir} (requires sudo)..."
        sudo mkdir -p "${target_dir}"
        sudo cp "${binary_path}" "${target_dir}/${BINARY_NAME}"
        sudo chmod +x "${target_dir}/${BINARY_NAME}"
    else
        target_dir="${DEFAULT_INSTALL_DIR}"
        log_info "Installing to ${target_dir}..."
        mkdir -p "${target_dir}"
        cp "${binary_path}" "${target_dir}/${BINARY_NAME}"
        chmod +x "${target_dir}/${BINARY_NAME}"
    fi

    echo "${target_dir}"
}

# =============================================================================
# Shell Integration
# =============================================================================
get_shell_config() {
    local shell_name
    shell_name=$(basename "${SHELL:-/bin/bash}")

    case "${shell_name}" in
        bash)
            if [[ -f "${HOME}/.bash_profile" ]]; then
                echo "${HOME}/.bash_profile"
            else
                echo "${HOME}/.bashrc"
            fi
            ;;
        zsh) echo "${HOME}/.zshrc" ;;
        fish) echo "${HOME}/.config/fish/config.fish" ;;
        *) echo "" ;;
    esac
}

get_shell_name() {
    basename "${SHELL:-/bin/bash}"
}

add_to_path() {
    local install_dir="$1"
    local rc_file
    rc_file=$(get_shell_config)

    if [[ -z "${rc_file}" ]]; then
        log_warn "Could not detect shell config file"
        return 1
    fi

    local shell_name
    shell_name=$(get_shell_name)
    local path_line

    case "${shell_name}" in
        fish)
            path_line="fish_add_path ${install_dir}"
            ;;
        *)
            path_line="export PATH=\"${install_dir}:\$PATH\""
            ;;
    esac

    # Check if already configured
    if [[ -f "${rc_file}" ]] && grep -Fq "${install_dir}" "${rc_file}" 2>/dev/null; then
        log_info "PATH already configured in ${rc_file}"
        return 0
    fi

    log_info "Adding ${install_dir} to PATH in ${rc_file}..."

    # Ensure directory exists
    mkdir -p "$(dirname "${rc_file}")"

    {
        echo ""
        echo "# Added by bbrew installer on $(date)"
        echo "${path_line}"
    } >> "${rc_file}"

    log_success "Updated ${rc_file}"
}

# =============================================================================
# Post-Install
# =============================================================================
run_doctor() {
    local install_dir="$1"
    local binary="${install_dir}/${BINARY_NAME}"

    if [[ ! -x "${binary}" ]]; then
        log_warn "Binary not executable: ${binary}"
        return 1
    fi

    log_info "Running bbrew doctor..."
    if "${binary}" doctor 2>&1 | head -30; then
        log_success "Doctor check passed"
    else
        log_warn "Doctor check had warnings (this is normal for first install)"
    fi
}

verify_installation() {
    local install_dir="$1"
    local binary="${install_dir}/${BINARY_NAME}"

    if [[ -x "${binary}" ]]; then
        local version
        version=$("${binary}" --version 2>/dev/null | head -1 || echo "unknown")
        log_success "bbrew installed: ${version}"
        return 0
    else
        log_error "Installation verification failed"
        return 1
    fi
}

# =============================================================================
# Uninstall
# =============================================================================
uninstall() {
    local system_install="${1:-false}"
    local install_dir

    if [[ "${system_install}" == "true" ]]; then
        install_dir="${SYSTEM_INSTALL_DIR}"
    else
        install_dir="${DEFAULT_INSTALL_DIR}"
    fi

    local binary="${install_dir}/${BINARY_NAME}"

    log_info "Uninstalling bbrew from ${install_dir}..."

    if [[ -f "${binary}" ]]; then
        if [[ "${system_install}" == "true" ]]; then
            sudo rm -f "${binary}"
            sudo rmdir "${install_dir}" 2>/dev/null || true
        else
            rm -f "${binary}"
            rmdir "${install_dir}" 2>/dev/null || true
        fi
        log_success "Removed binary"
    else
        log_warn "Binary not found at ${binary}"
    fi

    # Remove from PATH in shell config
    local rc_file
    rc_file=$(get_shell_config)
    if [[ -n "${rc_file}" && -f "${rc_file}" ]]; then
        if grep -Fq "${install_dir}" "${rc_file}" 2>/dev/null; then
            log_info "Removing PATH entry from ${rc_file}..."
            sed -i.bak "/# Added by bbrew installer/,/export PATH.*${install_dir//\//\\/}/d" "${rc_file}"
            log_success "Updated ${rc_file}"
        fi
    fi

    # Remove data directories
    local data_dirs=(
        "${HOME}/.local/share/bbrew"
        "${HOME}/.cache/bbrew"
        "${HOME}/.config/bbrew"
    )
    for dir in "${data_dirs[@]}"; do
        if [[ -d "${dir}" ]]; then
            log_info "Removing ${dir}..."
            rm -rf "${dir}"
        fi
    done

    log_success "bbrew uninstalled"
    log_info "Restart your shell to complete removal"
}

# =============================================================================
# Self-Update
# =============================================================================
self_update() {
    local current_version
    current_version=$("${DEFAULT_INSTALL_DIR}/${BINARY_NAME}" --version 2>/dev/null | head -1 || echo "unknown")

    log_info "Current version: ${current_version}"
    log_info "Checking for updates..."

    local latest_version
    latest_version=$(get_latest_version "stable")

    if [[ -z "${latest_version}" ]]; then
        log_error "Could not fetch latest version"
        return 1
    fi

    log_info "Latest version: ${latest_version}"

    if [[ "${current_version}" == *"${latest_version}"* ]]; then
        log_success "Already up to date"
        return 0
    fi

    log_info "Updating to ${latest_version}..."
    # Re-run installer with same flags
    local args=("--version" "${latest_version}")
    if [[ "${SYSTEM_INSTALL:-false}" == "true" ]]; then
        args+=("--system")
    fi
    exec "${BASH_SOURCE[0]}" "${args[@]}"
}

# =============================================================================
# Help
# =============================================================================
show_help() {
    cat <<EOF
${BOLD}bbrew installer${RESET} - Install the bbrew package manager

${BOLD}USAGE:${RESET}
    $0 [OPTIONS]

${BOLD}OPTIONS:${RESET}
    ${BOLD}--channel CHANNEL${RESET}    Release channel: ${BOLD}stable${RESET} (default) or ${BOLD}nightly${RESET}
    ${BOLD}--system${RESET}             Install system-wide to ${SYSTEM_INSTALL_DIR} (requires sudo)
    ${BOLD}--version VERSION${RESET}    Install specific version (e.g., v0.1.0)
    ${BOLD}--build-from-source${RESET}  Build from source instead of downloading binary
    ${BOLD}--local PATH${RESET}         Install from local binary (e.g., ./target/release/bbrew)
    ${BOLD}--uninstall${RESET}          Uninstall bbrew
    ${BOLD}--update${RESET}             Update bbrew to latest version
    ${BOLD}--no-path${RESET}            Don't modify shell PATH
    ${BOLD}--debug${RESET}              Enable debug output
    ${BOLD}--help, -h${RESET}           Show this help

${BOLD}EXAMPLES:${RESET}
    # Install latest stable (recommended)
    /bin/bash -c "\$(curl -fsSL ${GH_RAW_BASE}/install.sh)"

    # Install nightly
    /bin/bash -c "\$(curl -fsSL ${GH_RAW_BASE}/install.sh)" -- --channel nightly

    # Install system-wide (requires sudo)
    /bin/bash -c "\$(curl -fsSL ${GH_RAW_BASE}/install.sh)" -- --system

    # Install specific version
    /bin/bash -c "\$(curl -fsSL ${GH_RAW_BASE}/install.sh)" -- --version v0.1.0

    # Build from source
    /bin/bash -c "\$(curl -fsSL ${GH_RAW_BASE}/install.sh)" -- --build-from-source

    # Update existing installation
    bbrew self-update
    # or
    /bin/bash -c "\$(curl -fsSL ${GH_RAW_BASE}/install.sh)" -- --update

    # Uninstall
    /bin/bash -c "\$(curl -fsSL ${GH_RAW_BASE}/install.sh)" -- --uninstall

${BOLD}ENVIRONMENT VARIABLES:${RESET}
    ${BOLD}DEBUG=1${RESET}           Enable debug output
    ${BOLD}BBREW_INSTALL_DIR${RESET} Custom install directory (default: ~/.local/bin)

${BOLD}REQUIREMENTS:${RESET}
    - macOS 12+ (Monterey) or Linux
    - curl
    - For --build-from-source: Rust toolchain (https://rustup.rs/), git, cmake, pkg-config

${BOLD}MORE INFO:${RESET}
    https://github.com/${REPO}
EOF
}

# =============================================================================
# Main
# =============================================================================
main() {
    # Parse arguments
    local CHANNEL="stable"
    local SYSTEM_INSTALL="false"
    local VERSION=""
    local LOCAL_BINARY=""
    local BUILD_FROM_SOURCE="false"
    local DO_UNINSTALL="false"
    local DO_UPDATE="false"
    local NO_PATH="false"
    local DEBUG_MODE="false"

    while [[ $# -gt 0 ]]; do
        case $1 in
            --channel)
                CHANNEL="$2"
                shift 2
                ;;
            --system)
                SYSTEM_INSTALL="true"
                shift
                ;;
            --version)
                VERSION="$2"
                shift 2
                ;;
            --local)
                LOCAL_BINARY="$2"
                shift 2
                ;;
            --build-from-source)
                BUILD_FROM_SOURCE="true"
                shift
                ;;
            --uninstall)
                DO_UNINSTALL="true"
                shift
                ;;
            --update)
                DO_UPDATE="true"
                shift
                ;;
            --no-path)
                NO_PATH="true"
                shift
                ;;
            --debug)
                DEBUG_MODE="true"
                export DEBUG=1
                shift
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done

    # Handle special commands
    if [[ "${DO_UNINSTALL}" == "true" ]]; then
        uninstall "${SYSTEM_INSTALL}"
        exit 0
    fi

    if [[ "${DO_UPDATE}" == "true" ]]; then
        self_update
        exit 0
    fi

    # Validate channel
    if [[ "${CHANNEL}" != "stable" && "${CHANNEL}" != "nightly" ]]; then
        log_error "Invalid channel: ${CHANNEL}. Must be 'stable' or 'nightly'"
        exit 1
    fi

    # Check prerequisites
    if [[ -z "${LOCAL_BINARY}" && "${BUILD_FROM_SOURCE}" != "true" ]] && ! command -v curl >/dev/null 2>&1; then
        log_error "curl is required but not installed"
        exit 1
    fi

    # Detect platform
    local platform
    platform=$(detect_platform)
    log_info "Platform: ${platform}"

    if is_macos; then
        local macos_ver
        macos_ver=$(macos_version)
        log_info "macOS version: ${macos_ver}"
        if [[ "${macos_ver}" -lt 12 ]]; then
            log_warn "macOS ${macos_ver} detected. bbrew works best on macOS 12+"
        fi
        if is_apple_silicon; then
            log_info "Apple Silicon detected - using arm64 binaries"
        fi
    fi

    local binary_path

    if [[ -n "${LOCAL_BINARY}" ]]; then
        # Install from local binary
        log_info "Installing from local binary: ${LOCAL_BINARY}"
        if [[ ! -f "${LOCAL_BINARY}" ]]; then
            log_error "Local binary not found: ${LOCAL_BINARY}"
            exit 1
        fi
        binary_path="${LOCAL_BINARY}"
        VERSION="local-build"
    elif [[ "${BUILD_FROM_SOURCE}" == "true" ]]; then
        # Build from source
        binary_path=$(build_from_source "${VERSION:-main}") || exit 1
        VERSION="source-build"
    else
        # Download from GitHub Releases
        if [[ -z "${VERSION}" ]]; then
            log_info "Fetching latest ${CHANNEL} version..."
            VERSION=$(get_latest_version "${CHANNEL}")
            if [[ -z "${VERSION}" ]]; then
                log_error "Could not determine latest version"
                exit 1
            fi
        fi
        log_info "Installing version: ${VERSION}"

        binary_path=$(download_release "${VERSION}" "${platform}") || exit 1
    fi

    # Install
    local install_dir
    install_dir=$(install_binary "${binary_path}" "${SYSTEM_INSTALL}")

    # Update shell PATH
    if [[ "${NO_PATH}" != "true" ]]; then
        add_to_path "${install_dir}"
    else
        log_info "Skipping PATH modification (--no-path specified)"
    fi

    # Run doctor
    run_doctor "${install_dir}"

    # Verify
    verify_installation "${install_dir}"

    # Final message
    echo ""
    log_success "bbrew ${VERSION} installed successfully!"
    echo ""

    local shell_name
    shell_name=$(get_shell_name)
    if [[ "${NO_PATH}" != "true" ]]; then
        case "${shell_name}" in
            fish)
                log_info "Run: ${BOLD}source ~/.config/fish/config.fish${RESET} or restart your shell"
                ;;
            *)
                log_info "Run: ${BOLD}source ~/.${shell_name}rc${RESET} or restart your shell"
                ;;
        esac
    fi
    log_info "Then run: ${BOLD}bbrew --help${RESET} to get started"
    log_info "Try: ${BOLD}bbrew search <package>${RESET} or ${BOLD}bbrew install <package>${RESET}"
}

# Run main
main "$@"