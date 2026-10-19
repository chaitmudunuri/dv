#!/bin/sh
# Install dv (github.com/chaitmudunuri/dv)
#   curl -fsSL https://raw.githubusercontent.com/chaitmudunuri/dv/master/install.sh | sh
set -e

REPO=chaitmudunuri/dv
DEST=${DV_INSTALL_DIR:-"$HOME/.local/bin"}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    else
        wget -qO "$2" "$1"
    fi
}

# detect OS
case "$(uname -s)" in
    Darwin) os=darwin ;;
    Linux) os=linux ;;
    *) echo "error: unsupported OS '$(uname -s)' (windows: grab dv-windows-x86_64.zip from the releases page)" >&2; exit 1 ;;
esac

# detect arch
case "$(uname -m)" in
    arm64|aarch64) arch=arm64 ;;
    x86_64|amd64) arch=x86_64 ;;
    *) echo "error: unsupported arch '$(uname -m)'" >&2; exit 1 ;;
esac

asset="dv-${os}-${arch}.tar.gz"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

download "https://api.github.com/repos/${REPO}/releases/latest" "$tmp/rel.json"
url=$(sed -n "s/.*browser_download_url.*\(https[^\"]*${asset}\).*/\1/p" "$tmp/rel.json" | head -1)
if [ -z "$url" ]; then
    echo "error: no release asset for ${asset}" >&2
    exit 1
fi

echo "fetching ${url}"
download "$url" "$tmp/${asset}"
tar -C "$tmp" -xzf "$tmp/${asset}"

mkdir -p "$DEST"
mv "$tmp/dv" "$DEST/dv"
chmod +x "$DEST/dv"

echo "installed: $DEST/dv"
"$DEST/dv" --version

case ":$PATH:" in
    *":$DEST:"*) ;;
    *) echo "note: $DEST is not in PATH — add: export PATH=\"\$PATH:$DEST\"" ;;
esac
