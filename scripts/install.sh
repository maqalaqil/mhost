#!/bin/bash

set -euo pipefail

REPO="maqalaqil/mhost"
INSTALL_DIR="/usr/local/bin"

# Detect OS
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    OS="linux"
elif [[ "$OSTYPE" == "darwin"* ]]; then
    OS="darwin"
else
    echo "Error: Unsupported OS: $OSTYPE"
    exit 1
fi

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)
        ARCH="x86_64"
        ;;
    aarch64)
        ARCH="aarch64"
        ;;
    arm64)
        ARCH="aarch64"
        ;;
    *)
        echo "Error: Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

TARGET_TRIPLE="${ARCH}-${OS}"

# Map to correct Rust target triple
case "$TARGET_TRIPLE" in
    x86_64-linux)
        TARGET="x86_64-unknown-linux-musl"
        ;;
    aarch64-linux)
        TARGET="aarch64-unknown-linux-musl"
        ;;
    x86_64-darwin)
        TARGET="x86_64-apple-darwin"
        ;;
    aarch64-darwin)
        TARGET="aarch64-apple-darwin"
        ;;
    *)
        echo "Error: Unsupported platform: $TARGET_TRIPLE"
        exit 1
        ;;
esac

echo "Detecting latest version..."
LATEST_RELEASE=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest")
VERSION=$(echo "$LATEST_RELEASE" | grep -o '"tag_name": "[^"]*"' | cut -d'"' -f4)

if [ -z "$VERSION" ]; then
    echo "Error: Could not detect latest version"
    exit 1
fi

echo "Found version: $VERSION"

# Download binary
BINARY_NAME="mhost-${TARGET}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY_NAME}"

echo "Downloading mhost from $DOWNLOAD_URL..."

if ! command -v curl &> /dev/null; then
    echo "Error: curl is required but not installed"
    exit 1
fi

# Check if we need sudo
if [ ! -w "$INSTALL_DIR" ]; then
    SUDO="sudo"
else
    SUDO=""
fi

# Download to temporary location
TEMP_FILE=$(mktemp)
trap "rm -f $TEMP_FILE" EXIT

if ! curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_FILE"; then
    echo "Error: Failed to download mhost"
    exit 1
fi

# Install binary
echo "Installing mhost to $INSTALL_DIR/mhost..."
$SUDO mv "$TEMP_FILE" "$INSTALL_DIR/mhost"
$SUDO chmod +x "$INSTALL_DIR/mhost"

# Verify installation
if ! command -v mhost &> /dev/null; then
    echo "Warning: mhost installed but not found in PATH"
    echo "Please add $INSTALL_DIR to your PATH"
else
    echo "Installation complete!"
    echo "Version: $(mhost --version)"
fi
