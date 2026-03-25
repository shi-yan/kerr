
// Error type exposed to Swift via UniFFI
#[derive(Debug, thiserror::Error)]
pub enum KerrError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Invalid connection string: {0}")]
    InvalidConnectionString(String),
    #[error("File system error: {0}")]
    FileSystemError(String),
    #[error("Shell error: {0}")]
    ShellError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Operation timed out")]
    Timeout,
}

// Implement conversion from anyhow::Error
impl From<anyhow::Error> for KerrError {
    fn from(err: anyhow::Error) -> Self {
        KerrError::NetworkError(err.to_string())
    }
}

// Implement conversion from std::io::Error
impl From<std::io::Error> for KerrError {
    fn from(err: std::io::Error) -> Self {
        KerrError::NetworkError(err.to_string())
    }
}

// File entry for Swift
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub metadata: Option<FileMetadata>,
}

// File metadata for Swift
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: u64,
    pub created_timestamp: Option<u64>,  // Unix timestamp in seconds
    pub modified_timestamp: Option<u64>, // Unix timestamp in seconds
    pub is_dir: bool,
}

// ---- JSON deserialization types matching server's output ----
// The server serializes std::time::SystemTime as { secs_since_epoch, nanos_since_epoch }

#[derive(serde::Deserialize)]
pub(crate) struct JsonSystemTime {
    pub secs_since_epoch: u64,
    #[allow(dead_code)]
    pub nanos_since_epoch: u32,
}

#[derive(serde::Deserialize)]
pub(crate) struct JsonFileMetadata {
    pub size: u64,
    pub created: Option<JsonSystemTime>,
    pub modified: Option<JsonSystemTime>,
    pub is_dir: bool,
}

// Server's FileEntry has `path: PathBuf` which serde serializes as a string
#[derive(serde::Deserialize)]
pub(crate) struct JsonFileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub metadata: Option<JsonFileMetadata>,
}

impl FileMetadata {
    pub(crate) fn from_json(json: &JsonFileMetadata) -> Self {
        Self {
            size: json.size,
            created_timestamp: json.created.as_ref().map(|t| t.secs_since_epoch),
            modified_timestamp: json.modified.as_ref().map(|t| t.secs_since_epoch),
            is_dir: json.is_dir,
        }
    }
}

impl FileEntry {
    pub(crate) fn from_json(json: &JsonFileEntry) -> Self {
        Self {
            name: json.name.clone(),
            path: json.path.clone(),
            is_dir: json.is_dir,
            is_hidden: json.is_hidden,
            metadata: json.metadata.as_ref().map(FileMetadata::from_json),
        }
    }
}

pub(crate) fn parse_entries(entries_json: &str) -> Result<Vec<FileEntry>, KerrError> {
    let json_entries: Vec<JsonFileEntry> = serde_json::from_str(entries_json)
        .map_err(|e| KerrError::FileSystemError(format!("Failed to parse entries: {}", e)))?;
    Ok(json_entries.iter().map(FileEntry::from_json).collect())
}

pub(crate) fn parse_metadata(metadata_json: &str) -> Result<FileMetadata, KerrError> {
    let json_meta: JsonFileMetadata = serde_json::from_str(metadata_json)
        .map_err(|e| KerrError::FileSystemError(format!("Failed to parse metadata: {}", e)))?;
    Ok(FileMetadata::from_json(&json_meta))
}
