#!/bin/sh
set -eu

REPOSITORY="${RETAKE_REPOSITORY:-mj-hd/retake}"
INSTALL_DIR="${RETAKE_INSTALL_DIR:-$HOME/.local/share/retake}"
BIN_DIR="${RETAKE_BIN_DIR:-$HOME/.local/bin}"
VERSION="${RETAKE_VERSION:-latest}"

case "$(uname -s)" in
  Darwin) os="macos" ;;
  Linux) os="linux" ;;
  *) echo "retake: unsupported operating system: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "retake: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

command -v curl >/dev/null 2>&1 || {
  echo "retake: curl is required" >&2
  exit 1
}
command -v node >/dev/null 2>&1 || {
  echo "retake: Node.js is required" >&2
  exit 1
}
command -v npm >/dev/null 2>&1 || {
  echo "retake: npm is required" >&2
  exit 1
}
node_major="$(node -p 'Number(process.versions.node.split(".")[0])')"
if [ "$node_major" -lt 20 ]; then
  echo "retake: Node.js 20 or later is required (found $(node --version))" >&2
  exit 1
fi

asset="retake-${os}-${arch}.tar.gz"
if [ "$VERSION" = "latest" ]; then
  base_url="https://github.com/${REPOSITORY}/releases/latest/download"
else
  base_url="https://github.com/${REPOSITORY}/releases/download/${VERSION}"
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

echo "Downloading ${asset}"
curl --fail --location --proto '=https' --tlsv1.2 \
  --output "$tmp_dir/$asset" "$base_url/$asset"
curl --fail --location --proto '=https' --tlsv1.2 \
  --output "$tmp_dir/$asset.sha256" "$base_url/$asset.sha256"

(
  cd "$tmp_dir"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum --check "$asset.sha256"
  else
    shasum -a 256 --check "$asset.sha256"
  fi
)

tar -xzf "$tmp_dir/$asset" -C "$tmp_dir"
rm -rf "$INSTALL_DIR.new"
mkdir -p "$(dirname "$INSTALL_DIR")"
mv "$tmp_dir/retake" "$INSTALL_DIR.new"

echo "Installing renderer dependencies"
(
  cd "$INSTALL_DIR.new/workers/web-capture"
  npm ci --omit=dev --ignore-scripts --legacy-peer-deps
  if [ "${RETAKE_SKIP_BROWSER:-0}" != "1" ]; then
    npx playwright install chromium
  fi
)

rm -rf "$INSTALL_DIR"
mv "$INSTALL_DIR.new" "$INSTALL_DIR"
mkdir -p "$BIN_DIR"
ln -sfn "$INSTALL_DIR/bin/retake" "$BIN_DIR/retake"

echo "retake installed at $BIN_DIR/retake"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "Add $BIN_DIR to PATH before registering the MCP server" ;;
esac
