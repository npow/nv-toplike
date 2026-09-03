#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# nv-toplike installer script

set -e

REPO="npow/nv-toplike"
BINARY="nv-toplike"

# Color helpers
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    CYAN=''
    BOLD=''
    NC=''
fi

echo "${BOLD}${CYAN}Installing nv-toplike (Real-time NVIDIA GPU telemetry)...${NC}"

# Detect OS
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
if [ "$OS" != "linux" ]; then
    echo "${RED}Error: nv-toplike is designed for Linux systems with NVIDIA NVML drivers.${NC}"
    exit 1
fi

# Detect Architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)
        TARGET="x86_64-unknown-linux-musl"
        ;;
    aarch64|arm64)
        TARGET="aarch64-unknown-linux-gnu"
        ;;
    *)
        echo "${RED}Error: Unsupported architecture: $ARCH${NC}"
        exit 1
        ;;
esac

# Find latest release tag
LATEST_TAG=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
if [ -z "$LATEST_TAG" ]; then
    LATEST_TAG="v0.1.0"
fi

TARBALL="nv-toplike-${LATEST_TAG}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${TARBALL}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Fetching ${BOLD}${TARBALL}${NC} from GitHub Releases..."
if ! curl -fSL "$URL" -o "${TMP_DIR}/${TARBALL}"; then
    # Fallback to gnu if musl build isn't found
    if [ "$TARGET" = "x86_64-unknown-linux-musl" ]; then
        TARGET="x86_64-unknown-linux-gnu"
        TARBALL="nv-toplike-${LATEST_TAG}-${TARGET}.tar.gz"
        URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${TARBALL}"
        curl -fSL "$URL" -o "${TMP_DIR}/${TARBALL}" || {
            echo "${RED}Failed to download binary from ${URL}${NC}"
            exit 1
        }
    else
        echo "${RED}Failed to download binary from ${URL}${NC}"
        exit 1
    fi
fi

tar -xzf "${TMP_DIR}/${TARBALL}" -C "$TMP_DIR"

# Determine install location
if [ -w "/usr/local/bin" ]; then
    INSTALL_DIR="/usr/local/bin"
elif [ -n "$HOME" ]; then
    INSTALL_DIR="${HOME}/.local/bin"
    mkdir -p "$INSTALL_DIR"
else
    INSTALL_DIR="/usr/bin"
fi

EXTRACTED_DIR="${TMP_DIR}/nv-toplike-${LATEST_TAG}-${TARGET}"
if [ -f "${EXTRACTED_DIR}/${BINARY}" ]; then
    cp "${EXTRACTED_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
elif [ -f "${TMP_DIR}/${BINARY}" ]; then
    cp "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
else
    find "$TMP_DIR" -name "$BINARY" -type f -exec cp {} "${INSTALL_DIR}/${BINARY}" \;
fi

chmod +x "${INSTALL_DIR}/${BINARY}"

echo "${GREEN}${BOLD}✓ Successfully installed ${BINARY} to ${INSTALL_DIR}/${BINARY}${NC}"

# Check PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo "${CYAN}Note: Ensure ${INSTALL_DIR} is in your PATH:${NC}"
        echo "  export PATH=\"\$PATH:${INSTALL_DIR}\""
        ;;
esac

echo "Run ${BOLD}${BINARY}${NC} to start monitoring!"
