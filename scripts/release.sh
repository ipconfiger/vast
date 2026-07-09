#!/usr/bin/env bash
# ===========================================================================
# release.sh — Build, package, and publish a GitHub release for VAST
#
# Usage:
#   ./scripts/release.sh                    # auto-detect version from Cargo.toml
#   ./scripts/release.sh v0.2.0             # specify version tag
#   ./scripts/release.sh v0.2.0 --dry-run   # build + package only, skip upload
# ===========================================================================
set -euo pipefail

# ---- Colors ----
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC} $1"; }
pass()  { echo -e "${GREEN}[PASS]${NC} $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
fail()  { echo -e "${RED}[FAIL]${NC} $1"; exit 1; }

# ---- Config ----
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
DRY_RUN=false

# ---- Parse args ----
TAG=""
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=true ;;
        v*)        TAG="$arg" ;;
        *)         ;;
    esac
done

if [[ -z "$TAG" ]]; then
    TAG="v$(grep '^version' "$PROJECT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')"
    info "Auto-detected version: $TAG"
fi

RELEASE_NAME="$TAG"
PACKAGE_DIR="$PROJECT_DIR/target/release-pkg"
ZIP_FILE="$PROJECT_DIR/target/${TAG}.zip"
BINARY_PATH="$PROJECT_DIR/target/release/im-server"

# ---- Step 1: Build ----
info "Building release binary..."
bash "$SCRIPT_DIR/build.sh"
echo ""

# ---- Step 2: Package ----
info "Packaging $TAG..."
rm -rf "$PACKAGE_DIR"
mkdir -p "$PACKAGE_DIR"

cp "$BINARY_PATH"           "$PACKAGE_DIR/im-server"
cp "$PROJECT_DIR/.env.example"  "$PACKAGE_DIR/.env.example"
cp "$PROJECT_DIR/README.md" "$PACKAGE_DIR/README.md"

cd "$PACKAGE_DIR"
zip -r "$ZIP_FILE" . > /dev/null
cd "$PROJECT_DIR"

rm -rf "$PACKAGE_DIR"
pass "Package created: $ZIP_FILE ($(du -h "$ZIP_FILE" | cut -f1))"

# ---- Step 3: Release notes ----
NOTES_FILE="$PROJECT_DIR/target/release-notes.md"
{
    echo "## $TAG"
    echo ""
    echo "### Changes"
    echo ""
    git log --oneline --no-merges "$(git describe --tags --abbrev=0 2>/dev/null || echo '')"..HEAD 2>/dev/null | sed 's/^/- /' || echo "- Initial release"
    echo ""
    echo "### Binary"
    echo "- \`im-server\` — Linux x86\_64 release build"
    echo "- Copy \`.env.example\` to \`.env\` and configure before running"
} > "$NOTES_FILE"

# ---- Step 4: GitHub Release ----
if $DRY_RUN; then
    pass "Dry run complete. Package at: $ZIP_FILE"
    echo ""
    echo "Would create release: $TAG"
    echo "Release notes:"
    cat "$NOTES_FILE"
else
    info "Creating GitHub release: $TAG"
    if ! command -v gh &>/dev/null; then
        fail "gh CLI not found. Install: https://cli.github.com"
    fi

    gh release create "$TAG" \
        --title "$RELEASE_NAME" \
        --notes-file "$NOTES_FILE" \
        "$ZIP_FILE"

    pass "Release published: $(gh release view "$TAG" --json url -q '.url')"
fi

rm -f "$NOTES_FILE"
echo ""
echo "============================================"
echo " Done!"
echo "============================================"
