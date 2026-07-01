#!/usr/bin/env bash
# ===========================================================================
# e2e-test.sh — End-to-end test runner
#
# 1. Build the backend
# 2. Start the server with a temp DB
# 3. Run Rust integration tests (cargo test --test integration)
# 4. Run frontend Playwright tests (requires bun + playwright)
# 5. Stop the server and clean up
# ===========================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}[INFO]${NC} $1"; }
pass()  { echo -e "${GREEN}[PASS]${NC} $1"; }
fail()  { echo -e "${RED}[FAIL]${NC} $1"; }
warn()  { echo -e "\033[1;33m[WARN]${NC} $1"; }

# ------------------------------------------------------------------ config
PORT="${E2E_PORT:-3099}"
DB_FILE="/tmp/vast-e2e-$$.db"
PID_FILE="/tmp/vast-e2e-$$.pid"
DATA_DIR="/tmp/vast-e2e-data-$$"

cleanup() {
    info "Cleaning up..."
    if [[ -f "$PID_FILE" ]]; then
        PID=$(cat "$PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            kill "$PID" 2>/dev/null || true
            wait "$PID" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    fi
    rm -f "$DB_FILE" "$DB_FILE-shm" "$DB_FILE-wal"
    rm -rf "$DATA_DIR"
}
trap cleanup EXIT INT TERM

# ------------------------------------------------------------------ build
info "Building backend..."
(cd "$PROJECT_DIR" && cargo build --release 2>&1) || {
    fail "Backend build failed"
    exit 1
}
pass "Backend built"

# ------------------------------------------------------------------ start server
info "Starting IM server on port $PORT..."
mkdir -p "$DATA_DIR"
JWT_SECRET="e2e-test-secret-$(date +%s)" \
INVITE_CODE="IM2024" \
    "$PROJECT_DIR/target/release/im-server" &
SERVER_PID=$!
echo "$SERVER_PID" > "$PID_FILE"

# Wait for server to start
for i in $(seq 1 30); do
    if curl -sf "http://localhost:$PORT/" >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

if ! curl -sf "http://localhost:$PORT/" >/dev/null 2>&1; then
    fail "Server failed to start"
    exit 1
fi
pass "Server started on port $PORT"

# ------------------------------------------------------------------ Rust integration tests
info "Running Rust integration tests..."
(cd "$PROJECT_DIR" && cargo test --test integration 2>&1) || {
    fail "Rust integration tests failed"
    exit 1
}
pass "Rust integration tests passed"

# ------------------------------------------------------------------ Frontend Playwright tests
info "Running Playwright tests..."

# Build frontend for the embedded server
info "Building frontend..."
(cd "$PROJECT_DIR/frontend" && bun install --frozen-lockfile 2>&1) || true
(cd "$PROJECT_DIR/frontend" && bun run build 2>&1) || {
    fail "Frontend build failed"
    exit 1
}
pass "Frontend built"

# Rebuild backend with embedded frontend (since we just built frontend/dist)
info "Rebuilding backend with frontend..."
(cd "$PROJECT_DIR" && cargo build --release 2>&1) || {
    fail "Backend rebuild failed"
    exit 1
}

# Restart server with new binary
if [[ -f "$PID_FILE" ]]; then
    OLD_PID=$(cat "$PID_FILE")
    kill "$OLD_PID" 2>/dev/null || true
    wait "$OLD_PID" 2>/dev/null || true
fi

info "Restarting server with embedded frontend..."
JWT_SECRET="e2e-test-secret-$(date +%s)" \
INVITE_CODE="IM2024" \
    "$PROJECT_DIR/target/release/im-server" &
SERVER_PID=$!
echo "$SERVER_PID" > "$PID_FILE"

for i in $(seq 1 30); do
    if curl -sf "http://localhost:$PORT/" >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

# Run Playwright tests
if command -v npx &>/dev/null; then
    info "Running Playwright tests..."
    (cd "$PROJECT_DIR/frontend" && E2E_BASE_URL="http://localhost:$PORT" npx playwright test 2>&1) || {
        fail "Playwright tests failed"
        exit 1
    }
    pass "Playwright tests passed"
else
    warn "npx not found — skipping Playwright tests"
fi

# ------------------------------------------------------------------ done
echo ""
echo "============================================"
pass "All e2e tests passed"
echo "============================================"
