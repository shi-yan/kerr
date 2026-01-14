# Kerr iOS Architecture

## Overview

The Kerr iOS app is a native Swift application that uses a Rust core library for P2P networking via the Iroh protocol. The architecture is divided into three main layers:

```
┌─────────────────────────────────────────────────────────┐
│                    Presentation Layer                    │
│                  (SwiftUI + SwiftTerm)                   │
├─────────────────────────────────────────────────────────┤
│  ContentView          │ ConnectionView                   │
│  FileBrowserView      │ TerminalView                     │
│  ConnectionManager    │ FileActionsView                  │
└─────────────┬───────────────────────────────────────────┘
              │ Swift/Rust Bridge (UniFFI)
┌─────────────▼───────────────────────────────────────────┐
│                 Business Logic Layer                     │
│                    (Rust - kerr-ios)                     │
├─────────────────────────────────────────────────────────┤
│  Endpoint        │ Session        │ FileBrowser         │
│  ShellSession    │ Types          │ Error Handling      │
└─────────────┬───────────────────────────────────────────┘
              │ Direct API Calls
┌─────────────▼───────────────────────────────────────────┐
│                  Network/Core Layer                      │
│                   (Iroh + Tokio)                         │
├─────────────────────────────────────────────────────────┤
│  iroh::Endpoint              │ QUIC Protocol            │
│  iroh::endpoint::Connection  │ NAT Traversal            │
│  tokio Runtime               │ Stream Multiplexing      │
└─────────────────────────────────────────────────────────┘
```

## Components

### Swift Layer (Presentation)

#### 1. **ConnectionManager** (`ConnectionManager.swift`)
- **Purpose**: Central state manager for connection lifecycle
- **Responsibilities**:
  - Create and manage Iroh endpoint
  - Connect/disconnect from remote servers
  - Maintain connection state
  - Provide access to FileBrowser and ShellSession
- **State**:
  - `isConnected: Bool` - Current connection status
  - `connectionStatus: String` - Human-readable status
  - `errorMessage: String?` - Last error if any

#### 2. **ConnectionView** (`ConnectionView.swift`)
- **Purpose**: UI for establishing connections
- **Features**:
  - Text field for connection string input
  - Paste from clipboard
  - QR code scanner (placeholder)
  - Connection status display
- **User Flow**: Paste → Connect → Navigate to main view

#### 3. **FileBrowserView** (`FileBrowserView.swift`)
- **Purpose**: Navigate remote filesystem
- **Features**:
  - Directory listing with icons
  - File/folder metadata display
  - Navigation breadcrumbs
  - File download capability
- **User Flow**: List dir → Tap folder → Navigate deeper
                 List dir → Tap file → Download/view

#### 4. **TerminalView** (`TerminalView.swift`)
- **Purpose**: Interactive shell interface
- **Features**:
  - Shell output display (SwiftTerm integration)
  - Input field for commands
  - Terminal resize handling
  - Shell lifecycle management
- **User Flow**: Start shell → Send commands → View output

### Rust FFI Layer (Business Logic)

#### 1. **Endpoint** (`endpoint.rs`)
- **Purpose**: P2P endpoint management
- **Key Functions**:
  ```rust
  pub async fn new() -> Result<Arc<Self>>
  pub fn connect(connection_string: String) -> Result<Arc<Session>>
  pub fn connection_string() -> Result<String>
  ```
- **Internal State**: `iroh::endpoint::Endpoint`

#### 2. **Session** (`session.rs`)
- **Purpose**: Active connection session manager
- **Key Functions**:
  ```rust
  pub fn file_browser() -> Result<Arc<FileBrowser>>
  pub fn start_shell(callback: Box<dyn ShellCallback>) -> Result<Arc<ShellSession>>
  pub fn disconnect()
  pub fn is_connected() -> bool
  ```
- **Internal State**:
  - `conn: Arc<iroh::endpoint::Connection>`
  - `file_browser: Option<Arc<FileBrowser>>`
  - `shell_session: Option<Arc<ShellSession>>`

#### 3. **FileBrowser** (`file_browser.rs`)
- **Purpose**: Remote filesystem operations
- **Protocol**: Uses `ClientMessage` and `ServerMessage` envelopes
- **Key Functions**:
  ```rust
  pub fn list_dir(path: String) -> Result<Vec<FileEntry>>
  pub fn metadata(path: String) -> Result<FileMetadata>
  pub fn download_file(path: String) -> Result<Vec<u8>>
  pub fn upload_file(path: String, data: Vec<u8>) -> Result<()>
  pub fn delete(path: String) -> Result<()>
  pub fn exists(path: String) -> Result<bool>
  ```
- **Internal State**:
  - `send: Arc<Mutex<iroh::endpoint::SendStream>>`
  - `recv: Arc<Mutex<iroh::endpoint::RecvStream>>`
  - `session_id: String`

#### 4. **ShellSession** (`shell.rs`)
- **Purpose**: Interactive shell management
- **Protocol**: Bidirectional stream with envelope multiplexing
- **Key Functions**:
  ```rust
  pub fn send_input(data: String) -> Result<()>
  pub fn resize(cols: u16, rows: u16) -> Result<()>
  pub fn close()
  ```
- **Callback Interface** (implemented in Swift):
  ```rust
  trait ShellCallback {
      fn on_output(data: String);
      fn on_error(message: String);
      fn on_close();
  }
  ```
- **Internal State**:
  - Background tokio task for receiving output
  - Stream send/recv handles
  - Session ID for multiplexing

#### 5. **Types** (`types.rs`)
- **Purpose**: Shared data structures between Swift and Rust
- **Key Types**:
  ```rust
  pub enum KerrError
  pub struct FileEntry
  pub struct FileMetadata
  ```

### Protocol Layer

#### Message Envelope Structure
```rust
MessageEnvelope {
    session_id: String,      // Multiplexing identifier
    payload: MessagePayload, // Client or Server message
}
```

#### Client Messages (Swift → Rust → Server)
- `Hello { session_type }` - Initialize session
- `Input { data }` - Send shell input
- `Resize { cols, rows }` - Resize terminal
- `ListDir { path }` - List directory
- `ReadFile { path }` - Download file
- `WriteFile { path, data }` - Upload file
- `DeleteFile { path }` - Delete file
- `GetMetadata { path }` - Get file info
- `FileExists { path }` - Check existence

#### Server Messages (Server → Rust → Swift)
- `Output { data }` - Shell output
- `Error { message }` - Error occurred
- `DirListing { entries }` - Directory contents
- `FileContent { data }` - File data
- `Metadata { metadata }` - File metadata
- `Success` - Operation succeeded
- `Exists { exists }` - File exists check result

### Network Layer (Iroh)

#### QUIC Connection
- **Protocol**: QUIC over UDP
- **ALPN**: `"kerr/0"`
- **Encryption**: TLS 1.3 (built into QUIC)
- **NAT Traversal**: Automatic via Iroh's relay servers

#### Stream Multiplexing
Each logical session (FileBrowser, Shell) uses:
1. Single bidirectional QUIC stream
2. Length-prefixed bincode-encoded envelopes
3. Session ID for demultiplexing on server side

#### Connection String Format
```
Base64(Gzip(JSON({
    node_id: PublicKey,
    relay_url: Option<Url>,
    direct_addresses: Vec<SocketAddr>
})))
```

## Data Flow Examples

### File Listing Flow

```
Swift: FileBrowserView.loadFiles()
  ↓
Swift: connectionManager.getFileBrowser()
  ↓
Rust: FileBrowser::list_dir("/home")
  ↓
Rust: Send ClientMessage::ListDir envelope
  ↓
Network: QUIC stream to server
  ↓
Server: Process ListDir, read filesystem
  ↓
Network: QUIC stream from server
  ↓
Rust: Receive ServerMessage::DirListing envelope
  ↓
Rust: Return Vec<FileEntry>
  ↓
Swift: Update @State files array
  ↓
SwiftUI: Rerender List view
```

### Shell Input Flow

```
User: Types "ls -la" + Enter
  ↓
SwiftUI: TextField captures input
  ↓
Swift: TerminalController.sendInput()
  ↓
Rust: ShellSession::send_input("ls -la\n")
  ↓
Rust: Send ClientMessage::Input envelope
  ↓
Network: QUIC stream to server
  ↓
Server: Write to PTY
  ↓
(Shell processes command)
  ↓
Server: Read PTY output
  ↓
Network: QUIC stream from server
  ↓
Rust: Background task receives ServerMessage::Output
  ↓
Rust: Call ShellCallback::on_output(data)
  ↓
Swift: TerminalController updates outputBuffer
  ↓
SwiftUI: Rerender terminal view with new output
```

## Threading Model

### Swift Side
- **Main Thread**: All UI updates via `@Published` and `@State`
- **Background Queue**: All Rust FFI calls via `DispatchQueue.global(qos: .userInitiated)`
- **Thread Safety**: `@ObservedObject` and `@StateObject` ensure UI updates on main thread

### Rust Side
- **Tokio Runtime**: Single static multi-threaded runtime
- **Async Operations**: All Iroh operations are async
- **Blocking Bridge**: `get_runtime().block_on()` converts async to sync for FFI
- **Mutex Protection**: All shared state wrapped in `Arc<Mutex<T>>`

### Callback Thread Safety
- Shell output callbacks are invoked from Tokio background threads
- Swift implementation must dispatch to main thread for UI updates:
  ```swift
  func onOutput(data: String) {
      DispatchQueue.main.async {
          self.outputBuffer += data
      }
  }
  ```

## Memory Management

### Swift Side
- **ARC**: Automatic Reference Counting
- **Strong References**: Views hold strong references to `@StateObject`
- **Weak References**: Use `[weak self]` in closures

### Rust Side
- **Arc**: Atomic Reference Counting for shared ownership
- **Mutex**: Interior mutability for concurrent access
- **Lifetime**: All FFI objects wrapped in `Arc` for Swift ownership

### FFI Boundary
- **UniFFI Handles**: Swift holds opaque handles to Rust objects
- **Drop Semantics**: When Swift releases last reference, Rust `Drop` is called
- **No Manual Memory**: UniFFI handles all memory management

## Error Handling

### Error Propagation
```
Rust Error → KerrError enum → Swift throws → SwiftUI .catch()
```

### Error Types
```rust
enum KerrError {
    ConnectionFailed(String),
    InvalidConnectionString,
    FileSystemError(String),
    ShellError(String),
    NetworkError(String),
    Timeout,
}
```

### Swift Error Handling
```swift
do {
    let files = try browser.listDir(path: path)
} catch let error as KerrError {
    self.errorMessage = error.localizedDescription
}
```

## Build Process

### 1. Rust Compilation
```bash
cargo build --release --target aarch64-apple-ios       # Device
cargo build --release --target aarch64-apple-ios-sim   # Simulator (M1)
cargo build --release --target x86_64-apple-ios        # Simulator (Intel)
```

### 2. Library Combination
```bash
lipo -create \
    aarch64-apple-ios-sim/release/libkerr_ios.a \
    x86_64-apple-ios/release/libkerr_ios.a \
    -output libkerr_ios_sim.a  # Universal simulator binary
```

### 3. Swift Binding Generation
```bash
uniffi-bindgen generate src/kerr_ios.udl \
    --language swift \
    --out-dir ../ios/Generated
```

Generates:
- `kerr_ios.swift` - Swift wrapper classes
- `kerr_iosFFI.h` - C FFI header

### 4. Xcode Integration
- Link `libkerr_ios.a` for device builds
- Link `libkerr_ios_sim.a` for simulator builds
- Include generated Swift/C files in project

## Testing Strategy

### Unit Tests (Rust)
```bash
cd kerr-ios
cargo test
```

### Integration Tests (Swift)
- XCTest framework in Xcode
- Test FFI boundary
- Mock Rust responses

### Manual Testing
- iOS Simulator: Fast iteration
- Physical Device: Real-world network conditions

## Performance Considerations

### Optimization
- **Rust Release Builds**: LTO enabled, size optimized
- **Stream Reuse**: Single QUIC connection for all sessions
- **Lazy Initialization**: FileBrowser/Shell created on demand
- **Background Processing**: All network I/O off main thread

### Profiling
- **Instruments**: Profile Swift UI and FFI calls
- **Tokio Console**: Profile async tasks
- **Network Link Conditioner**: Test under poor network

## Security

### Transport Security
- **QUIC/TLS 1.3**: All traffic encrypted
- **Public Key Auth**: Server identity verified by public key
- **No CA Required**: Direct trust model

### Data Security
- **No Plaintext Storage**: Connection strings are ephemeral
- **Keychain Integration**: (Future) Store credentials securely
- **App Sandbox**: iOS sandbox limits file access

## Future Enhancements

### Short Term
- SwiftTerm full integration
- QR code scanner for connection strings
- File upload from iOS
- Connection history/favorites

### Medium Term
- Background connection keep-alive
- Push notifications for disconnection
- Biometric authentication
- Share extension for files

### Long Term
- iPad multi-window support
- macOS Catalyst version
- Shortcuts integration
- Widget for quick connect
