#!/bin/bash
set -e

# iOS Build Script for kerr-ios
# This script builds the Rust library for iOS targets and generates Swift bindings

echo "🔨 Building kerr-ios for iOS..."

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Error: cargo is not installed"
    exit 1
fi

UNIFFI_BINDGEN="cargo run --features=uniffi/cli --"

# Add iOS targets if not already added
echo "📱 Adding iOS targets..."
rustup target add aarch64-apple-ios      # iOS devices (ARM64)
rustup target add aarch64-apple-ios-sim  # iOS Simulator (ARM64 - M1/M2 Macs)
rustup target add x86_64-apple-ios       # iOS Simulator (x86_64 - Intel Macs)

# Create output directories
mkdir -p ../ios/Generated
mkdir -p ../ios/Frameworks

# Build for iOS device (ARM64)
echo "🔨 Building for iOS device (ARM64)..."
cargo build --release --target aarch64-apple-ios

# Build for iOS Simulator (ARM64 - M1/M2 Macs)
echo "🔨 Building for iOS Simulator (ARM64)..."
cargo build --release --target aarch64-apple-ios-sim

# Build for iOS Simulator (x86_64 - Intel Macs)
echo "🔨 Building for iOS Simulator (x86_64)..."
cargo build --release --target x86_64-apple-ios

# Create universal library for simulators
echo "🔗 Creating universal simulator library..."
lipo -create \
    target/aarch64-apple-ios-sim/release/libkerr_ios.a \
    target/x86_64-apple-ios/release/libkerr_ios.a \
    -output ../ios/Frameworks/libkerr_ios_sim.a

# Copy device library
echo "📦 Copying device library..."
cp target/aarch64-apple-ios/release/libkerr_ios.a ../ios/Frameworks/libkerr_ios.a

# Generate Swift bindings
echo "🔧 Generating Swift bindings..."
$UNIFFI_BINDGEN generate src/kerr_ios.udl --language swift --out-dir ../ios/Generated

echo "✅ Build complete!"
echo ""
echo "📂 Output files:"
echo "  - iOS Device library: ios/Frameworks/libkerr_ios.a"
echo "  - iOS Simulator library: ios/Frameworks/libkerr_ios_sim.a"
echo "  - Swift bindings: ios/Generated/kerr_ios.swift"
echo "  - C headers: ios/Generated/kerr_iosFFI.h"
echo ""
echo "🎯 Next steps:"
echo "  1. Open ios/KerrApp/KerrApp.xcodeproj in Xcode"
echo "  2. Add the generated files to your Xcode project"
echo "  3. Build and run on simulator or device"
