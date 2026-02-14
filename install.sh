#!/bin/sh
set -e

# LogPulse installer — auto-detects OS/arch, downloads latest release
# Usage: curl -fsSL https://raw.githubusercontent.com/vltamanec/logpulse/main/install.sh | sh

REPO="vltamanec/logpulse"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
BINARY="logpulse"

# --- Detect platform ---
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  OS_TARGET="unknown-linux-gnu" ;;
    Darwin) OS_TARGET="apple-darwin" ;;
    MINGW*|MSYS*|CYGWIN*) OS_TARGET="pc-windows-msvc" ;;
    *) echo "Error: unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64)  ARCH_TARGET="x86_64" ;;
    aarch64|arm64) ARCH_TARGET="aarch64" ;;
    *) echo "Error: unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

TARGET="${ARCH_TARGET}-${OS_TARGET}"

# --- Get latest version ---
if command -v curl >/dev/null 2>&1; then
    LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
elif command -v wget >/dev/null 2>&1; then
    LATEST=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
else
    echo "Error: curl or wget is required" >&2
    exit 1
fi

if [ -z "$LATEST" ]; then
    echo "Error: could not determine latest version" >&2
    exit 1
fi

echo "Installing ${BINARY} ${LATEST} for ${TARGET}..."

# --- Download ---
if [ "$OS_TARGET" = "pc-windows-msvc" ]; then
    ARCHIVE="${BINARY}-${LATEST}-${TARGET}.zip"
else
    ARCHIVE="${BINARY}-${LATEST}-${TARGET}.tar.gz"
fi

URL="https://github.com/${REPO}/releases/download/${LATEST}/${ARCHIVE}"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${URL}..."
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$URL" -o "${TMPDIR}/${ARCHIVE}"
else
    wget -q "$URL" -O "${TMPDIR}/${ARCHIVE}"
fi

# --- Extract ---
cd "$TMPDIR"
if [ "$OS_TARGET" = "pc-windows-msvc" ]; then
    unzip -q "$ARCHIVE"
else
    tar xzf "$ARCHIVE"
fi

# --- Install ---
if [ -w "$INSTALL_DIR" ]; then
    mv "$BINARY" "$INSTALL_DIR/"
else
    echo "Need sudo to install to ${INSTALL_DIR}"
    sudo mv "$BINARY" "$INSTALL_DIR/"
fi

chmod +x "${INSTALL_DIR}/${BINARY}"

echo ""
echo "Done! ${BINARY} ${LATEST} installed to ${INSTALL_DIR}/${BINARY}"
echo ""
echo "Quick start:"
echo "  logpulse /var/log/syslog"
echo "  docker logs -f myapp | logpulse -"
echo "  logpulse docker myapp /var/log/app.log"
