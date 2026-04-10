#!/bin/bash
# 生成自签名 TLS 证书（开发/内网使用）
# 使用: ./tls/gen-cert.sh
# 输出: tls/cert.pem, tls/key.pem

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CERT="${SCRIPT_DIR}/cert.pem"
KEY="${SCRIPT_DIR}/key.pem"
DAYS=365

# 获取本机 IP
LOCAL_IP=$(hostname -I 2>/dev/null | awk '{print $1}' || echo "192.168.1.100")

echo "生成自签名证书..."
echo "  有效期: ${DAYS} 天"
echo "  IP SAN: ${LOCAL_IP}"

openssl req -x509 -newkey rsa:2048 \
    -keyout "${KEY}" \
    -out "${CERT}" \
    -days "${DAYS}" \
    -nodes \
    -subj "/CN=FileTransfer" \
    -addext "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:${LOCAL_IP}"

echo ""
echo "证书已生成:"
echo "  ${CERT}"
echo "  ${KEY}"
echo ""
echo "启动命令:"
echo "  transfer-app -p /your/path --tls-cert ${CERT} --tls-key ${KEY}"
