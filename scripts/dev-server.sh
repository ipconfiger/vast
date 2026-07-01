#!/usr/bin/env bash
# dev-server.sh — 调试模式启动 IM Server
# 用法: ./scripts/dev-server.sh [--no-frontend] [--release]

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# 解析参数
BUILD_FRONTEND=true
RELEASE_FLAG=""

for arg in "$@"; do
    case "$arg" in
        --no-frontend) BUILD_FRONTEND=false ;;
        --release)     RELEASE_FLAG="--release" ;;
        *)             echo "未知参数: $arg"; exit 1 ;;
    esac
done

echo "╔══════════════════════════════════════════════╗"
echo "║   VAST IM Server — 调试模式                    ║"
echo "╚══════════════════════════════════════════════╝"
echo ""

# 1. 构建前端 (可选)
if $BUILD_FRONTEND && [ -d "frontend" ]; then
    echo "📦 构建前端..."
    cd frontend
    bun install --silent 2>/dev/null || true
    bun run build 2>&1 | tail -3 || {
        echo "⚠️  前端构建失败, 创建最小 dist..."
        mkdir -p dist
        echo '<!DOCTYPE html><html><body><div id="root"></div></body></html>' > dist/index.html
    }
    cd "$PROJECT_DIR"
    echo "✅ 前端构建完成"
    echo ""
fi

# 2. 设置环境变量
export RUST_LOG="${RUST_LOG:-info,im_server=debug,tower_http=debug}"
export JWT_SECRET="${JWT_SECRET:-dev-secret-change-me-in-production}"
export INVITE_CODE="${INVITE_CODE:-IM2024}"
export SERVER_PORT="${SERVER_PORT:-3000}"
export TLS_MODE="${TLS_MODE:-none}"

echo "🔧 环境变量:"
echo "   RUST_LOG=$RUST_LOG"
echo "   JWT_SECRET=$JWT_SECRET"
echo "   INVITE_CODE=$INVITE_CODE"
echo "   SERVER_PORT=$SERVER_PORT"
echo "   TLS_MODE=$TLS_MODE"
echo ""

# 3. 启动
if [ -n "$RELEASE_FLAG" ]; then
    echo "🚀 cargo run --release"
    cargo run --release
else
    echo "🚀 cargo run (debug)"
    cargo run
fi
