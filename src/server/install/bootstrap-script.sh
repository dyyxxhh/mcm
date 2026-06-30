#!/bin/bash
# MCM Bootstrap Installer
# Installs/updates MCM — an apt-like Minecraft manager.
#
# Usage:
#   curl -fsSL https://mc.dyyapp.com/install | bash
#
# Env overrides (all optional):
#   MCM_INSTALL_DIR          Install directory (default: ~/.local/bin)
#   MCM_RELEASE_BASE_URL     Base URL for release artifacts
#   MCM_INSTALL_OS           Override OS detection (for testing)
#   MCM_INSTALL_ARCH         Override arch detection (for testing)
#   MCM_INSTALL_DRY_RUN      If "true", print actions without executing

set -euo pipefail

# ---- Configuration defaults ----
RELEASE_BASE_URL="${MCM_RELEASE_BASE_URL:-https://mc.dyyapp.com}"
INSTALL_DIR="${MCM_INSTALL_DIR:-}"
DRY_RUN="${MCM_INSTALL_DRY_RUN:-false}"

# ---- Platform detection ----
OS="${MCM_INSTALL_OS:-$(uname -s | tr '[:upper:]' '[:lower:]')}"
ARCH="${MCM_INSTALL_ARCH:-$(uname -m)}"

case "${OS}" in
    linux) ;;
    *)
        echo "Error: unsupported OS '${OS}'." >&2
        echo "MCM bootstrap currently supports Linux x86_64 only." >&2
        echo "See https://github.com/code-yeongyu/mcm for other platforms." >&2
        exit 1
        ;;
esac

case "${ARCH}" in
    x86_64|amd64) ;;
    *)
        echo "Error: unsupported architecture '${ARCH}'." >&2
        echo "MCM bootstrap currently supports x86_64 only." >&2
        exit 1
        ;;
esac

# ---- Install directory resolution ----
if [ -z "${INSTALL_DIR}" ]; then
    if [ -d "${HOME}/.local/bin" ] && [ -w "${HOME}/.local/bin" ]; then
        INSTALL_DIR="${HOME}/.local/bin"
    elif [ -n "${HOME}" ] && [ -w "${HOME}" ]; then
        INSTALL_DIR="${HOME}/.local/bin"
        mkdir -p "${INSTALL_DIR}"
    else
        INSTALL_DIR="${HOME:?}/.local/bin"
        mkdir -p "${INSTALL_DIR}"
    fi
fi

# ---- Asset URLs ----
BINARY_NAME="mcm"
RELEASE_NAME="mcm-linux-x86_64"
DOWNLOAD_URL="${RELEASE_BASE_URL}/release/${RELEASE_NAME}"
CHECKSUM_URL="${RELEASE_BASE_URL}/release/${RELEASE_NAME}.sha256"

# ---- Temp directory (cleaned up on exit) ----
TEMP_DIR=$(mktemp -d -t mcm-install-XXXXXX 2>/dev/null || mktemp -d 2>/dev/null || echo "/tmp/mcm-install-$$")
mkdir -p "${TEMP_DIR}"
trap 'rm -rf "${TEMP_DIR}"' EXIT

# ---- Dry-run / preview ----
if [ "${DRY_RUN}" = "true" ]; then
    echo "[DRY-RUN] Release base URL: ${RELEASE_BASE_URL}"
    echo "[DRY-RUN] Would download:   ${RELEASE_BASE_URL}/release/${RELEASE_NAME}"
    echo "[DRY-RUN] Would verify:     ${RELEASE_BASE_URL}/release/${RELEASE_NAME}.sha256"
    echo "[DRY-RUN] Would install to: ${INSTALL_DIR}/${BINARY_NAME}"
    echo "[DRY-RUN] OS=${OS} ARCH=${ARCH}"
    exit 0
fi

# ---- Download binary ----
echo "Downloading MCM for ${OS}/${ARCH}..."

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "${DOWNLOAD_URL}" -o "${TEMP_DIR}/${BINARY_NAME}"
    curl -fsSL "${CHECKSUM_URL}" -o "${TEMP_DIR}/${RELEASE_NAME}.sha256" 2>/dev/null || true
elif command -v wget >/dev/null 2>&1; then
    wget -q "${DOWNLOAD_URL}" -O "${TEMP_DIR}/${BINARY_NAME}"
    wget -q "${CHECKSUM_URL}" -O "${TEMP_DIR}/${RELEASE_NAME}.sha256" 2>/dev/null || true
else
    echo "Error: requires curl or wget to download." >&2
    exit 1
fi

# ---- Verify checksum ----
echo "Verifying checksum..."
_checksum_ok=false

if [ -f "${TEMP_DIR}/${RELEASE_NAME}.sha256" ] && [ -s "${TEMP_DIR}/${RELEASE_NAME}.sha256" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
        if (cd "${TEMP_DIR}" && sha256sum -c "${RELEASE_NAME}.sha256" >/dev/null 2>&1); then
            _checksum_ok=true
        fi
    elif command -v shasum >/dev/null 2>&1; then
        if (cd "${TEMP_DIR}" && shasum -a 256 -c "${RELEASE_NAME}.sha256" >/dev/null 2>&1); then
            _checksum_ok=true
        fi
    elif command -v openssl >/dev/null 2>&1; then
        _expected=$(cut -d' ' -f1 < "${TEMP_DIR}/${RELEASE_NAME}.sha256")
        _computed=$(openssl dgst -sha256 "${TEMP_DIR}/${BINARY_NAME}" | cut -d' ' -f2)
        if [ "${_expected}" = "${_computed}" ]; then
            _checksum_ok=true
        fi
    fi
fi

if [ "${_checksum_ok}" != "true" ]; then
    echo "Error: checksum verification failed." >&2
    echo "The downloaded binary may be corrupted or tampered with." >&2
    echo "Aborting installation." >&2
    rm -rf "${TEMP_DIR}"
    exit 1
fi

echo "Checksum verified."

# ---- Install binary ----
chmod +x "${TEMP_DIR}/${BINARY_NAME}"

if [ ! -w "${INSTALL_DIR}" ]; then
    echo "Error: cannot write to ${INSTALL_DIR}." >&2
    echo "Run the following command manually:" >&2
    echo "" >&2
    echo "  sudo install -m 755 \"${TEMP_DIR}/${BINARY_NAME}\" \"${INSTALL_DIR}/${BINARY_NAME}\"" >&2
    echo "" >&2
    echo "Or set MCM_INSTALL_DIR to a user-writable directory and re-run." >&2
    rm -rf "${TEMP_DIR}"
    exit 1
fi

install -m 755 "${TEMP_DIR}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"

echo ""
echo "MCM has been installed to ${INSTALL_DIR}/${BINARY_NAME}"
echo ""
echo "Run '${INSTALL_DIR}/${BINARY_NAME} --help' to get started."
