use bincode::{Decode, Encode};

pub mod server;
pub mod client;
pub mod transfer;
pub mod browser;
pub mod custom_explorer;

/// Session type for initial handshake
#[derive(Debug, Clone, Encode, Decode)]
pub enum SessionType {
    /// Interactive shell session
    Shell,
    /// File transfer session
    FileTransfer,
}

/// Messages sent from client to server
#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientMessage {
    /// Initial handshake with session type
    Hello { session_type: SessionType },
    /// Key event from the client terminal
    KeyEvent { data: Vec<u8> },
    /// Request to resize the PTY
    Resize { cols: u16, rows: u16 },
    /// Client is disconnecting
    Disconnect,
    /// Start file upload (send)
    StartUpload { path: String, size: u64, is_dir: bool, force: bool },
    /// File data chunk
    FileChunk { data: Vec<u8> },
    /// End of file upload
    EndUpload,
    /// Confirmation response (true = yes, false = no)
    ConfirmResponse { confirmed: bool },
    /// Request file download (pull)
    RequestDownload { path: String },
}

/// Messages sent from server to client
#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerMessage {
    /// Output from the PTY
    Output { data: Vec<u8> },
    /// Error message
    Error { message: String },
    /// Acknowledge upload start
    UploadAck,
    /// Ask for confirmation (e.g., file exists, overwrite?)
    ConfirmPrompt { message: String },
    /// Start file download
    StartDownload { size: u64, is_dir: bool },
    /// File data chunk
    FileChunk { data: Vec<u8> },
    /// End of file download
    EndDownload,
    /// Transfer progress
    Progress { bytes_transferred: u64, total_bytes: u64 },
}

/// ALPN for the Kerr protocol
pub const ALPN: &[u8] = b"kerr/0";
