#!/bin/sh
# audit-risk installer
# Usage: curl -sSf https://raw.githubusercontent.com/834063245-creator/HoloGram/main/install.sh | sh
# Or:    curl -sSf https://raw.githubusercontent.com/834063245-creator/HoloGram/main/install.sh | sh -s -- --prefix ~/.local

set -eu

REPO="834063245-creator/HoloGram"
BINARY="audit-risk"
DEFAULT_PREFIX="/usr/local"

# ── helpers ──────────────────────────────────────────────────────────────────

say() { printf '\033[1;32m[audit-risk]\033[0m %s\n' "$*"; }
err() { printf '\033[1;31m[audit-risk error]\033[0m %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || err "required command not found: $1"; }

# ── parse args ───────────────────────────────────────────────────────────────

PREFIX="$DEFAULT_PREFIX"
VERSION=""

for arg in "$@"; do
  case "$arg" in
    --prefix=*) PREFIX="${arg#--prefix=}" ;;
    --prefix)   shift; PREFIX="$1" ;;
    --version=*) VERSION="${arg#--version=}" ;;
    --version)  shift; VERSION="$1" ;;
    --help|-h)
      echo "Usage: install.sh [--prefix <dir>] [--version <tag>]"
      echo "  --prefix   Install directory (default: /usr/local, binary goes to PREFIX/bin)"
      echo "  --version  Specific version tag to install (default: latest)"
      exit 0
      ;;
  esac
done

INSTALL_DIR="$PREFIX/bin"

# ── detect OS and architecture ───────────────────────────────────────────────

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  OS_NAME="linux" ;;
  Darwin) OS_NAME="macos" ;;
  *)      err "Unsupported OS: $OS. Please build from source: https://github.com/$REPO" ;;
esac

case "$ARCH" in
  x86_64 | amd64) ARCH_NAME="x64" ;;
  arm64 | aarch64) ARCH_NAME="arm64" ;;
  *) err "Unsupported architecture: $ARCH. Please build from source: https://github.com/$REPO" ;;
esac

ASSET_NAME="${BINARY}-${OS_NAME}-${ARCH_NAME}"

# ── fetch latest version if not pinned ───────────────────────────────────────

need curl

if [ -z "$VERSION" ]; then
  say "Fetching latest release version..."
  VERSION="$(curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' \
    | head -1 \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
  [ -n "$VERSION" ] || err "Could not determine latest version. Set --version to install a specific release."
fi

say "Installing $BINARY $VERSION ($OS_NAME/$ARCH_NAME) → $INSTALL_DIR"

# ── download binary ───────────────────────────────────────────────────────────

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}"
TMP_BIN="$(mktemp)"

say "Downloading from $DOWNLOAD_URL"
if ! curl -sSfL "$DOWNLOAD_URL" -o "$TMP_BIN"; then
  rm -f "$TMP_BIN"
  err "Download failed. Check that version $VERSION exists: https://github.com/$REPO/releases"
fi

# ── verify checksum if available ─────────────────────────────────────────────

CHECKSUM_URL="https://github.com/${REPO}/releases/download/${VERSION}/checksums.txt"
TMP_SUMS="$(mktemp)"

if curl -sSfL "$CHECKSUM_URL" -o "$TMP_SUMS" 2>/dev/null; then
  if command -v sha256sum >/dev/null 2>&1; then
    EXPECTED="$(grep "$ASSET_NAME" "$TMP_SUMS" | awk '{print $1}')"
    ACTUAL="$(sha256sum "$TMP_BIN" | awk '{print $1}')"
    if [ -n "$EXPECTED" ] && [ "$ACTUAL" != "$EXPECTED" ]; then
      rm -f "$TMP_BIN" "$TMP_SUMS"
      err "Checksum mismatch! Expected: $EXPECTED  Got: $ACTUAL"
    fi
    [ -n "$EXPECTED" ] && say "Checksum verified ✓"
  elif command -v shasum >/dev/null 2>&1; then
    EXPECTED="$(grep "$ASSET_NAME" "$TMP_SUMS" | awk '{print $1}')"
    ACTUAL="$(shasum -a 256 "$TMP_BIN" | awk '{print $1}')"
    if [ -n "$EXPECTED" ] && [ "$ACTUAL" != "$EXPECTED" ]; then
      rm -f "$TMP_BIN" "$TMP_SUMS"
      err "Checksum mismatch! Expected: $EXPECTED  Got: $ACTUAL"
    fi
    [ -n "$EXPECTED" ] && say "Checksum verified ✓"
  fi
fi
rm -f "$TMP_SUMS"

# ── install ───────────────────────────────────────────────────────────────────

chmod +x "$TMP_BIN"
DEST="$INSTALL_DIR/$BINARY"

install_binary() {
  mkdir -p "$INSTALL_DIR"
  mv "$TMP_BIN" "$DEST"
}

if install_binary 2>/dev/null; then
  :
elif command -v sudo >/dev/null 2>&1; then
  say "Needs sudo to write to $INSTALL_DIR"
  sudo mkdir -p "$INSTALL_DIR"
  sudo mv "$TMP_BIN" "$DEST"
  sudo chmod +x "$DEST"
else
  rm -f "$TMP_BIN"
  err "Cannot write to $INSTALL_DIR. Try: install.sh --prefix ~/.local"
fi

# ── verify install ────────────────────────────────────────────────────────────

if ! command -v "$BINARY" >/dev/null 2>&1; then
  say "Installed to $DEST"
  say "Add $INSTALL_DIR to your PATH if not already present:"
  say "  export PATH=\"$INSTALL_DIR:\$PATH\""
else
  say "Installed: $(command -v $BINARY)"
fi

"$DEST" 2>&1 | head -3 || true
say "Done. Run \`$BINARY help\` to get started."
