# Kerr iOS - Quick Start

Get up and running with Kerr on iOS in 5 minutes.

## Prerequisites

- macOS with Xcode 15.0+
- Rust toolchain
- UniFFI bindgen

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install UniFFI
cargo install uniffi-bindgen --version 0.28
```

## Build Steps

### 1. Build Rust Library

```bash
cd kerr-ios
./build-ios.sh
```

Wait ~5-10 minutes for first build (downloading dependencies).

### 2. Create Xcode Project

Open Xcode and create new project:
- File → New → Project
- iOS → App
- Product Name: `KerrApp`
- Interface: SwiftUI
- Save in: `kerr/ios/KerrApp/`

### 3. Add Files to Project

Drag these folders into Xcode project navigator:
- `ios/KerrApp/*.swift` files (all 6 Swift files)
- `ios/Generated/kerr_ios.swift`

### 4. Create Bridging Header

1. File → New → File → Header File
2. Name: `KerrApp-Bridging-Header.h`
3. Content:
   ```objective-c
   #import "kerr_iosFFI.h"
   ```
4. Build Settings → Search "bridging" → Set path

### 5. Link Libraries

Build Settings → Library Search Paths:
```
$(PROJECT_DIR)/../Frameworks
```

Build Settings → Header Search Paths:
```
$(PROJECT_DIR)/../Generated
```

Build Phases → Link Binary with Libraries → Add:
- `ios/Frameworks/libkerr_ios.a`
- `ios/Frameworks/libkerr_ios_sim.a`

### 6. Add SwiftTerm

File → Add Package Dependencies:
```
https://github.com/migueldeicaza/SwiftTerm
```

### 7. Run!

⌘+R to build and run on simulator.

## Testing Connection

### Start Server
On your Mac/Linux:
```bash
cargo run --bin kerr serve
```

Copy the connection string (base64 encoded).

### Connect from iOS
1. Launch app
2. Paste connection string
3. Tap "Connect"
4. Browse files or use terminal!

## Common Issues

**Build Error**: `No such module 'kerr_ios'`
→ Ensure `kerr_ios.swift` is added to project and in target membership

**Build Error**: `Library not found`
→ Check Library Search Paths and that `.a` files exist in `ios/Frameworks/`

**Runtime Crash**:
→ Make sure you're using the right library (simulator vs device)

## Next Steps

See [SETUP.md](SETUP.md) for detailed configuration.

## What You Get

✅ File Browser - Browse remote files, download to iOS
✅ Terminal - Interactive shell via SwiftTerm
✅ P2P Connection - Direct encrypted QUIC
✅ NAT Traversal - Works through firewalls

Enjoy! 🚀
