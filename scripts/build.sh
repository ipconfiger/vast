#!/usr/bin/env bash
# ===========================================================================
# build.sh — One-click build for VAST IM Server
#
# Builds both the frontend (Bun + Vite) and backend (Cargo) in the correct
# order.  Output binary is at:  target/release/im-server
#
# Usage:
#   ./scripts/build.sh            # full release build
#   ./scripts/build.sh --debug    # debug build (dev)
# ===========================================================================
set -euo pipefail

# ---- Colors ----
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC} $1"; }
pass()  { echo -e "${GREEN}[PASS]${NC} $1"; }
fail()  { echo -e "${RED}[FAIL]${NC} $1"; exit 1; }

# ---- Config ----
PROFILE="${1:-release}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

if [[ "$PROFILE" == "--debug" ]]; then
    PROFILE="debug"
fi

# ---- Prerequisites ----
info "Checking prerequisites..."

command -v rustc &>/dev/null || fail "rustc not found — install Rust via https://rustup.rs"
command -v cargo &>/dev/null || fail "cargo not found — install Rust via https://rustup.rs"
command -v bun  &>/dev/null || fail "bun not found — install via https://bun.sh"

RUSTC_VERSION=$(rustc --version)
BUN_VERSION=$(bun --version)
info "rustc: $RUSTC_VERSION"
info "bun:   $BUN_VERSION"

# ---- Frontend ----
info "Installing frontend dependencies..."
cd "$PROJECT_DIR/frontend"
bun install --frozen-lockfile

info "Building frontend..."
bun run build
pass "Frontend built (dist/)"

# ---- Backend ----
info "Building backend ($PROFILE profile)..."
cd "$PROJECT_DIR"

if [[ "$PROFILE" == "debug" ]]; then
    cargo build
    BINARY_PATH="target/debug/im-server"
else
    cargo build --release
    BINARY_PATH="target/release/im-server"
fi

pass "Backend built: $BINARY_PATH"

# ---- Summary ----
echo ""
echo "============================================"
echo " Build complete!"
echo "============================================"
echo "  Binary:  $PROJECT_DIR/$BINARY_PATH"
echo "  Frontend: $PROJECT_DIR/frontend/dist/"
echo ""
echo "  Run:  $BINARY_PATH"
echo "============================================"
