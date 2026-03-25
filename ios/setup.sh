#!/bin/bash
set -e
cd "$(dirname "$0")"

echo "🚀 Setting up KerrApp..."

# Build Rust library, generate Swift bindings, and create XCFramework
cd ../kerr-ios
./build-ios.sh
cd ../ios

# Generate Xcode project from project.yml
if ! command -v xcodegen &> /dev/null; then
    echo "❌ xcodegen not found. Install with: brew install xcodegen"
    exit 1
fi

echo "🛠  Generating Xcode project..."
xcodegen generate

echo ""
echo "✅ Done! Open ios/KerrApp.xcodeproj in Xcode, or run:"
echo "   ios/build-sim.sh    — build for iOS Simulator"
