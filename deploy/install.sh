#!/usr/bin/env bash
set -euo pipefail

# ───────────────────────────────────────────────
# IM Server deployment script
# ───────────────────────────────────────────────
# Usage:
#   sudo ./install.sh /path/to/im-server-binary
#
# The binary is the release build from:
#   cargo build --release  →  target/release/im-server
# ───────────────────────────────────────────────

BINARY_SRC="${1:?Usage: $0 <path-to-im-server-binary>}"
BINARY_DST="/opt/im-server/im-server"
ENV_FILE="/opt/im-server/.env"
SERVICE_FILE="/etc/systemd/system/im-server.service"

# ---- Pre-flight checks ----
if [[ $EUID -ne 0 ]]; then
    echo "ERROR: This script must be run as root (sudo)." >&2
    exit 1
fi

if [[ ! -f "$BINARY_SRC" ]]; then
    echo "ERROR: Binary not found at '$BINARY_SRC'." >&2
    exit 1
fi

# ---- Create directories ----
install -d -m 755 /opt/im-server
install -d -m 755 /var/log/im-server

# ---- Copy binary ----
echo "→ Installing binary ..."
install -m 755 "$BINARY_SRC" "$BINARY_DST"
echo "  ✓ $BINARY_DST"

# ---- Write .env (preserve existing) ----
if [[ -f "$ENV_FILE" ]]; then
    echo "→ .env already exists — skipping (remove $ENV_FILE to regenerate)."
else
    echo "→ Writing .env ..."
    cat > "$ENV_FILE" <<-EOF
JWT_SECRET=change-me-to-a-random-64-char-string
INVITE_CODE=change-me-to-a-random-invite-code
SERVER_PORT=3000
UPLOAD_MAX_SIZE=52428800
TLS_MODE=none
EOF
    chmod 600 "$ENV_FILE"
    echo "  ✓ $ENV_FILE (600) — PLEASE UPDATE THE SECRETS."
fi

# ---- Install systemd unit ----
echo "→ Installing systemd unit ..."
install -m 644 /home/alex/Projects/vast/deploy/im-server.service "$SERVICE_FILE"
echo "  ✓ $SERVICE_FILE"

# ---- Create system user (idempotent) ----
if ! id im-server &>/dev/null; then
    echo "→ Creating 'im-server' system user ..."
    useradd --system --no-create-home --shell /usr/sbin/nologin im-server
    echo "  ✓ user created"
fi

# ---- Set ownership ----
chown -R im-server:im-server /opt/im-server /var/log/im-server

# ---- Reload systemd ----
systemctl daemon-reload

# ---- Enable & start ----
echo "→ Enabling service (will start on boot) ..."
systemctl enable im-server.service

echo ""
echo "✅ Installation complete!"
echo ""
echo "   Next steps:"
echo "     1. Edit $ENV_FILE with real secrets."
echo "     2. Start the service:  sudo systemctl start im-server"
echo "     3. Check status:       sudo systemctl status im-server"
echo "     4. View logs:          sudo journalctl -u im-server -f"
