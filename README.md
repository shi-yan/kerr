# Kerr

A peer-to-peer remote shell application - like SSH through a wormhole.

Named after the Kerr black hole solution, representing wormhole-like connections between peers in a network.

## Features

- **Peer-to-peer connections** using Iroh for NAT traversal
- **Full PTY support** with bash running on the server
- **Real-time terminal interaction** with complete keyboard support
- **Terminal resize handling** for proper display adaptation
- **Efficient binary protocol** using bincode serialization
- **No central server required** - direct peer-to-peer connections

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
```

The binary will be located at `target/release/kerr`

## Usage

### Start the Server

```bash
kerr serve
```

This will:
- Start the Kerr server
- Create a PTY with bash
- Display the connection command in the terminal
- Press `c` to copy the full connection command to clipboard
- Press `Ctrl+C` to stop the server

The output will look like:

```
╔══════════════════════════════════════════════════════════════╗
║                    Kerr Server Online                        ║
╚══════════════════════════════════════════════════════════════╝

Connection command:

  kerr connect eyJub2RlX2lkIjoiNGI0Yz...base64string...3RpbmciXX0

─────────────────────────────────────────────────────────────────
Press 'c' to copy to clipboard | Ctrl+C to stop server
─────────────────────────────────────────────────────────────────
```

### Connect to a Server

```bash
kerr connect <CONNECTION_STRING>
```

Example:
```bash
kerr connect eyJub2RlX2lkIjoiNGI0Yz...base64string...3RpbmciXX0
```

This will:
- Decode the connection string
- Connect to the server
- Provide an interactive terminal session
- Press `Ctrl+D` to disconnect

## How it Works

Kerr uses [Iroh](https://iroh.computer/) for peer-to-peer connectivity, which handles:
- NAT traversal
- Hole punching
- Direct connections between peers
- Encrypted communication

The server creates a PTY (pseudo-terminal) device and spawns bash inside it. The server's node address is encoded as a base64 connection string. When clients connect using this string, their keyboard input is sent to the PTY, and the PTY output is streamed back to the client in real-time.

## Architecture

```
Client                          Server
  │                               │
  ├─ Keyboard Input              ├─ PTY Master
  │   (crossterm)                │   (portable-pty)
  │                               │
  ├─ Message Encoding            ├─ Bash Process
  │   (bincode)                  │
  │                               │
  └─ P2P Connection ←────────────┤
      (Iroh/QUIC)                └─ Output Streaming
```

## Protocol

Kerr uses a simple binary protocol with length-prefixed messages:

**Client → Server:**
- `KeyEvent { data: Vec<u8> }` - Raw key input
- `Resize { cols: u16, rows: u16 }` - Terminal resize
- `Disconnect` - Clean disconnect signal

**Server → Client:**
- `Output { data: Vec<u8> }` - PTY output
- `Error { message: String }` - Error messages

## Requirements

- Rust 1.70 or later
- macOS, Linux, or Windows (with PTY support)
- Network connectivity for peer discovery

## License

This project is open source. See LICENSE for details.

## Acknowledgments

- Built on [Iroh](https://iroh.computer/) for P2P networking
- Uses [portable-pty](https://crates.io/crates/portable-pty) for cross-platform PTY support
- Terminal handling via [crossterm](https://crates.io/crates/crossterm)
