use std::sync::Arc;
use anyhow::Result;
use base64::Engine as _;

// Include the kerr core types
// We'll need to reference the parent crate
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

pub(crate) mod types;
mod endpoint;
mod session;
mod file_browser;
mod shell;

pub use types::*;
pub use endpoint::*;
pub use session::*;
pub use file_browser::*;
pub use shell::*;

// UniFFI will generate bindings from this
uniffi::include_scaffolding!("kerr_ios");

// Helper function to create a tokio runtime for async operations
fn get_runtime() -> &'static tokio::runtime::Runtime {
    use std::sync::OnceLock;
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    })
}

// Top-level functions exposed to Swift
pub fn create_endpoint() -> Result<Arc<Endpoint>, KerrError> {
    let runtime = get_runtime();
    runtime.block_on(async {
        Endpoint::new().await
    })
}

pub fn decode_connection_string(conn_str: String) -> Result<String, KerrError> {
    // Decode and return a human-readable description
    // In practice, this validates the connection string
    let _decoded = decode_addr(&conn_str)?;
    Ok(format!("Valid connection string"))
}

// Helper to decode connection string (from parent crate logic)
fn decode_addr(conn_str: &str) -> Result<iroh::EndpointAddr, KerrError> {
    let trimmed = conn_str.trim();
    eprintln!("[kerr] decode_addr: input length={}", trimmed.len());

    // Decode base64 (URL-safe, no padding)
    let compressed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(trimmed)
        .map_err(|e| {
            eprintln!("[kerr] decode_addr: base64 failed: {e}");
            KerrError::InvalidConnectionString(format!("base64 decode failed: {e}"))
        })?;
    eprintln!("[kerr] decode_addr: base64 ok, compressed len={}", compressed.len());

    // Decompress gzip
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(&compressed[..]);
    let mut json_str = String::new();
    decoder
        .read_to_string(&mut json_str)
        .map_err(|e| {
            eprintln!("[kerr] decode_addr: gzip failed: {e}");
            KerrError::InvalidConnectionString(format!("gzip decompress failed: {e}"))
        })?;
    eprintln!("[kerr] decode_addr: gzip ok, json={json_str}");

    // Deserialize JSON into EndpointAddr
    let addr: iroh::EndpointAddr = serde_json::from_str(&json_str)
        .map_err(|e| {
            eprintln!("[kerr] decode_addr: json parse failed: {e}");
            KerrError::InvalidConnectionString(format!("json parse failed: {e} (json was: {json_str})"))
        })?;
    eprintln!("[kerr] decode_addr: success");

    Ok(addr)
}

// Helper to encode connection string
#[allow(dead_code)]
fn encode_addr(addr: &iroh::EndpointAddr) -> Result<String, KerrError> {
    use std::io::Write;

    let json_str = serde_json::to_string(addr)
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(json_str.as_bytes())
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;
    let compressed = encoder.finish().map_err(|e| KerrError::NetworkError(e.to_string()))?;

    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&compressed);
    Ok(encoded)
}

// Message types (copied from parent crate - we need these for protocol)
const ALPN: &[u8] = b"kerr/0";

#[derive(Debug, Archive, RkyvSerialize, RkyvDeserialize)]
#[rkyv(derive(Debug))]
pub enum SessionType {
    Shell,
    FileTransfer,
    FileBrowser,
    TcpRelay,
    Ping,
    HttpProxy,
    Dns,
}

#[derive(Debug, Archive, RkyvSerialize, RkyvDeserialize)]
#[rkyv(derive(Debug))]
pub struct MessageEnvelope {
    pub session_id: String,
    pub payload: MessagePayload,
}

#[derive(Debug, Archive, RkyvSerialize, RkyvDeserialize)]
#[rkyv(derive(Debug))]
pub enum MessagePayload {
    Client(ClientMessage),
    Server(ServerMessage),
}

#[derive(Debug, Archive, RkyvSerialize, RkyvDeserialize)]
#[rkyv(derive(Debug))]
pub enum ClientMessage {
    Hello { session_type: SessionType },
    KeyEvent { data: Vec<u8> },
    Resize { cols: u16, rows: u16 },
    Disconnect,
    StartUpload { path: String, size: u64, is_dir: bool, force: bool },
    FileChunk { data: Vec<u8> },
    EndUpload,
    FileStart { relative_path: String, size: u64 },
    ConfirmResponse { confirmed: bool },
    RequestDownload { path: String, offset: u64 },
    FsReadDir { path: String },
    FsMetadata { path: String },
    FsReadFile { path: String },
    FsHashFile { path: String },
    FsDelete { path: String },
    TcpOpen { stream_id: u32, destination_host: Option<String>, destination_port: u16 },
    TcpData { stream_id: u32, data: Vec<u8> },
    TcpClose { stream_id: u32 },
    PingRequest { data: Vec<u8> },
    DnsQuery { query_id: u32, query_data: Vec<u8> },
}

#[derive(Debug, Archive, RkyvSerialize, RkyvDeserialize)]
#[rkyv(derive(Debug))]
pub enum ServerMessage {
    Output { data: Vec<u8> },
    Error { message: String },
    UploadAck,
    ConfirmPrompt { message: String },
    StartDownload { size: u64, is_dir: bool },
    FileChunk { data: Vec<u8> },
    EndDownload,
    FileStart { relative_path: String, size: u64 },
    Progress { bytes_transferred: u64, total_bytes: u64 },
    FsDirListing { entries_json: String },
    FsMetadataResponse { metadata_json: String },
    FsFileContent { data: Vec<u8> },
    FsHashResponse { hash: String },
    FsDeleteResponse { success: bool },
    FsError { message: String },
    TcpOpenResponse { stream_id: u32, success: bool, error: Option<String> },
    TcpDataResponse { stream_id: u32, data: Vec<u8> },
    TcpCloseResponse { stream_id: u32, error: Option<String> },
    PingResponse { data: Vec<u8> },
    DnsResponse { query_id: u32, response_data: Vec<u8> },
}

// Helper to send envelope
async fn send_envelope(
    send: &mut iroh::endpoint::SendStream,
    envelope: &MessageEnvelope,
) -> Result<(), KerrError> {
    let data = rkyv::to_bytes::<rkyv::rancor::Error>(envelope)
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;
    let len = data.len() as u32;
    send.write_all(&len.to_be_bytes())
        .await
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;
    send.write_all(&data)
        .await
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;
    Ok(())
}

// Helper to receive envelope
async fn recv_envelope(
    recv: &mut iroh::endpoint::RecvStream,
) -> Result<MessageEnvelope, KerrError> {
    use tokio::io::AsyncReadExt;

    let mut len_bytes = [0u8; 4];
    recv.read_exact(&mut len_bytes)
        .await
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    let mut data = vec![0u8; len];
    recv.read_exact(&mut data)
        .await
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;

    let archived = rkyv::access::<rkyv::Archived<MessageEnvelope>, rkyv::rancor::Error>(&data)
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;
    let envelope = rkyv::deserialize::<MessageEnvelope, rkyv::rancor::Error>(archived)
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;
    Ok(envelope)
}
