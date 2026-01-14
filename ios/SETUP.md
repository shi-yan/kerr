# iOS Project Setup Guide

This guide walks you through setting up the Kerr iOS app from scratch.

## Step 1: Install Prerequisites

### Install Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Install UniFFI
```bash
cargo install uniffi-bindgen --version 0.28
```

### Install Xcode
Download Xcode 15.0+ from the Mac App Store or https://developer.apple.com/xcode/

## Step 2: Build Rust Library

```bash
cd kerr-ios
./build-ios.sh
```

This creates:
- `ios/Frameworks/libkerr_ios.a` - iOS device library
- `ios/Frameworks/libkerr_ios_sim.a` - iOS simulator library
- `ios/Generated/kerr_ios.swift` - Swift bindings
- `ios/Generated/kerr_iosFFI.h` - C headers

## Step 3: Create Xcode Project

### Option A: Using Xcode GUI

1. Open Xcode
2. File → New → Project
3. Choose: iOS → App
4. Settings:
   - Product Name: `KerrApp`
   - Team: Your development team
   - Organization Identifier: `com.yourname`
   - Interface: `SwiftUI`
   - Language: `Swift`
   - Storage: None
   - Include Tests: Optional
5. Save in: `kerr/ios/KerrApp/`

### Option B: Using Command Line (Advanced)

```bash
cd ios
# Create basic Xcode project structure
mkdir -p KerrApp.xcodeproj
# You'll need to create project.pbxproj manually or use a template
```

## Step 4: Add Source Files

1. In Xcode, right-click on the `KerrApp` folder
2. Select "Add Files to KerrApp..."
3. Add these files from `ios/KerrApp/`:
   - `KerrApp.swift`
   - `ContentView.swift`
   - `ConnectionManager.swift`
   - `ConnectionView.swift`
   - `FileBrowserView.swift`
   - `TerminalView.swift`

## Step 5: Add Generated FFI Files

1. Right-click on `KerrApp` folder
2. "Add Files to KerrApp..."
3. Navigate to `ios/Generated/`
4. Select `kerr_ios.swift`
5. Make sure "Copy items if needed" is UNCHECKED
6. Add to target: KerrApp

## Step 6: Create Bridging Header

1. File → New → File
2. Choose: iOS → Header File
3. Name: `KerrApp-Bridging-Header.h`
4. Add content:
   ```objective-c
   #ifndef KerrApp_Bridging_Header_h
   #define KerrApp_Bridging_Header_h

   #import "kerr_iosFFI.h"

   #endif
   ```

5. Set in Build Settings:
   - Search for "bridging"
   - Set "Objective-C Bridging Header" to: `$(PROJECT_DIR)/KerrApp-Bridging-Header.h`

## Step 7: Configure Build Settings

### Add Library Search Paths

1. Select project in navigator
2. Select KerrApp target
3. Build Settings tab
4. Search for "Library Search Paths"
5. Add: `$(PROJECT_DIR)/../Frameworks`

### Add Header Search Paths

1. Search for "Header Search Paths"
2. Add: `$(PROJECT_DIR)/../Generated`

### Link Static Libraries

1. Go to Build Phases tab
2. Expand "Link Binary With Libraries"
3. Click `+`
4. Click "Add Other..." → "Add Files..."
5. Add both:
   - `ios/Frameworks/libkerr_ios.a`
   - `ios/Frameworks/libkerr_ios_sim.a`

**Important**: Configure per-architecture linking:
1. Select `libkerr_ios.a`
2. Set "iOS Deployment Target" → Only Device
3. Select `libkerr_ios_sim.a`
4. Set for Simulator only

Or use build settings:
```
OTHER_LDFLAGS[sdk=iphoneos*] = -L$(PROJECT_DIR)/../Frameworks -lkerr_ios
OTHER_LDFLAGS[sdk=iphonesimulator*] = -L$(PROJECT_DIR)/../Frameworks -lkerr_ios_sim
```

## Step 8: Add SwiftTerm Package

1. File → Add Package Dependencies
2. Enter URL: `https://github.com/migueldeicaza/SwiftTerm`
3. Dependency Rule: Branch → `main` (or specific version)
4. Add to target: KerrApp

## Step 9: Configure App Settings

### Update Info.plist

The provided `Info.plist` includes:
- Camera usage description (for QR scanning)
- Photo library description (for saving files)

### Set Bundle Identifier

1. Select project → KerrApp target
2. General tab
3. Identity section
4. Bundle Identifier: `com.yourname.KerrApp`

### Configure Signing

1. General tab → Signing & Capabilities
2. Select your Team
3. Xcode will automatically manage signing

## Step 10: Build and Run

### For Simulator

1. Select a simulator (e.g., "iPhone 15 Pro")
2. Press ⌘+R to build and run
3. Uses `libkerr_ios_sim.a`

### For Device

1. Connect your iPhone/iPad via USB
2. Trust computer on device if prompted
3. Select your device in Xcode
4. Press ⌘+R
5. Uses `libkerr_ios.a`

**First time**: You may need to trust the developer certificate on device:
- Settings → General → VPN & Device Management → Trust

## Step 11: Test the App

### Get a Connection String

On your server (Linux/Mac):
```bash
kerr serve --register myserver
```

This will output a connection string like:
```
H4sIAAAAAAAA/6SRPW/DMAyE9/0K...
```

### Connect from iOS

1. Launch Kerr app on iOS
2. Paste the connection string
3. Tap "Connect"
4. Navigate to Files tab to browse remote filesystem
5. Navigate to Terminal tab for shell access

## Troubleshooting

### "No such module 'kerr_ios'"

**Solution**:
- Verify `ios/Generated/kerr_ios.swift` is added to Xcode project
- Check file is in target membership (right-click → show File Inspector)
- Clean build folder (Product → Clean Build Folder)

### "Undefined symbol: _uniffi_kerr_ios_..."

**Solution**:
- Ensure static libraries are linked in Build Phases
- Check Library Search Paths includes `$(PROJECT_DIR)/../Frameworks`
- Verify correct library for target (device vs simulator)

### "Library not found for -lkerr_ios"

**Solution**:
- Run `./build-ios.sh` in `kerr-ios` directory
- Check that `ios/Frameworks/` contains `.a` files
- Verify Library Search Paths is correct

### Build succeeds but crashes on launch

**Solution**:
- Check you're using correct library:
  - Simulator: `libkerr_ios_sim.a`
  - Device: `libkerr_ios.a`
- Verify architectures match (see Build Settings → Architectures)

### Connection fails

**Solution**:
- Check connection string is valid and not expired
- Verify server is running: `kerr serve`
- Test network connectivity
- Check firewall settings

### SwiftTerm not found

**Solution**:
- Add package dependency: File → Add Package Dependencies
- URL: `https://github.com/migueldeicaza/SwiftTerm`
- Clean and rebuild

## Development Workflow

### Making Rust Changes

1. Edit code in `kerr-ios/src/`
2. Run `./build-ios.sh` in `kerr-ios/`
3. In Xcode: Product → Clean Build Folder (⌘+Shift+K)
4. Build and run (⌘+R)

### Making Swift Changes

1. Edit `.swift` files in Xcode
2. Build and run (⌘+R)
3. No need to rebuild Rust

### Updating UDL Interface

If you change `kerr_ios.udl`:
1. Edit `kerr-ios/src/kerr_ios.udl`
2. Update Rust implementation in `kerr-ios/src/`
3. Run `./build-ios.sh`
4. Check generated `ios/Generated/kerr_ios.swift` for new APIs
5. Update Swift code to use new APIs

## Next Steps

- Implement QR code scanner for connection strings
- Integrate real SwiftTerm component (commented out in TerminalView.swift)
- Add file upload functionality
- Add connection history
- Implement background connection keep-alive
- Add biometric authentication for saved connections

## Resources

- [UniFFI User Guide](https://mozilla.github.io/uniffi-rs/)
- [SwiftTerm Documentation](https://migueldeicaza.github.io/SwiftTerm/)
- [Iroh Documentation](https://docs.iroh.computer/)
- [SwiftUI Tutorials](https://developer.apple.com/tutorials/swiftui)
