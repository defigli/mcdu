#!/bin/bash
set -e

if [ -z "$1" ]; then
    echo "Usage: $0 <new-version>"
    echo "Example: $0 0.4.0"
    exit 1
fi

NEW_VERSION="$1"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

echo "Bumping version to $NEW_VERSION..."

# Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$ROOT_DIR/Cargo.toml"
rm -f "$ROOT_DIR/Cargo.toml.bak"
echo "  ✓ Cargo.toml"

# AUR PKGBUILD
sed -i.bak "s/^pkgver=.*/pkgver=$NEW_VERSION/" "$ROOT_DIR/packaging/aur/PKGBUILD"
rm -f "$ROOT_DIR/packaging/aur/PKGBUILD.bak"
echo "  ✓ packaging/aur/PKGBUILD"

# AUR .SRCINFO
sed -i.bak "s/pkgver = .*/pkgver = $NEW_VERSION/" "$ROOT_DIR/packaging/aur/.SRCINFO"
sed -i.bak "s/mcdu-.*\.tar\.gz/mcdu-$NEW_VERSION.tar.gz/g" "$ROOT_DIR/packaging/aur/.SRCINFO"
rm -f "$ROOT_DIR/packaging/aur/.SRCINFO.bak"
echo "  ✓ packaging/aur/.SRCINFO"

# Debian changelog (add new entry at top)
DATE=$(date -R)
TEMP_CHANGELOG=$(mktemp)
cat > "$TEMP_CHANGELOG" << EOF
mcdu ($NEW_VERSION-1) unstable; urgency=medium

  * Release $NEW_VERSION

 -- Mikal Villa <m@meeh.dev>  $DATE

EOF
cat "$ROOT_DIR/packaging/debian/changelog" >> "$TEMP_CHANGELOG"
mv "$TEMP_CHANGELOG" "$ROOT_DIR/packaging/debian/changelog"
echo "  ✓ packaging/debian/changelog"

# RPM spec
sed -i.bak "s/^Version:.*/Version:        $NEW_VERSION/" "$ROOT_DIR/packaging/rpm/mcdu.spec"
rm -f "$ROOT_DIR/packaging/rpm/mcdu.spec.bak"
echo "  ✓ packaging/rpm/mcdu.spec"

echo ""
echo "Version bumped to $NEW_VERSION"
echo ""
echo "Next steps:"
echo "  1. Run 'cargo build' to update Cargo.lock"
echo "  2. Review changes: git diff"
echo "  3. Commit: git commit -am 'Bump version to $NEW_VERSION'"
echo "  4. Tag: git tag v$NEW_VERSION"
echo "  5. Push: git push && git push --tags"
