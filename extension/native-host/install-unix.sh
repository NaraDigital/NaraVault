#!/usr/bin/env bash
# Registers the NaraVault native-messaging host on macOS / Linux (current user).
#
#   ./install-unix.sh <extension-id> [chrome|chromium|edge|brave|firefox]
#
# For Chromium browsers <extension-id> is the ID from chrome://extensions.
# For Firefox pass the add-on id (e.g. naravault@naravault.dev) instead.

set -euo pipefail

EXT_ID="${1:-}"
BROWSER="${2:-chrome}"
if [[ -z "$EXT_ID" ]]; then
  echo "usage: $0 <extension-id> [chrome|chromium|edge|brave|firefox]" >&2
  exit 1
fi

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Locate the host binary (release preferred).
BIN=""
for c in \
  "$HERE/../../src-tauri/target/release/naravault-host" \
  "$HERE/../../src-tauri/target/debug/naravault-host"; do
  if [[ -x "$c" ]]; then BIN="$(cd "$(dirname "$c")" && pwd)/$(basename "$c")"; break; fi
done
if [[ -z "$BIN" ]]; then
  echo "naravault-host not found. Build it first: (cd src-tauri && cargo build --release)" >&2
  exit 1
fi

OS="$(uname -s)"
case "$OS" in
  Darwin) ROOT="$HOME/Library/Application Support" ;;
  *)      ROOT="$HOME/.config" ;;
esac

# Target directory differs per browser.
case "$BROWSER" in
  chrome)   DIR="$ROOT/Google/Chrome/NativeMessagingHosts" ;;
  chromium) DIR="$ROOT/chromium/NativeMessagingHosts" ;;
  edge)     DIR="$ROOT/Microsoft Edge/NativeMessagingHosts" ;;
  brave)    DIR="$ROOT/BraveSoftware/Brave-Browser/NativeMessagingHosts" ;;
  firefox)
    if [[ "$OS" == "Darwin" ]]; then DIR="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts";
    else DIR="$HOME/.mozilla/native-messaging-hosts"; fi ;;
  *) echo "unknown browser: $BROWSER" >&2; exit 1 ;;
esac

mkdir -p "$DIR"
MANIFEST="$DIR/com.naravault.host.json"

if [[ "$BROWSER" == "firefox" ]]; then
  cat > "$MANIFEST" <<EOF
{
  "name": "com.naravault.host",
  "description": "NaraVault native messaging host",
  "path": "$BIN",
  "type": "stdio",
  "allowed_extensions": ["$EXT_ID"]
}
EOF
else
  cat > "$MANIFEST" <<EOF
{
  "name": "com.naravault.host",
  "description": "NaraVault native messaging host",
  "path": "$BIN",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://$EXT_ID/"]
}
EOF
fi

chmod 600 "$MANIFEST"
echo "Installed NaraVault host for $BROWSER"
echo "  host binary : $BIN"
echo "  manifest    : $MANIFEST"
echo "  extension   : $EXT_ID"
echo "Restart the browser, then reload the extension."
