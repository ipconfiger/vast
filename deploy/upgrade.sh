#!/usr/bin/env bash
#
# upgrade.sh — im-server 一键升级
#
# 流程: 构建前端(dist) -> musl 交叉编译后端(前端编译期内嵌) -> 上传二进制 -> 远端热更新
#
# 连接配置从 deploy/.env.deploy 读取 (该文件已 gitignore, 不含明文凭据)
# 模板: deploy/.env.deploy.example
#
# 用法:
#   ./deploy/upgrade.sh
set -euo pipefail

# ---------- 定位项目根 ----------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# ---------- 颜色输出 ----------
if [[ -t 1 ]]; then
  C=$'\033[1;36m'; G=$'\033[1;32m'; R=$'\033[1;31m'; Y=$'\033[1;33m'; N=$'\033[0m'
else
  C=''; G=''; R=''; Y=''; N=''
fi
step() { printf "\n${C}▶ %s${N}\n" "$*"; }
ok()   { printf "${G}✔ %s${N}\n" "$*"; }
warn() { printf "${Y}⚠ %s${N}\n" "$*"; }
die()  { printf "${R}✖ %s${N}\n" "$*" >&2; exit 1; }
trap 'die "升级中断 (line $LINENO)"' ERR

# ---------- 加载环境配置 (deploy/.env.deploy, gitignored) ----------
ENV_FILE="${ENV_FILE:-$SCRIPT_DIR/.env.deploy}"
if [[ ! -f "$ENV_FILE" ]]; then
  die "未找到配置文件: $ENV_FILE — 请复制模板并填值: cp $SCRIPT_DIR/.env.deploy.example $ENV_FILE"
fi
set -a; . "$ENV_FILE"; set +a

# ---------- 配置 (来自 .env.deploy 或环境变量, 带必填校验) ----------
: "${SSH_KEY:?未配置 SSH_KEY (见 $ENV_FILE)}"
: "${REMOTE_HOST:?未配置 REMOTE_HOST (见 $ENV_FILE)}"
: "${REMOTE_USER:=root}"
: "${REMOTE_BIN:=/tmp/im-server}"
: "${REMOTE_DEPLOY_SCRIPT:=/tmp/deploy-im-server.sh}"

TARGET_TRIPLE="x86_64-unknown-linux-musl"
BIN_NAME="im-server"

# ---------- 0. 前置检查 ----------
step "前置检查"
command -v npm >/dev/null            || die "未找到 npm (前端构建需要)"
command -v rust_build_x86 >/dev/null || die "未找到 rust_build_x86 (musl 交叉编译 wrapper)"
[[ -f "$SSH_KEY" ]]                 || die "SSH 私钥不存在: $SSH_KEY"
[[ -f Cargo.toml ]]                 || die "未在项目根发现 Cargo.toml: $PROJECT_ROOT"
[[ -d frontend ]]                   || die "未发现 frontend 目录: $PROJECT_ROOT/frontend"
[[ -f "$SCRIPT_DIR/deploy-im-server.sh" ]] || die "未发现部署脚本: $SCRIPT_DIR/deploy-im-server.sh"
ok "环境就绪 (host=$REMOTE_HOST, user=$REMOTE_USER)"

# ---------- 1. 构建前端 ----------
step "构建前端 -> frontend/dist"
( cd frontend && npm install --legacy-peer-deps && npm run build )
[[ -d frontend/dist ]] || die "前端构建失败: frontend/dist 不存在"
ok "前端 dist 已生成 ($(du -sh frontend/dist | cut -f1))"

# ---------- 2. musl 交叉编译 (rust-embed 在编译期内嵌前端 dist) ----------
step "musl 交叉编译 ($TARGET_TRIPLE, release)"
rust_build_x86

GLOBAL_TARGET="${CARGO_TARGET_DIR:-$HOME/.cargo/global-target}"
BIN_PATH="$GLOBAL_TARGET/$TARGET_TRIPLE/release/$BIN_NAME"
[[ -x "$BIN_PATH" ]] || die "编译产物不存在或不可执行: $BIN_PATH"
ok "编译完成: $BIN_PATH ($(du -h "$BIN_PATH" | cut -f1))"

# ---------- 3. 上传 ----------
step "上传到 ${REMOTE_USER}@${REMOTE_HOST}"
SSH_OPTS=(-i "$SSH_KEY" -o StrictHostKeyChecking=accept-new -o ConnectTimeout=10)
scp "${SSH_OPTS[@]}" "$BIN_PATH"                       "${REMOTE_USER}@${REMOTE_HOST}:${REMOTE_BIN}"
# 同步最新部署脚本, 保证远端逻辑与本地一致
scp "${SSH_OPTS[@]}" "$SCRIPT_DIR/deploy-im-server.sh" "${REMOTE_USER}@${REMOTE_HOST}:${REMOTE_DEPLOY_SCRIPT}"
ok "二进制 + 部署脚本已上传"

# ---------- 4. 远端热更新 ----------
step "远端热更新 (stop -> backup -> replace -> start -> healthcheck)"
ssh "${SSH_OPTS[@]}" "${REMOTE_USER}@${REMOTE_HOST}" "bash '$REMOTE_DEPLOY_SCRIPT'"

echo
ok "升级完成 🎉  (${REMOTE_USER}@${REMOTE_HOST})"
