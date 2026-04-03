#!/bin/bash
set -euo pipefail

REPO="maqalaqil/mhost"
INSTALL_DIR="${MHOST_INSTALL_DIR:-/usr/local/bin}"

# ─── Detect platform ─────────────────────────────────────
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin) TARGET_OS="apple-darwin" ;;
  linux)  TARGET_OS="unknown-linux-musl" ;;
  *)      echo "Error: Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  TARGET_ARCH="x86_64" ;;
  aarch64|arm64) TARGET_ARCH="aarch64" ;;
  *)             echo "Error: Unsupported arch: $ARCH"; exit 1 ;;
esac

TARGET="${TARGET_ARCH}-${TARGET_OS}"

# ─── Get latest version ──────────────────────────────────
echo "  Detecting latest version..."
VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null | grep '"tag_name"' | cut -d'"' -f4)

if [ -z "$VERSION" ]; then
  echo "  Error: Could not detect latest version"
  exit 1
fi

# ─── Download ─────────────────────────────────────────────
ARCHIVE="mhost-${TARGET}.tar.gz"
URL="https://github.com/$REPO/releases/download/$VERSION/$ARCHIVE"

echo "  Downloading $ARCHIVE..."

TMP=$(mktemp -d)
trap "rm -rf $TMP" EXIT

if ! curl -fsSL "$URL" -o "$TMP/$ARCHIVE"; then
  echo "  Error: Download failed from $URL"
  exit 1
fi

# ─── Extract ──────────────────────────────────────────────
tar xzf "$TMP/$ARCHIVE" -C "$TMP"

# ─── Install ──────────────────────────────────────────────
if [ ! -w "$INSTALL_DIR" ]; then
  SUDO="sudo"
else
  SUDO=""
fi

$SUDO install -m 755 "$TMP/mhost" "$INSTALL_DIR/mhost"
$SUDO install -m 755 "$TMP/mhostd" "$INSTALL_DIR/mhostd" 2>/dev/null || true

# ─── Verify + Success message ─────────────────────────────
VER=$("$INSTALL_DIR/mhost" -v 2>/dev/null || echo "mhost")

echo ""
echo "  ╔══════════════════════════════════════════════════════╗"
echo "  ║                                                      ║"
echo "  ║   ✔  mhost installed successfully                    ║"
echo "  ║                                                      ║"
printf "  ║   %-52s║\n" "$VER"
printf "  ║   Platform: %-40s║\n" "$TARGET"
printf "  ║   Location: %-40s║\n" "$INSTALL_DIR"
echo "  ║                                                      ║"
echo "  ║   Get started:                                       ║"
echo "  ║     mhost start server.js        Start a process     ║"
echo "  ║     mhost list                   See what's running  ║"
echo "  ║     mhost logs <app>             View logs           ║"
echo "  ║     mhost dev server.js          Dev mode            ║"
echo "  ║     mhost --help                 All commands        ║"
echo "  ║                                                      ║"
echo "  ║   Docs: https://mhostai.com                          ║"
echo "  ║                                                      ║"
echo "  ╚══════════════════════════════════════════════════════╝"
echo ""
