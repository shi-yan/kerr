#!/bin/bash
set -e

# Change to the script's directory so cargo uses kerr-ios/Cargo.toml (not the root workspace)
cd "$(dirname "$0")"

# iOS Build Script for kerr-ios
# Builds the Rust library for iOS targets, generates Swift bindings,
# and packages everything into an XCFramework.

echo "🔨 Building kerr-ios for iOS..."

# Check dependencies
if ! command -v cargo &> /dev/null; then
    echo "❌ Error: cargo is not installed"
    exit 1
fi

if ! command -v xcodebuild &> /dev/null; then
    echo "❌ Error: xcodebuild is not installed (Xcode required)"
    exit 1
fi

UNIFFI_BINDGEN="cargo run --bin uniffi-bindgen --"

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
cp target/aarch64-apple-ios/release/libkerr_ios.a ../ios/Frameworks/libkerr_ios.a

# Generate Swift bindings (also produces kerr_iosFFI.h needed by XCFramework)
echo "🔧 Generating Swift bindings..."
$UNIFFI_BINDGEN generate src/kerr_ios.udl --language swift --out-dir ../ios/Generated

# Package into an XCFramework so Xcode automatically picks the right slice
# (device vs simulator) without duplicate-symbol linker errors.
#
# We pass only kerr_iosFFI.h — NOT the modulemap or the generated .swift file.
# Including the modulemap would create a clang module inside the XCFramework
# that conflicts with the bridging header in the consuming app, causing
# #pragma once to suppress the second inclusion and hiding all C declarations.
echo "📦 Creating XCFramework..."
rm -rf ../ios/Frameworks/kerr_ios.xcframework

XCFW_HEADERS=$(mktemp -d)
# Use a placeholder header — do NOT include kerr_iosFFI.h here.
# If kerr_iosFFI.h is in the XCFramework headers, Xcode adds it via
# HEADER_SEARCH_PATHS which pre-defines all UNIFFI_FFIDEF_* macros.
# When the bridging header then includes the same file from Generated/,
# every function declaration is guarded by those already-set macros and
# silently skipped — Swift never sees any of the FFI functions.
# The bridging header (SWIFT_OBJC_BRIDGING_HEADER in project.yml) handles
# exposing kerr_iosFFI.h to Swift cleanly.
echo "// kerr-ios static library" > "$XCFW_HEADERS/kerr_ios.h"

xcodebuild -create-xcframework \
    -library ../ios/Frameworks/libkerr_ios.a \
    -headers "$XCFW_HEADERS" \
    -library ../ios/Frameworks/libkerr_ios_sim.a \
    -headers "$XCFW_HEADERS" \
    -output ../ios/Frameworks/kerr_ios.xcframework

rm -rf "$XCFW_HEADERS"

echo "✅ Build complete!"
echo ""
echo "📂 Output files:"
echo "  - XCFramework:     ios/Frameworks/kerr_ios.xcframework"
echo "  - Swift bindings:  ios/Generated/kerr_ios.swift"
echo "  - C header:        ios/Generated/kerr_iosFFI.h"
