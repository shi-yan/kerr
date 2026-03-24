use std::sync::Arc;
use anyhow::Result;

// Include the kerr core types
// We'll need to reference the parent crate
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

mod types;
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
    // Decode base64
    let compressed = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        conn_str.trim(),
    )
    .map_err(|e| KerrError::InvalidConnectionString)?;

    // Decompress gzip
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(&compressed[..]);
    let mut json_str = String::new();
    decoder
        .read_to_string(&mut json_str)
        .map_err(|_| KerrError::InvalidConnectionString)?;

    // Deserialize
    let addr: iroh::EndpointAddr = serde_json::from_str(&json_str)
        .map_err(|_| KerrError::InvalidConnectionString)?;

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

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &compressed);
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
    Input { data: Vec<u8> },
    Resize { cols: u16, rows: u16 },
    ListDir { path: String },
    ReadFile { path: String },
    WriteFile { path: String, data: Vec<u8> },
    DeleteFile { path: String },
    GetMetadata { path: String },
    FileExists { path: String },
}

#[derive(Debug, Archive, RkyvSerialize, RkyvDeserialize)]
#[rkyv(derive(Debug))]
pub enum ServerMessage {
    Output { data: Vec<u8> },
    Error { message: String },
    DirListing { entries: Vec<RemoteFileEntry> },
    FileContent { data: Vec<u8> },
    Metadata { metadata: RemoteFileMetadata },
    Success,
    Exists { exists: bool },
}

#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[rkyv(derive(Debug))]
pub struct RemoteFileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub metadata: Option<RemoteFileMetadata>,
}

#[derive(Debug, Clone, Archive, RkyvSerialize, RkyvDeserialize)]
#[rkyv(derive(Debug))]
pub struct RemoteFileMetadata {
    pub size: u64,
    pub created: Option<u64>,   // Unix timestamp in seconds
    pub modified: Option<u64>,  // Unix timestamp in seconds
    pub is_dir: bool,
}

// Helper to send envelope
async fn send_envelope(
    send: &mut iroh::endpoint::SendStream,
    envelope: &MessageEnvelope,
) -> Result<(), KerrError> {
    let data = rkyv::to_bytes::<rkyv::rancor::Error>(envelope)
        .map_err(|e| KerrError::NetworkError(e.to_string()))?;
    let len = data.len() as u32;
    send.write_all(&len.to_le_bytes())
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
    let len = u32::from_le_bytes(len_bytes) as usize;

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
