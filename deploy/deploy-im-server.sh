#!/usr/bin/env bash
# im-server 幂等部署/更新脚本
# 首次执行: 自动建用户、目录、生成 .env、安装 systemd unit 并启动
# 再次执行: 备份旧二进制 -> 替换 -> 重启
#
# 用法:
#   ./deploy-im-server.sh                 # 默认从 /tmp/im-server 取二进制
#   ./deploy-im-server.sh /path/to/bin    # 指定二进制源路径
set -euo pipefail

BIN_SRC="${1:-/tmp/im-server}"
INSTALL_DIR="/opt/im-server"
BIN_PATH="$INSTALL_DIR/im-server"
SERVICE="im-server"
SERVICE_FILE="/etc/systemd/system/${SERVICE}.service"
ENV_FILE="$INSTALL_DIR/.env"
LOG_DIR="/var/log/im-server"
RUN_USER="im-server"

log()  { echo "[deploy] $*"; }
err()  { echo "[deploy][ERROR] $*" >&2; }

# --- 0. 前置校验 -----------------------------------------------------------
[[ "$(id -u)" -eq 0 ]] || { err "请以 root 运行"; exit 1; }
[[ -s "$BIN_SRC" ]] || { err "源二进制不存在或为空: $BIN_SRC"; exit 1; }
command -v systemctl >/dev/null || { err "未发现 systemd"; exit 1; }

# --- 1. 首次初始化 (用户/目录/.env) ----------------------------------------
if ! id "$RUN_USER" >/dev/null 2>&1; then
  log "创建系统用户 $RUN_USER"
  useradd -r -s /usr/sbin/nologin "$RUN_USER"
fi
install -d -o "$RUN_USER" -g "$RUN_USER" -m 0750 \
  "$INSTALL_DIR" "$INSTALL_DIR/data" "$INSTALL_DIR/certs"
install -d -o "$RUN_USER" -g "$RUN_USER" -m 0750 "$LOG_DIR"

if [[ ! -f "$ENV_FILE" ]]; then
  log "首次部署: 生成 $ENV_FILE (随机 JWT_SECRET, TLS_MODE=none)"
  JWT_SECRET="$(openssl rand -base64 48 2>/dev/null || head -c 48 /dev/urandom | base64)"
  (
    umask 077
    cat > "$ENV_FILE" <<EOF
# im-server 配置 — 首次自动生成于 $(date -Is)
# 必填: JWT 签名密钥
JWT_SECRET=${JWT_SECRET}
# 数据库 (SQLite)
DATABASE_URL=sqlite://${INSTALL_DIR}/data/im.db
# TLS: none(由前端 nginx 反代终结 TLS) | self-signed | letsencrypt
TLS_MODE=none
# 可选 (按需取消注释):
# ADMIN_PASSWORD=
# INVITE_CODE=
EOF
  )
  chown "$RUN_USER":"$RUN_USER" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  log "已生成 $ENV_FILE — 如需 ADMIN_PASSWORD/INVITE_CODE 请编辑后重启服务"
fi

# --- 2. systemd unit (幂等覆盖) -------------------------------------------
log "写入 systemd unit -> $SERVICE_FILE"
cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=im-server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${RUN_USER}
Group=${RUN_USER}
WorkingDirectory=${INSTALL_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN_PATH}
Restart=always
RestartSec=3
LimitNOFILE=65536
LimitNPROC=4096
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=${INSTALL_DIR} ${LOG_DIR}
StandardOutput=append:${LOG_DIR}/stdout.log
StandardError=append:${LOG_DIR}/stderr.log

[Install]
WantedBy=multi-user.target
EOF

# --- 3. 停止现有服务 -------------------------------------------------------
if systemctl list-unit-files 2>/dev/null | grep -q "^${SERVICE}\.service"; then
  log "停止现有服务"
  systemctl stop "$SERVICE" 2>/dev/null || true
fi

# --- 4. 备份 + 更新二进制 --------------------------------------------------
if [[ -f "$BIN_PATH" ]]; then
  log "备份旧二进制 -> ${BIN_PATH}.bak"
  cp -a "$BIN_PATH" "${BIN_PATH}.bak"
fi
log "安装新二进制: $BIN_SRC -> $BIN_PATH"
install -m 0755 "$BIN_SRC" "$BIN_PATH"
chown "$RUN_USER":"$RUN_USER" "$BIN_PATH"

# --- 5. 启动 ---------------------------------------------------------------
systemctl daemon-reload
systemctl enable "$SERVICE" >/dev/null 2>&1 || true
log "启动服务"
systemctl restart "$SERVICE"

# --- 6. 健康检查 -----------------------------------------------------------
HEALTH="http://127.0.0.1:3000/api/health"
log "健康检查 $HEALTH"
ok=""
for _ in $(seq 1 15); do
  code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 2 "$HEALTH" 2>/dev/null || echo 000)"
  [[ "$code" == "200" ]] && { ok=1; break; }
  sleep 1
done

echo
systemctl --no-pager --full status "$SERVICE" 2>/dev/null | head -15 || true
echo

if [[ -n "$ok" ]]; then
  log "✅ 部署成功，健康检查通过 (200)"
  log "   二进制: $BIN_PATH"
  log "   配置:   $ENV_FILE"
  log "   日志:   $LOG_DIR/{stdout,stderr}.log"
  log "   回滚:   systemctl stop $SERVICE && cp -a ${BIN_PATH}.bak $BIN_PATH && systemctl start $SERVICE"
else
  err "⚠️  服务已启动但健康检查未通过，请排查:"
  err "   journalctl -u $SERVICE -n 50 --no-pager"
  err "   tail -50 $LOG_DIR/stderr.log"
  exit 2
fi
