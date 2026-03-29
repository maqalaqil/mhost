#!/bin/bash
# Usage: ./scripts/release.sh 0.2.0
# Creates a git tag and pushes it, triggering the full CI/CD pipeline.

set -e

VERSION="${1:?Usage: ./scripts/release.sh <version> (e.g. 0.2.0)}"

# Validate format
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
  echo "Error: Version must be in semver format (e.g. 0.2.0, 1.0.0-beta.1)"
  exit 1
fi

echo "Releasing mhost v${VERSION}"
echo "─────────────────────────────"

# Check we're on main
BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$BRANCH" != "main" ]; then
  echo "Warning: Not on main branch (on: $BRANCH)"
  read -p "Continue anyway? [y/N] " -n 1 -r
  echo
  [[ $REPLY =~ ^[Yy]$ ]] || exit 1
fi

# Check working tree is clean
if ! git diff --quiet HEAD; then
  echo "Error: Working tree is dirty. Commit or stash changes first."
  exit 1
fi

# Run tests
echo "Running tests..."
cargo test --workspace
echo ""

# Run clippy
echo "Running clippy..."
cargo clippy --workspace -- -D warnings
echo ""

# Update version in Cargo.toml
echo "Updating version to ${VERSION}..."
sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
rm -f Cargo.toml.bak

# Update npm package version
if [ -f npm-package/package.json ]; then
  cd npm-package
  npm version "$VERSION" --no-git-tag-version --allow-same-version 2>/dev/null || true
  cd ..
fi

# Commit version bump
git add Cargo.toml npm-package/package.json 2>/dev/null || git add Cargo.toml
git commit -m "chore: bump version to ${VERSION}"

# Create tag
git tag -a "v${VERSION}" -m "Release v${VERSION}"

echo ""
echo "Tag v${VERSION} created."
echo ""
echo "To trigger the release pipeline:"
echo "  git push origin main --tags"
echo ""
echo "This will:"
echo "  1. Build binaries for 6 platforms"
echo "  2. Create GitHub Release with checksums"
echo "  3. Publish to npm"
echo "  4. Publish to crates.io"
echo "  5. Update Homebrew tap"
echo "  6. Deploy website to GitHub Pages"
echo "  7. Push Docker image to GHCR"
