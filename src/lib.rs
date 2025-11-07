use bincode::{Decode, Encode};
use base64::Engine;

pub mod server;
pub mod client;
pub mod transfer;
pub mod browser;
pub mod custom_explorer;
pub mod auth;
pub mod connections_list;
pub mod traffic_ui;
pub mod debug_log;
pub mod web_ui;

/// Session type for initial handshake
#[derive(Debug, Clone, Encode, Decode)]
pub enum SessionType {
    /// Interactive shell session
    Shell,
    /// File transfer session
    FileTransfer,
    /// File browser session
    FileBrowser,
    /// TCP relay session
    TcpRelay,
    /// Network performance testing session
    Ping,
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
    /// Request to list directory contents (for file browser)
    FsReadDir { path: String },
    /// Request file metadata (for file browser)
    FsMetadata { path: String },
    /// Request to read file content (for file browser)
    FsReadFile { path: String },
    /// Request file hash (for file browser caching)
    FsHashFile { path: String },
    /// Request to delete a file or directory (for file browser)
    FsDelete { path: String },
    /// Open a new TCP connection on the remote server
    TcpOpen { stream_id: u32, destination_port: u16 },
    /// Send TCP data to a remote connection
    TcpData { stream_id: u32, data: Vec<u8> },
    /// Close a TCP connection
    TcpClose { stream_id: u32 },
    /// Ping request with payload
    PingRequest { data: Vec<u8> },
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
    /// Directory listing response (for file browser)
    FsDirListing { entries_json: String },
    /// File metadata response (for file browser)
    FsMetadataResponse { metadata_json: String },
    /// File content response (for file browser)
    FsFileContent { data: Vec<u8> },
    /// File hash response (for file browser caching) - 32 bytes blake3 hash as hex string
    FsHashResponse { hash: String },
    /// File deletion confirmation response (for file browser)
    FsDeleteResponse { success: bool },
    /// Filesystem error notification (for file browser UI feedback)
    FsError { message: String },
    /// TCP connection opened successfully (or failed)
    TcpOpenResponse { stream_id: u32, success: bool, error: Option<String> },
    /// TCP data received from remote server
    TcpDataResponse { stream_id: u32, data: Vec<u8> },
    /// TCP connection closed or error occurred
    TcpCloseResponse { stream_id: u32, error: Option<String> },
    /// Ping response echoing back the payload
    PingResponse { data: Vec<u8> },
}

/// ALPN for the Kerr protocol
pub const ALPN: &[u8] = b"kerr/0";

/// Encode a NodeAddr as a compressed connection string (JSON -> gzip -> base64)
pub fn encode_connection_string(addr: &iroh::NodeAddr) -> String {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let addr_json = serde_json::to_string(addr).unwrap();

    // Compress with gzip
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(addr_json.as_bytes()).unwrap();
    let compressed = encoder.finish().unwrap();

    // Base64 encode
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&compressed)
}

/// Decode a compressed connection string to NodeAddr (base64 -> gzip -> JSON)
pub fn decode_connection_string(connection_string: &str) -> Result<iroh::NodeAddr, Box<dyn std::error::Error>> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    // Base64 decode
    let compressed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(connection_string.as_bytes())?;

    // Decompress with gzip
    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut addr_json = String::new();
    decoder.read_to_string(&mut addr_json)?;

    // Parse JSON
    let addr: iroh::NodeAddr = serde_json::from_str(&addr_json)?;
    Ok(addr)
}
