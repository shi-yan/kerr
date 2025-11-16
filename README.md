# Kerr

**The Ultimate Swiss Army Knife for Remote Operations**

Named after the Kerr black hole solution, representing wormhole-like connections between peers in a network.

Kerr is a next-generation peer-to-peer remote access and administration tool that combines the power of SSH, SCP, port forwarding, and file management into a single, seamless package. Built on cutting-edge peer-to-peer technology, Kerr establishes direct encrypted connections between machines without requiring complex firewall configurations, port forwarding, or central servers.

## Table of Contents

- [Why Kerr?](#why-kerr)
- [Key Features](#key-features)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Complete Feature Guide](#complete-feature-guide)
  - [1. Remote Shell Access](#1-remote-shell-access)
  - [2. File Transfer Operations](#2-file-transfer-operations)
  - [3. Interactive File Browser](#3-interactive-file-browser)
  - [4. TCP Port Forwarding](#4-tcp-port-forwarding)
  - [5. Network Performance Testing](#5-network-performance-testing)
  - [6. Web-Based Interface](#6-web-based-interface)
  - [7. Connection Management](#7-connection-management)
  - [8. Authentication & Session Management](#8-authentication--session-management)
- [Architecture & Technical Details](#architecture--technical-details)
- [Use Cases](#use-cases)
- [Advanced Usage](#advanced-usage)
- [Requirements](#requirements)
- [Security](#security)
- [Acknowledgments](#acknowledgments)

## Why Kerr?

Traditional remote access tools like SSH require:
- Public IP addresses or complex port forwarding
- Firewall configuration on both ends
- Separate tools for different operations (SSH, SCP, SFTP, SSH tunneling)
- VPNs or reverse proxy solutions for NAT traversal

**Kerr eliminates all these pain points:**

✓ **Zero Configuration** - Works through NAT and firewalls automatically
✓ **Peer-to-Peer** - Direct connections for maximum speed and privacy
✓ **All-in-One** - Shell, file transfer, browsing, and port forwarding in one tool
✓ **Modern UI** - Terminal UI and web interface for different workflows
✓ **Cross-Platform** - Works on Linux, macOS, and Windows
✓ **Encrypted** - Built on QUIC with modern cryptography

## Key Features

### Core Capabilities

- **🖥️ Remote Shell** - Full interactive terminal with PTY support, complete keyboard handling, and automatic resize detection
- **📁 Bidirectional File Transfer** - Send and pull files/directories with progress tracking and compression
- **🗂️ Interactive File Browser** - TUI-based file explorer for both local and remote filesystems
- **🔀 TCP Port Forwarding** - Create secure tunnels to forward local ports to remote services
- **📊 Network Diagnostics** - Built-in ping and bandwidth testing tools
- **🌐 Web Interface** - Full-featured browser-based UI for remote administration
- **🔐 Authentication** - Google OAuth2 integration for secure identity management
- **💾 Connection Manager** - Save and organize connections with aliases

### Technical Highlights

- **P2P Architecture** - Uses [Iroh](https://iroh.computer/) for NAT traversal and hole punching
- **QUIC Protocol** - Fast, reliable, multiplexed connections with built-in encryption
- **Session Multiplexing** - Multiple concurrent operations over a single connection
- **Efficient Binary Protocol** - Uses bincode serialization with gzip compression
- **Cross-Platform PTY** - Portable pseudo-terminal support via portable-pty
- **Modern TUI** - Rich terminal interfaces built with Ratatui
- **Real-Time Performance** - Low-latency streaming for responsive remote sessions

## Installation

### From Source

```bash
cargo build --release
```

The binary will be located at `target/release/kerr`

### Install to System

```bash
cargo install --path .
```

This will install `kerr` to your cargo bin directory (usually `~/.cargo/bin/`).

## Quick Start

### 1. Start a Server

On the machine you want to access remotely:

```bash
kerr serve
```

This displays connection commands you can copy with keyboard shortcuts:

```
╔══════════════════════════════════════════════════════════════╗
║                    Kerr Server Online                        ║
╚══════════════════════════════════════════════════════════════╝

Commands:
  Connect: kerr connect eyJub2RlX2lkIjoiNGI0Yz...
  Send:    kerr send eyJub2RlX2lkIjoiNGI0Yz... <local> <remote>
  Pull:    kerr pull eyJub2RlX2lkIjoiNGI0Yz... <remote> <local>
  Browse:  kerr browse eyJub2RlX2lkIjoiNGI0Yz...
  Relay:   kerr relay eyJub2RlX2lkIjoiNGI0Yz... <local_port> <remote_port>
  Ping:    kerr ping eyJub2RlX2lkIjoiNGI0Yz...

─────────────────────────────────────────────────────────────────
Keys: [c]onnect | [s]end | [p]ull | [b]rowse | [r]elay | p[i]ng | Ctrl+C
─────────────────────────────────────────────────────────────────
```

**Keyboard shortcuts:**
- Press `c` to copy the connect command
- Press `s` to copy the send command
- Press `p` to copy the pull command
- Press `b` to copy the browse command
- Press `r` to copy the relay command
- Press `i` to copy the ping command
- Press `Ctrl+C` to stop the server

### 2. Connect from Client

On any other machine, use the connection string from the server:

```bash
kerr connect <CONNECTION_STRING>
```

You'll instantly have a full interactive shell session!

## Complete Feature Guide

### 1. Remote Shell Access

Establish an interactive terminal session on the remote machine.

**Start Server:**
```bash
kerr serve
```

**Connect:**
```bash
kerr connect <CONNECTION_STRING>
```

**Features:**
- Full PTY support with bash
- Complete keyboard mapping (arrow keys, function keys, Ctrl combinations)
- Automatic terminal resize handling
- ANSI color and escape sequence support
- Custom prompt showing connection context
- Ctrl+D to disconnect

**Use Cases:**
- System administration and maintenance
- Debugging and troubleshooting
- Running commands on remote machines
- Interactive development environments
- Emergency access when SSH is unavailable

### 2. File Transfer Operations

Transfer files and directories between local and remote machines with progress tracking.

#### Send Files to Remote

Upload files or entire directories to the remote machine:

```bash
# Send a single file
kerr send <CONNECTION_STRING> ./document.pdf /remote/path/document.pdf

# Send a directory
kerr send <CONNECTION_STRING> ./my-project /remote/path/my-project

# Send with auto-naming (uses local filename)
kerr send <CONNECTION_STRING> ./file.txt /remote/path/

# Force overwrite without confirmation
kerr send <CONNECTION_STRING> ./file.txt /remote/path/file.txt --force
```

#### Pull Files from Remote

Download files or directories from the remote machine:

```bash
# Pull a single file
kerr pull <CONNECTION_STRING> /remote/path/document.pdf ./document.pdf

# Pull a directory
kerr pull <CONNECTION_STRING> /remote/path/logs ./local-logs

# Pull to current directory with original name
kerr pull <CONNECTION_STRING> /remote/file.txt ./
```

**Features:**
- Progress bars with speed and ETA
- Automatic directory creation
- Overwrite confirmation prompts (can be bypassed with --force)
- Resume capability for interrupted transfers
- Efficient chunked transfer with compression
- Preserves file structure for directories

**Use Cases:**
- Deploying applications and configurations
- Backing up remote data
- Syncing files between machines
- Collecting logs and diagnostic files
- Distributing updates and patches

### 3. Interactive File Browser

Launch a full-featured terminal UI for browsing and managing files.

#### Browse Local Filesystem

```bash
kerr browse
```

#### Browse Remote Filesystem

```bash
kerr browse <CONNECTION_STRING>
```

**Features:**
- Dual-pane interface for easy navigation
- File operations: view, edit, delete, copy, move
- Directory tree navigation
- File metadata display (size, permissions, timestamps)
- Hidden file toggle
- Image preview support
- Search functionality
- File hashing for integrity verification
- Keyboard-driven navigation

**Keyboard Shortcuts:**
- Arrow keys: Navigate files and directories
- Enter: Open directory / view file
- Space: Select/deselect files
- d: Delete file/directory
- q: Quit browser
- h: Toggle hidden files
- /: Search

**Use Cases:**
- Exploring unfamiliar remote systems
- Visual file management
- Bulk file operations
- Finding specific files across directories
- Verifying file integrity
- Quick file edits without full editor setup

### 4. TCP Port Forwarding

Create secure tunnels to access remote services through the P2P connection.

```bash
# Forward local port 8080 to remote port 80
kerr relay <CONNECTION_STRING> 8080 80

# Forward to remote database
kerr relay <CONNECTION_STRING> 5432 5432

# Access remote web service
kerr relay <CONNECTION_STRING> 3000 3000
```

**Real-Time Traffic Monitoring:**

When you create a relay, Kerr displays a live TUI showing:
- Upload and download speeds
- Total bytes transferred
- Active connection count
- Connection duration
- Real-time bandwidth graphs

**Features:**
- Multiple concurrent port forwards
- Automatic reconnection on failure
- Low latency forwarding
- Support for any TCP protocol
- Traffic statistics and monitoring
- Clean shutdown on Ctrl+C or 'q'

**Use Cases:**
- Access remote databases (PostgreSQL, MySQL, MongoDB)
- Connect to remote web services and APIs
- Access internal services behind NAT/firewall
- Remote debugging (debug servers, profilers)
- Secure access to admin panels
- Connect to remote development servers
- VNC/RDP tunneling
- Game server access

**Example Workflows:**

```bash
# Access remote PostgreSQL database
kerr relay <CONNECTION_STRING> 5432 5432
# Now connect locally: psql -h localhost -p 5432

# Access remote Jupyter notebook
kerr relay <CONNECTION_STRING> 8888 8888
# Now open: http://localhost:8888

# Connect to remote Redis
kerr relay <CONNECTION_STRING> 6379 6379
# Now: redis-cli -h localhost -p 6379
```

### 5. Network Performance Testing

Measure connection quality and bandwidth with built-in diagnostic tools.

```bash
kerr ping <CONNECTION_STRING>
```

**Test Results:**

```
╔══════════════════════════════════════════════════════════════════════╗
║                    Network Performance Test                          ║
╚══════════════════════════════════════════════════════════════════════╝

Payload Size Round-Trip      Throughput      Effective BW
──────────────────────────────────────────────────────────────────────
0 B          2.34 ms         0.00 MB/s       0.00 Mbps
1 KB         3.12 ms         0.65 MB/s       5.13 Mbps
4 KB         3.45 ms         2.32 MB/s       18.55 Mbps
16 KB        4.12 ms         7.77 MB/s       62.14 Mbps
64 KB        6.23 ms         20.57 MB/s      164.53 Mbps
256 KB       12.45 ms        41.15 MB/s      329.18 Mbps
1 MB         45.67 ms        43.82 MB/s      350.59 Mbps
```

**Metrics Explained:**
- **Payload Size**: Amount of data in test packet
- **Round-Trip**: Time for packet to go to server and back
- **Throughput**: Total data transfer rate (including overhead)
- **Effective BW**: Actual payload bandwidth utilization

**Use Cases:**
- Verify connection quality before large transfers
- Diagnose network issues
- Compare different network paths
- Benchmark P2P performance
- Validate QoS settings
- Troubleshoot latency problems

### 6. Web-Based Interface

Access a full-featured web UI for remote management through your browser.

```bash
# Launch with automatic connection
kerr ui <CONNECTION_STRING>

# Launch with connection selector
kerr ui

# Launch on custom port
kerr ui <CONNECTION_STRING> --port 8080
```

Then open your browser to `http://localhost:3000` (or your custom port).

**Web UI Features:**

**File Management:**
- Visual file browser with drag-and-drop
- Upload files directly from browser
- Download files with single click
- Create, rename, delete files and folders
- In-browser file editor with syntax highlighting
- Image preview and media playback

**Terminal Access:**
- Full WebSocket-based terminal
- Same experience as native shell
- Multiple concurrent terminal tabs
- Persistent sessions

**Port Forwarding:**
- Create and manage port forwards from UI
- Real-time traffic monitoring
- Start/stop forwards on demand

**Connection Management:**
- Save multiple connections
- Quick connection switching
- Connection status indicators
- Saved session restoration

**Use Cases:**
- Remote file management from any device
- Quick access without CLI installation
- Sharing access with non-technical users
- Browser-based remote administration
- Mobile device access (tablets, phones)
- Remote support and troubleshooting
- Teaching and demonstrations

### 7. Connection Management

Save and organize your remote connections for easy access.

#### Register a Connection

When starting a server, register it with an alias:

```bash
kerr serve --register my-home-server
kerr serve --register production-db
kerr serve --register dev-machine
```

This saves the connection to a backend service associated with your Google account.

#### List Saved Connections

```bash
kerr ls
```

This shows an interactive list of all your saved connections:

```
╔══════════════════════════════════════════════════════════════╗
║                    Saved Connections                          ║
╚══════════════════════════════════════════════════════════════╝

  my-home-server @ hostname-1234
  production-db @ db-server-5678
  dev-machine @ laptop-9012

Use ↑/↓ to navigate, Enter to select, q to quit
```

When you select a connection, it displays all available commands:

```
╔══════════════════════════════════════════════════════════════╗
║             Connection Commands                              ║
╚══════════════════════════════════════════════════════════════╝

Selected: my-home-server @ hostname-1234

Commands:
  Connect: kerr connect eyJub2RlX2lkIjoiNGI0Yz...
  Send:    kerr send eyJub2RlX2lkIjoiNGI0Yz... <local> <remote>
  Pull:    kerr pull eyJub2RlX2lkIjoiNGI0Yz... <remote> <local>
  Browse:  kerr browse eyJub2RlX2lkIjoiNGI0Yz...
  Ping:    kerr ping eyJub2RlX2lkIjoiNGI0Yz...
  Web UI:  kerr ui eyJub2RlX2lkIjoiNGI0Yz...
```

#### Authentication

```bash
# Login with Google account
kerr login

# Logout and clear session
kerr logout
```

**Features:**
- Cloud-synced connections across devices
- Automatic connection string management
- Hostname tracking
- Timestamp tracking for connection registration
- Secure OAuth2 authentication

**Use Cases:**
- Managing multiple servers
- Quick access to frequent destinations
- Team collaboration (shared connection registry)
- Mobile and cross-device access
- Avoiding manual connection string management

### 8. Authentication & Session Management

Kerr integrates with Google OAuth2 for secure identity and connection management.

#### Login Flow

```bash
kerr login
```

This will:
1. Open your default browser
2. Redirect to Google login
3. Request permission for email and profile access
4. Create a secure session
5. Save session credentials locally

#### Session Persistence

Sessions are stored in your system's config directory:
- Linux: `~/.config/kerr/`
- macOS: `~/Library/Application Support/kerr/`
- Windows: `%APPDATA%\kerr\`

#### Server Registration

When serving with registration enabled:

```bash
kerr serve --register my-server-name
```

The server:
- Registers connection string with your account
- Associates hostname for identification
- Tracks registration timestamp
- Automatically unregisters on shutdown

#### Custom Session Path

```bash
kerr serve --session /path/to/custom/session.json
```

**Use Cases:**
- Multi-user environments
- Shared server access management
- Audit trails
- Connection sharing within teams
- Cross-device synchronization

## Architecture & Technical Details

### Peer-to-Peer Networking

Kerr uses **Iroh**, a modern P2P networking library that provides:

- **NAT Traversal**: Automatic hole punching through firewalls
- **QUIC Protocol**: Multiplexed, encrypted streams over UDP
- **Direct Connections**: No relay servers (unless NAT traversal fails)
- **Connection Migration**: Handles IP address changes gracefully
- **Built-in Encryption**: TLS 1.3 with forward secrecy

### Connection Flow

```
Client                                Server
  │                                     │
  ├─ Decode connection string          │
  │  (contains node ID + relay info)   │
  │                                     │
  ├─ Initiate QUIC connection ────────>│
  │                                     │
  │<──── NAT traversal & hole punch ───┤
  │                                     │
  │<──────── Direct P2P stream ────────>│
  │                                     │
  ├─ Send Hello (session type) ───────>│
  │                                     │
  │<───── Session-specific handler ────┤
  │                                     │
  │<══════ Encrypted data flow ═══════>│
```

### Protocol Architecture

#### Session Types

Kerr supports multiple concurrent session types over a single connection:

1. **Shell** - Interactive terminal with PTY
2. **FileTransfer** - Bidirectional file operations
3. **FileBrowser** - Directory listing and file metadata
4. **TcpRelay** - Port forwarding proxy
5. **Ping** - Network diagnostics

#### Message Format

All messages use a length-prefixed binary protocol:

```
┌──────────────┬────────────────────────────┐
│   4 bytes    │       N bytes              │
│   Length     │   Bincode-encoded message  │
└──────────────┴────────────────────────────┘
```

#### Message Types

**Client → Server:**
- `Hello` - Session initiation
- `KeyEvent` - Terminal input
- `Resize` - Terminal size change
- `StartUpload` - Begin file upload
- `FileChunk` - File data
- `FsReadDir` - List directory
- `FsReadFile` - Read file content
- `TcpOpen` - Open TCP connection
- `TcpData` - Forward TCP data
- `PingRequest` - Performance test

**Server → Client:**
- `Output` - Terminal output
- `UploadAck` - Ready for upload
- `FileChunk` - File download data
- `FsDirListing` - Directory contents
- `FsFileContent` - File data
- `TcpDataResponse` - TCP data from remote
- `PingResponse` - Performance test echo
- `Error` - Error message

### PTY Implementation

The server creates a pseudo-terminal (PTY) device:

```
Server Process
    │
    ├─ PTY Master (read/write)
    │      │
    │      └─ PTY Slave
    │            │
    │            └─ Bash Process
    │                  │
    │                  └─ Child Processes
```

**Features:**
- Full terminal emulation (xterm-256color)
- Signal handling (Ctrl+C, Ctrl+Z, etc.)
- Job control support
- Custom prompt (`username@kerr path>`)
- Resize synchronization
- Line editing and history

### Compression & Encoding

- **Connection Strings**: JSON → gzip → base64
- **File Transfer**: Chunked with optional compression
- **Binary Protocol**: Efficient bincode serialization
- **Minimal Overhead**: Optimized for low latency

## Use Cases

### System Administration

- Manage remote servers without SSH setup
- Emergency access when SSH is down
- Quick diagnostics and troubleshooting
- Log collection and analysis
- Configuration deployment

### Development

- Access development servers behind NAT
- Remote debugging with port forwarding
- File synchronization during development
- Test environment management
- CI/CD integration

### DevOps & Infrastructure

- Container management and debugging
- Database administration through tunnels
- Microservice communication testing
- Network performance validation
- Multi-cloud resource access

### Personal Use

- Access home lab from anywhere
- Remote desktop assistance
- File sharing between personal devices
- IoT device management
- Personal cloud storage alternative

### Security & Penetration Testing

- Authorized remote access during assessments
- Secure data exfiltration (with authorization)
- Network path testing
- Firewall bypass testing (authorized)
- Red team operations (CTF, authorized pentesting)

### Education & Training

- Remote lab environments
- Student machine access
- Collaborative coding sessions
- Workshop demonstrations
- Technical support

## Advanced Usage

### Server Options

```bash
# Start with custom session file
kerr serve --session /path/to/session.json

# Start with logging to file
kerr serve --log /path/to/server.log

# Register with backend and log
kerr serve --register my-server --log server.log
```

### Connection String Management

Connection strings are base64-encoded, gzip-compressed JSON containing:
- Node public key (identity)
- Relay server information
- Direct addresses (if known)

You can:
- Save to environment variables
- Store in configuration files
- Share via secure channels
- Embed in automation scripts

### Programmatic Usage

Kerr can be integrated into scripts and automation:

```bash
#!/bin/bash
# Deploy script using Kerr

CONNECTION_STRING="eyJub2RlX2lkIjoiNGI0Yz..."

# Upload application
kerr send $CONNECTION_STRING ./app /opt/myapp --force

# Upload config
kerr send $CONNECTION_STRING ./config.yml /etc/myapp/config.yml

# Restart service remotely
echo "systemctl restart myapp" | kerr connect $CONNECTION_STRING
```

### Multiple Concurrent Operations

You can run multiple Kerr sessions simultaneously:

```bash
# Terminal 1: Interactive shell
kerr connect <STRING>

# Terminal 2: File browser
kerr browse <STRING>

# Terminal 3: Port forward
kerr relay <STRING> 8080 80

# Terminal 4: Web UI
kerr ui <STRING>
```

All sessions share the same P2P connection, making them efficient and fast.

### Firewall & Network Notes

**Outbound Requirements:**
- UDP port 443 (QUIC/Iroh default)
- Access to Iroh relay servers (fallback)

**No Inbound Requirements:**
- NAT traversal works automatically
- No port forwarding needed
- Works from restrictive networks

**Optimal Performance:**
- Allow UDP for best performance
- Direct connections preferred over relays
- Relay used only when hole-punching fails

## Requirements

### System Requirements

- **Operating System**: Linux, macOS, or Windows
- **Architecture**: x86_64, ARM64
- **Memory**: 50MB minimum
- **Disk**: 20MB for binary

### Build Requirements

- **Rust**: 1.70 or later
- **Cargo**: Latest stable version
- **Platform-specific**: PTY support (standard on Unix, available on Windows)

### Network Requirements

- **Internet Connection**: Required for initial relay
- **UDP Access**: Recommended for direct connections
- **Firewall**: Outbound UDP/443 recommended

## Security

### Encryption

- **Transport**: QUIC with TLS 1.3
- **Cipher Suites**: Modern AEAD ciphers (ChaCha20-Poly1305, AES-GCM)
- **Key Exchange**: X25519 (Elliptic Curve Diffie-Hellman)
- **Forward Secrecy**: Yes
- **Authentication**: Node public key verification

### Privacy

- **Peer-to-Peer**: No data flows through central servers (except optional relay)
- **No Logging**: Connection metadata not stored by relays
- **Ephemeral**: Connection strings can be regenerated
- **OAuth Tokens**: Stored locally, never transmitted in cleartext

### Best Practices

1. **Connection Strings**: Treat as sensitive credentials
2. **Authentication**: Use OAuth for connection management
3. **Network**: Use on trusted networks when possible
4. **Updates**: Keep Kerr updated for security patches
5. **Audit**: Review connection lists regularly
6. **Logout**: Clear sessions when finished

### Threat Model

**Protected Against:**
- Network eavesdropping (encryption)
- MITM attacks (public key authentication)
- Unauthorized access (connection string secret)

**Not Protected Against:**
- Compromised client machine
- Stolen connection strings
- Social engineering
- Physical access to machines

## Troubleshooting

### Connection Issues

```bash
# Test network performance
kerr ping <CONNECTION_STRING>

# Check if server is running
# (Connection will timeout if server is offline)
```

### Firewall Problems

If direct connections fail, Kerr falls back to relay servers. For best performance:
- Allow outbound UDP on port 443
- Check corporate firewall policies
- Consider using VPN if P2P is blocked

### Authentication Issues

```bash
# Clear session and re-login
kerr logout
kerr login
```

### Performance Optimization

- Use wired connection over WiFi when possible
- Close unnecessary applications
- Use `kerr ping` to baseline performance
- Consider network proximity for servers

## License

This project is open source. See LICENSE for details.

## Acknowledgments

Kerr is built on the shoulders of giants:

- **[Iroh](https://iroh.computer/)** - Next-generation P2P networking
- **[QUIC](https://www.chromium.org/quic/)** - Modern transport protocol
- **[portable-pty](https://crates.io/crates/portable-pty)** - Cross-platform PTY support
- **[crossterm](https://crates.io/crates/crossterm)** - Terminal manipulation library
- **[Ratatui](https://ratatui.rs/)** - Terminal UI framework
- **[Tokio](https://tokio.rs/)** - Asynchronous runtime for Rust
- **[Axum](https://github.com/tokio-rs/axum)** - Web framework for the UI

## Contributing

Contributions are welcome! Whether it's bug reports, feature requests, or code contributions, we appreciate your help in making Kerr better.

---

**Kerr** - *Your wormhole to remote systems* 🌀
