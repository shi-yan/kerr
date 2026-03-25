#!/bin/bash
set -e
cd "$(dirname "$0")"

BUNDLE_ID="com.kerr.app"
APP=$(find ~/Library/Developer/Xcode/DerivedData -name 'KerrApp.app' -path '*/Debug-iphonesimulator/*' ! -path '*/Index.noindex/*' ! -path '*/index/*' | head -1)

if [ -z "$APP" ]; then
    echo "❌ KerrApp.app not found. Run build-sim.sh first."
    exit 1
fi

# Pick a booted simulator, or boot one if none is running
BOOTED=$(xcrun simctl list devices booted -j | python3 -c "
import json, sys
d = json.load(sys.stdin)
for runtime, devices in d['devices'].items():
    for dev in devices:
        if dev['state'] == 'Booted':
            print(dev['udid'])
            sys.exit(0)
" 2>/dev/null || true)

if [ -z "$BOOTED" ]; then
    echo "📱 No booted simulator found. Booting iPhone 16..."
    UDID=$(xcrun simctl list devices available -j | python3 -c "
import json, sys
d = json.load(sys.stdin)
# Prefer exact 'iPhone 16' match first
for runtime, devices in d['devices'].items():
    if 'iOS' not in runtime:
        continue
    for dev in devices:
        if dev['name'] == 'iPhone 16':
            print(dev['udid'])
            sys.exit(0)
# Fallback: any iPhone 16 variant
for runtime, devices in d['devices'].items():
    if 'iOS' not in runtime:
        continue
    for dev in devices:
        if 'iPhone 16' in dev['name']:
            print(dev['udid'])
            sys.exit(0)
# Last resort: first available iPhone
for runtime, devices in d['devices'].items():
    if 'iOS' not in runtime:
        continue
    for dev in devices:
        if 'iPhone' in dev['name']:
            print(dev['udid'])
            sys.exit(0)
")
    if [ -z "$UDID" ]; then
        echo "❌ No iPhone simulator found."
        exit 1
    fi
    xcrun simctl boot "$UDID"
    open -a Simulator
    echo "⏳ Waiting for simulator to boot..."
    xcrun simctl bootstatus "$UDID" -b
    BOOTED="$UDID"
fi

echo "📲 Installing $APP..."
xcrun simctl install "$BOOTED" "$APP"

echo "🚀 Launching $BUNDLE_ID..."
xcrun simctl launch --console-pty "$BOOTED" "$BUNDLE_ID"
