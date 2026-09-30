#!/usr/bin/env bash
# Build EchoLocal and install it into /Applications for everyday use.
#
#   ./scripts/install-mac.sh
#
# The app is ad-hoc signed (no Apple Developer account needed). macOS ties
# Accessibility permission to the exact signature, so a grant made for an
# older build does not apply to a new one, even though the switch in System
# Settings still looks on. The script therefore clears EchoLocal's old
# Accessibility entry; allow it once more when the app asks.
set -euo pipefail

cd "$(dirname "$0")/.."
export CMAKE_POLICY_VERSION_MINIMUM="${CMAKE_POLICY_VERSION_MINIMUM:-3.5}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script installs the macOS app; run it on a Mac." >&2
  exit 1
fi

echo "==> Installing frontend dependencies"
bun install --frozen-lockfile

if [[ ! -s src-tauri/resources/models/silero_vad_v4.onnx ]]; then
  echo "==> Fetching the Silero VAD model"
  ./scripts/fetch-resources.sh || echo "   (continuing with the built-in voice detector)"
fi

echo "==> Building EchoLocal (release)"
bun run tauri build --bundles app

app="target/release/bundle/macos/EchoLocal.app"
if [[ ! -d "$app" ]]; then
  echo "Build finished but $app was not found." >&2
  exit 1
fi

echo "==> Installing to /Applications"
if pgrep -xq EchoLocal; then
  osascript -e 'quit app "EchoLocal"' || pkill -x EchoLocal || true
  sleep 1
fi
rm -rf /Applications/EchoLocal.app
cp -R "$app" /Applications/
# Built locally, so clear the quarantine flag Gatekeeper would otherwise check.
xattr -dr com.apple.quarantine /Applications/EchoLocal.app 2>/dev/null || true

echo "==> Clearing the old Accessibility permission (it belonged to the previous build)"
tccutil reset Accessibility app.echolocal.desktop >/dev/null 2>&1 || true

echo "==> Launching"
open /Applications/EchoLocal.app
echo "Done. EchoLocal is in your menu bar."
echo "If it asks for Accessibility access, click Allow (or turn EchoLocal on in"
echo "System Settings > Privacy & Security > Accessibility). The shortcut starts"
echo "working within a second, no restart needed."
