#!/bin/bash
set -e
cd "$(dirname "$0")"

if [ ! -d "KerrApp.xcodeproj" ]; then
    echo "❌ KerrApp.xcodeproj not found. Run setup.sh first."
    exit 1
fi

echo "🔨 Building KerrApp for iOS Simulator..."

xcodebuild \
    -project KerrApp.xcodeproj \
    -scheme KerrApp \
    -sdk iphonesimulator \
    -destination 'generic/platform=iOS Simulator' \
    -configuration Debug \
    build

echo "✅ Simulator build complete!"
echo ""
echo "💡 To install and launch on a booted simulator:"
echo "   APP=\$(find ~/Library/Developer/Xcode/DerivedData -name 'KerrApp.app' -path '*/Debug-iphonesimulator/*' | head -1)"
echo "   xcrun simctl install booted \"\$APP\""
echo "   xcrun simctl launch booted com.kerr.app"
