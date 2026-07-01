#!/usr/bin/env bash
# ===========================================================================
# bench.sh — SQLite/IM Server benchmark suite
#
# Tests:
#   1. Insert 1000 messages with overall timing
#   2. 100 concurrent reads — p50 / p95 / p99 latency
#   3. 50 WebSocket connections — memory usage snapshot
#
# Prerequisites: curl, jq, bc, websocat
# ===========================================================================
set -euo pipefail

# ------------------------------------------------------------------ config
BASE_URL="${BENCH_BASE_URL:-http://localhost:3000}"
WS_URL="${BENCH_WS_URL:-ws://localhost:3000/ws}"
TOKEN="${BENCH_TOKEN:-}"
USERNAME="${BENCH_USER:-benchuser}"
PASSWORD="${BENCH_PASS:-benchpass}"
CHANNEL_ID=""
PID_FILE="/tmp/vast-bench-server.pid"

# ------------------------------------------------------------------ colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

pass()  { echo -e "${GREEN}[PASS]${NC} $1"; }
fail()  { echo -e "${RED}[FAIL]${NC} $1"; }
info()  { echo -e "${CYAN}[INFO]${NC} $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }

# --------------------------------------------------------------- helpers
die() { echo "$1" >&2; exit 1; }

check_deps() {
    for cmd in curl jq bc; do
        command -v "$cmd" &>/dev/null || die "Missing dependency: $cmd"
    done
    if ! command -v websocat &>/dev/null; then
        warn "websocat not found — WS memory test will be skipped"
        SKIP_WS=1
    else
        SKIP_WS=0
    fi
}

# Authenticate (or use provided token)
ensure_token() {
    if [[ -n "$TOKEN" ]]; then
        info "Using provided token"
        return
    fi
    info "Authenticating as $USERNAME..."
    # Try register first, login on conflict
    local resp
    resp=$(curl -sf -X POST "$BASE_URL/api/auth/register" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}" 2>/dev/null) || true
    if [[ -z "$resp" ]]; then
        resp=$(curl -sf -X POST "$BASE_URL/api/auth/login" \
            -H "Content-Type: application/json" \
            -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}") || die "Login failed"
    fi
    TOKEN=$(echo "$resp" | jq -r '.access_token // empty')
    [[ -n "$TOKEN" ]] || die "Failed to obtain auth token"
    pass "Authenticated (token: ${TOKEN:0:12}...)"
}

# Create a channel and return its ID
create_channel() {
    local name="bench-$(date +%s)"
    info "Creating channel '$name'..."
    local resp
    resp=$(curl -sf -X POST "$BASE_URL/api/channels" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $TOKEN" \
        -d "{\"name\":\"$name\",\"description\":\"benchmark channel\"}")
    CHANNEL_ID=$(echo "$resp" | jq -r '.id // empty')
    [[ -n "$CHANNEL_ID" ]] || die "Failed to create channel"
    pass "Channel created: $CHANNEL_ID"
}

# ------------------------------------------------------------------- test 1
test_insert_messages() {
    echo ""
    echo "========================================"
    echo " Test 1: Insert 1000 messages (timing)"
    echo "========================================"

    local count=1000
    local start total elapsed

    start=$(date +%s%N)
    for i in $(seq 1 "$count"); do
        local payload
        payload=$(jq -n --arg t "msg $i" '{text: $t}')
        curl -sf -X POST "$BASE_URL/api/channels/$CHANNEL_ID/messages" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer $TOKEN" \
            -d "{\"msg_type\":\"text\",\"payload\":$payload}" >/dev/null
    done
    elapsed=$(($(date +%s%N) - start))
    total=$(echo "scale=3; $elapsed / 1000000000" | bc)
    local rps
    rps=$(echo "scale=0; $count / $total" | bc)
    pass "Inserted $count messages in ${total}s ($rps msg/s)"
}

# ------------------------------------------------------------------- test 2
test_concurrent_reads() {
    echo ""
    echo "=========================================="
    echo " Test 2: 100 concurrent reads (latency)"
    echo "=========================================="

    local concurrency=100
    local results_file
    results_file=$(mktemp)
    local start total elapsed

    start=$(date +%s%N)
    for i in $(seq 1 "$concurrency"); do
        (
            local t0 t1 lat
            t0=$(date +%s%N)
            curl -sf "$BASE_URL/api/channels/$CHANNEL_ID/messages?limit=50" \
                -H "Authorization: Bearer $TOKEN" >/dev/null 2>&1
            t1=$(date +%s%N)
            lat=$(echo "scale=3; ($t1 - $t0) / 1000000" | bc)  # ms
            echo "$lat" >> "$results_file"
        ) &
    done
    wait
    elapsed=$(echo "scale=3; ($(date +%s%N) - $start) / 1000000" | bc)

    # sort latencies for percentile calculation
    sort -n "$results_file" -o "$results_file"
    local lines
    lines=$(wc -l < "$results_file")
    local p50_idx p95_idx p99_idx
    p50_idx=$((lines * 50 / 100))
    p95_idx=$((lines * 95 / 100))
    p99_idx=$((lines * 99 / 100))
    [[ $p50_idx -lt 1 ]] && p50_idx=1
    [[ $p95_idx -lt 1 ]] && p95_idx=1
    [[ $p99_idx -lt 1 ]] && p99_idx=1

    local p50 p95 p99
    p50=$(sed -n "${p50_idx}p" "$results_file")
    p95=$(sed -n "${p95_idx}p" "$results_file")
    p99=$(sed -n "${p99_idx}p" "$results_file")
    rm -f "$results_file"

    pass "Concurrent reads: total=${elapsed}ms  p50=${p50}ms  p95=${p95}ms  p99=${p99}ms"
}

# ------------------------------------------------------------------- test 3
test_ws_memory() {
    echo ""
    echo "========================================"
    echo " Test 3: 50 WS connections (memory)"
    echo "========================================"

    if [[ "$SKIP_WS" -eq 1 ]]; then
        warn "Skipping — websocat not installed"
        echo "  Install: cargo install websocat"
        return
    fi

    # Get process memory before
    local mem_before mem_after
    mem_before=$(get_server_mem)

    # Open 50 concurrent WebSocket connections
    local pids=()
    for i in $(seq 1 50); do
        # Using a short timeout; each connection stays open ~3 seconds
        timeout 5 websocat -U "$WS_URL?token=$TOKEN" </dev/null 2>/dev/null &
        pids+=($!)
    done

    info "Waiting for connections to settle..."
    sleep 4

    # Kill any lingering websocat processes
    for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true

    mem_after=$(get_server_mem)
    local delta=$((mem_after - mem_before))

    pass "WS memory: before=${mem_before}KB  after=${mem_after}KB  delta=${delta}KB"
}

get_server_mem() {
    if [[ -f "$PID_FILE" ]]; then
        local pid
        pid=$(cat "$PID_FILE")
        if kill -0 "$pid" 2>/dev/null; then
            awk '/^VmRSS:/{print $2}' "/proc/$pid/status" 2>/dev/null || echo "0"
            return
        fi
    fi
    # Fallback: try pgrep
    local pid
    pid=$(pgrep -f "im-server" 2>/dev/null | head -1) || true
    if [[ -n "$pid" ]]; then
        awk '/^VmRSS:/{print $2}' "/proc/$pid/status" 2>/dev/null || echo "0"
    else
        echo "0"
    fi
}

# =================================================================== main
main() {
    echo "============================================"
    echo " IM Server Benchmark Suite"
    echo " Target: $BASE_URL"
    echo "============================================"

    check_deps
    ensure_token
    create_channel

    test_insert_messages
    test_concurrent_reads
    test_ws_memory

    echo ""
    echo "============================================"
    echo " All benchmarks complete"
    echo "============================================"
}

main "$@"
