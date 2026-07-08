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

# 0. 清理已有服务
echo "🧹 检查已有服务..."
for port in 3000 5173; do
    if command -v fuser &>/dev/null; then
        fuser -k ${port}/tcp 2>/dev/null && echo "   已终止端口 $port 上的旧进程" || true
    elif command -v lsof &>/dev/null; then
        pids=$(lsof -ti :$port 2>/dev/null) || true
        if [ -n "$pids" ]; then
            echo "$pids" | xargs kill 2>/dev/null && echo "   已终止端口 $port 上的旧进程" || true
        fi
    fi
done
sleep 0.5  # 给进程一点时间释放端口
echo ""

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

# 2. 加载 .env（不覆盖已存在的环境变量）
if [ -f ".env" ]; then
    set -a
    source .env
    set +a
fi

# 仅在未设置时提供默认值
export RUST_LOG="${RUST_LOG:-info,im_server=debug,tower_http=debug}"
export JWT_SECRET="${JWT_SECRET:-dev-secret-change-me-in-production}"
export INVITE_CODE="${INVITE_CODE:-IM2024}"
export SERVER_PORT="${SERVER_PORT:-3000}"
export TLS_MODE="${TLS_MODE:-none}"
export ADMIN_USERNAME="${ADMIN_USERNAME:-admin}"
export ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin123}"

echo "🔧 环境变量:"
echo "   RUST_LOG=$RUST_LOG"
echo "   JWT_SECRET=${JWT_SECRET:0:8}..."
echo "   INVITE_CODE=$INVITE_CODE"
echo "   SERVER_PORT=$SERVER_PORT"
echo "   TLS_MODE=$TLS_MODE"
echo "   ADMIN_USERNAME=$ADMIN_USERNAME"
echo "   ADMIN_PASSWORD=***"
echo ""

# 3. 强制 Cargo 检测前端变化后重新链接
#    rust-embed 在编译时嵌入 frontend/dist/，但 Cargo 不追踪这些文件。
#    touch src/embed.rs 确保每次运行都重新链接最新的前端。
touch src/embed.rs

# 4. 启动
if [ -n "$RELEASE_FLAG" ]; then
    echo "🚀 cargo run --release"
    cargo run --release
else
    echo "🚀 cargo run (debug)"
    cargo run
fi
