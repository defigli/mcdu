#!/bin/bash
set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.4.0"
    echo ""
    echo "This downloads the release tarball from GitHub and updates checksums."
    exit 1
fi

VERSION="$1"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

URL="https://github.com/mikalv/mcdu/archive/refs/tags/v$VERSION.tar.gz"
TEMP_FILE=$(mktemp)

echo "Downloading v$VERSION tarball..."
if ! curl -sL "$URL" -o "$TEMP_FILE"; then
    echo "Error: Failed to download $URL"
    echo "Make sure the release tag exists on GitHub."
    rm -f "$TEMP_FILE"
    exit 1
fi

SHA256=$(shasum -a 256 "$TEMP_FILE" | cut -d' ' -f1)
rm -f "$TEMP_FILE"

echo "SHA256: $SHA256"
echo ""

# Homebrew tap is auto-updated by the release workflow (mikalv/homebrew-mcdu)

# Update AUR PKGBUILD
sed -i.bak "s/sha256sums=('.*')/sha256sums=('$SHA256')/" "$ROOT_DIR/packaging/aur/PKGBUILD"
rm -f "$ROOT_DIR/packaging/aur/PKGBUILD.bak"
echo "  ✓ packaging/aur/PKGBUILD"

# Update AUR .SRCINFO
sed -i.bak "s/sha256sums = .*/sha256sums = $SHA256/" "$ROOT_DIR/packaging/aur/.SRCINFO"
rm -f "$ROOT_DIR/packaging/aur/.SRCINFO.bak"
echo "  ✓ packaging/aur/.SRCINFO"

echo ""
echo "Checksums updated for v$VERSION"
