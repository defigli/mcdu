#!/usr/bin/env bash

# Build and sign macOS binaries for mcdu release
# Requires: Xcode, Rust, macOS signing certificate

set -euo pipefail

VERSION="${1:-}"
SIGNING_IDENTITY="${2:-}"

if [ -z "$VERSION" ] || [ -z "$SIGNING_IDENTITY" ]; then
  echo "Usage: $0 <version> <signing-identity>"
  echo "Example: $0 0.5.0 'Developer ID Application: Your Name (TEAM_ID)'"
  exit 1
fi

echo "Building mcdu v$VERSION..."
echo "Signing with identity: $SIGNING_IDENTITY"

# Build for both architectures
echo ""
echo "Building x86_64-apple-darwin..."
cargo build --release --target x86_64-apple-darwin

echo "Building aarch64-apple-darwin..."
cargo build --release --target aarch64-apple-darwin

# Sign binaries
echo ""
echo "Signing binaries..."

# Sign x86_64 binary
codesign --force --deep --sign "$SIGNING_IDENTITY" \
  target/x86_64-apple-darwin/release/mcdu

# Sign aarch64 binary
codesign --force --deep --sign "$SIGNING_IDENTITY" \
  target/aarch64-apple-darwin/release/mcdu

# Verify signatures
echo ""
echo "Verifying signatures..."
codesign -dv target/x86_64-apple-darwin/release/mcdu
codesign -dv target/aarch64-apple-darwin/release/mcdu

# Create tarballs
echo ""
echo "Creating tarballs..."
mkdir -p release

cp target/x86_64-apple-darwin/release/mcdu release/
cd release
tar -czf "mcdu-macos-x86_64-$VERSION.tar.gz" mcdu
rm mcdu

cp ../target/aarch64-apple-darwin/release/mcdu .
tar -czf "mcdu-macos-aarch64-$VERSION.tar.gz" mcdu
cd ..

# Show checksums
echo ""
echo "Checksums:"
sha256sum release/*.tar.gz

echo ""
echo "Done! Upload release/*.tar.gz to GitHub release"
