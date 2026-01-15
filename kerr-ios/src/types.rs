use std::fmt;

// Error type exposed to Swift via UniFFI
#[derive(Debug, thiserror::Error)]
pub enum KerrError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Invalid connection string")]
    InvalidConnectionString,
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

impl FileMetadata {
    pub fn from_remote(remote: &crate::RemoteFileMetadata) -> Self {
        Self {
            size: remote.size,
            created_timestamp: remote.created.map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            }),
            modified_timestamp: remote.modified.map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            }),
            is_dir: remote.is_dir,
        }
    }
}

impl FileEntry {
    pub fn from_remote(remote: &crate::RemoteFileEntry) -> Self {
        Self {
            name: remote.name.clone(),
            path: remote.path.clone(),
            is_dir: remote.is_dir,
            is_hidden: remote.is_hidden,
            metadata: remote.metadata.as_ref().map(FileMetadata::from_remote),
        }
    }
}
