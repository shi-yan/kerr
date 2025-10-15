use bincode::{Decode, Encode};

pub mod server;
pub mod client;

/// Messages sent from client to server
#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientMessage {
    /// Key event from the client terminal
    KeyEvent { data: Vec<u8> },
    /// Request to resize the PTY
    Resize { cols: u16, rows: u16 },
    /// Client is disconnecting
    Disconnect,
}

/// Messages sent from server to client
#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerMessage {
    /// Output from the PTY
    Output { data: Vec<u8> },
    /// Error message
    Error { message: String },
}

/// ALPN for the Kerr protocol
pub const ALPN: &[u8] = b"kerr/0";
