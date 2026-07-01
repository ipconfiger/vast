#!/usr/bin/env bash
set -euo pipefail

# Generate a self-signed certificate for local TLS development/testing.
# Output: certs/cert.pem (certificate) and certs/key.pem (private key)
# Validity: 365 days, Common Name: localhost, IP SANs: 127.0.0.1, ::1

CERT_DIR="${1:-certs}"

mkdir -p "$CERT_DIR"

# Generate ECDSA P-256 key (modern, fast, widely supported)
openssl ecparam -genkey -name prime256v1 -out "$CERT_DIR/key.pem"

# Generate self-signed certificate with SANs
openssl req -new -x509 -key "$CERT_DIR/key.pem" -out "$CERT_DIR/cert.pem" -days 365 \
    -subj "/CN=localhost" \
    -addext "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1"

echo "✅ Self-signed certificate generated:"
echo "   cert:  $CERT_DIR/cert.pem"
echo "   key:   $CERT_DIR/key.pem"
echo ""
echo "Test with:  TLS_MODE=self-signed cargo run --release"
echo "Curl:       curl -k https://localhost:3443/api/health"
