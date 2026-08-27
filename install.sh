#!/bin/sh
# transfer-app 一键安装脚本 (Linux / macOS)
#
# 用法:
#   curl -fsSL https://raw.githubusercontent.com/zzhtl/transfer-app/main/install.sh | sh
#
# 可选环境变量:
#   TRANSFER_VERSION      安装指定版本, 如 v0.3.0 (默认: 最新 release)
#   TRANSFER_INSTALL_DIR  安装目录 (默认: /usr/local/bin)
set -eu

REPO="zzhtl/transfer-app"
BIN="transfer-app"
VERSION="${TRANSFER_VERSION:-latest}"
INSTALL_DIR="${TRANSFER_INSTALL_DIR:-/usr/local/bin}"

say() { printf '%s\n' "$*"; }
err() { printf 'install: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || err "需要 curl"
command -v tar >/dev/null 2>&1 || err "需要 tar"

# 识别平台
os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Linux)  suffix="unknown-linux-musl" ;;
    Darwin) suffix="apple-darwin" ;;
    *) err "不支持的系统: $os (Windows 请用 'cargo install' 或到 Releases 页面手动下载)" ;;
esac

case "$arch" in
    x86_64|amd64)  arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *) err "不支持的架构: $arch" ;;
esac

target="${arch}-${suffix}"
asset="${BIN}-${target}.tar.gz"

if [ "$VERSION" = "latest" ]; then
    url="https://github.com/${REPO}/releases/latest/download/${asset}"
else
    url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "下载 ${url}"
curl -fL --proto '=https' --tlsv1.2 --retry 3 -o "${tmp}/${asset}" "$url" \
    || err "下载失败, 请确认版本存在: https://github.com/${REPO}/releases"
curl -fsSL --proto '=https' --tlsv1.2 --retry 3 -o "${tmp}/${asset}.sha256" "${url}.sha256" \
    || err "下载校验文件失败"

say "校验 SHA256"
(
    cd "$tmp"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c "${asset}.sha256" >/dev/null
    else
        shasum -a 256 -c "${asset}.sha256" >/dev/null
    fi
) || err "SHA256 校验失败"

tar -xzf "${tmp}/${asset}" -C "$tmp"
[ -f "${tmp}/${BIN}" ] || err "压缩包内未找到 ${BIN}"

# 安装 (目录不可写时尝试 sudo)
mkdir -p "$INSTALL_DIR" 2>/dev/null || true
if [ -d "$INSTALL_DIR" ] && [ -w "$INSTALL_DIR" ]; then
    SUDO=""
elif command -v sudo >/dev/null 2>&1; then
    say "安装到 ${INSTALL_DIR} 需要 sudo 权限"
    SUDO="sudo"
else
    err "${INSTALL_DIR} 不可写且没有 sudo, 可用 TRANSFER_INSTALL_DIR 指定其他目录"
fi

$SUDO mkdir -p "$INSTALL_DIR"
$SUDO install -m 755 "${tmp}/${BIN}" "${INSTALL_DIR}/${BIN}"

say ""
say "已安装: $("${INSTALL_DIR}/${BIN}" --version)"

case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *) say "提示: ${INSTALL_DIR} 不在 PATH 中, 请自行加入 shell 配置" ;;
esac

say ""
say "快速开始:"
say "  ${BIN} --path /path/to/share"
say "  然后用浏览器访问终端打印的地址"
