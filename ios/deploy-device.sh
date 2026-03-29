#!/usr/bin/env bash
# deploy-device.sh -- Build and deploy KerrApp to a USB-connected iPhone.
#
# Usage:
#   ./ios/deploy-device.sh              # Release build (default)
#   ./ios/deploy-device.sh --debug      # Debug build (includes symbols)
#
# One-time Xcode setup (only needed once):
#   1. Open Xcode -> Settings -> Accounts -> add your Apple ID
#   2. Open ios/KerrApp.xcodeproj, select your phone as run target
#   3. Click "Register Device" / "Fix Issue" when Xcode prompts
#   After that this script handles everything.
#
# Free Apple ID: app expires after 7 days — just re-run this script to renew.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT="$SCRIPT_DIR/KerrApp.xcodeproj"
SCHEME="KerrApp"
BUILD_DIR="$SCRIPT_DIR/build-device"
CONFIG="Release"

for arg in "$@"; do
  case $arg in
    --debug) CONFIG="Debug" ;;
  esac
done

if [ ! -d "$PROJECT" ]; then
  echo "ERROR: $PROJECT not found. Run ./ios/setup.sh first."
  exit 1
fi

# ── Step 1: Find connected device ─────────────────────────────────────────────

echo "-> Looking for connected iPhone/iPad..."

TMPJSON="$(mktemp /tmp/devicectl.XXXXXX.json)"
xcrun devicectl list devices --json-output "$TMPJSON" 2>/dev/null || true

UDID="$(python3 - "$TMPJSON" <<'EOF'
import sys, json
data = json.load(open(sys.argv[1]))
devices = data.get("result", {}).get("devices", [])
if devices:
    print(devices[0].get("hardwareProperties", {}).get("udid", ""))
EOF
)"
DEVICE_CTL_ID="$(python3 - "$TMPJSON" <<'EOF'
import sys, json
data = json.load(open(sys.argv[1]))
devices = data.get("result", {}).get("devices", [])
if devices:
    print(devices[0].get("identifier", ""))
EOF
)"
rm -f "$TMPJSON"

if [ -z "$UDID" ]; then
  echo ""
  echo "ERROR: No connected iPhone/iPad found."
  echo "  1. Connect your phone via USB cable"
  echo "  2. Unlock it and tap 'Trust This Computer' if prompted"
  echo "  3. Re-run this script"
  echo ""
  exit 1
fi

echo "   Device: $UDID"

# ── Step 2: Detect signing team from keychain ─────────────────────────────────

echo "-> Detecting signing team..."

TEAM_ID="$(security find-certificate -a -c "Apple Development" -p 2>/dev/null \
  | openssl x509 -noout -subject 2>/dev/null \
  | grep -oE 'OU=[A-Z0-9]{10}' | head -1 | cut -d= -f2 || true)"

if [ -z "$TEAM_ID" ]; then
  echo ""
  echo "ERROR: No Apple Development certificate found in keychain."
  echo "  Do the one-time Xcode setup described at the top of this script."
  echo ""
  exit 1
fi

echo "   Team: $TEAM_ID"

# ── Step 3: Build ─────────────────────────────────────────────────────────────

echo "-> Building ($CONFIG) for device..."

BUILD_CMD=(
  xcodebuild
  -project "$PROJECT"
  -scheme "$SCHEME"
  -destination "id=$UDID"
  -configuration "$CONFIG"
  -derivedDataPath "$BUILD_DIR"
  DEVELOPMENT_TEAM="$TEAM_ID"
  CODE_SIGN_STYLE=Automatic
  build
)

if command -v xcpretty >/dev/null 2>&1; then
  "${BUILD_CMD[@]}" | xcpretty
else
  "${BUILD_CMD[@]}"
fi

echo "Build succeeded."

# ── Step 4: Install ───────────────────────────────────────────────────────────

APP_PATH="$(find "$BUILD_DIR/Build/Products" -maxdepth 2 -name "KerrApp.app" | grep -v simulator | head -1)"

if [ -z "$APP_PATH" ]; then
  echo "ERROR: Could not find KerrApp.app under $BUILD_DIR"
  exit 1
fi

echo "-> Installing on device..."

xcrun devicectl device install app --device "$DEVICE_CTL_ID" "$APP_PATH"

echo ""
echo "Done. Open 'KerrApp' on your phone."
echo "Note: Free Apple ID builds expire in 7 days — re-run this script to renew."
echo ""
