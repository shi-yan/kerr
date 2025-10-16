use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use super::file_explorer::FileMetadata;

/// Represents a file or directory entry in a filesystem
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub metadata: Option<FileMetadata>,
}

/// Trait for filesystem operations that can be implemented for local or remote filesystems
#[async_trait::async_trait]
pub trait Filesystem: Send + Sync {
    /// List entries in a directory
    async fn read_dir(&self, path: &Path) -> io::Result<Vec<FileEntry>>;

    /// Get metadata for a specific file or directory
    async fn metadata(&self, path: &Path) -> io::Result<FileMetadata>;

    /// Check if a path is a directory
    async fn is_dir(&self, path: &Path) -> io::Result<bool>;

    /// Check if a path exists
    async fn exists(&self, path: &Path) -> io::Result<bool>;

    /// Get the parent directory of a path
    fn parent(&self, path: &Path) -> Option<PathBuf> {
        path.parent().map(|p| p.to_path_buf())
    }

    /// Read file content as bytes
    async fn read_file(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Read file content as string (UTF-8)
    async fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let bytes = self.read_file(path).await?;
        String::from_utf8(bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Get the current working directory (for local filesystem)
    /// For remote filesystem, this might return a default root path
    fn current_dir(&self) -> io::Result<PathBuf>;
}

/// Local filesystem implementation
pub struct LocalFilesystem;

impl LocalFilesystem {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Filesystem for LocalFilesystem {
    async fn read_dir(&self, path: &Path) -> io::Result<Vec<FileEntry>> {
        let mut entries = Vec::new();

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file name"))?;

            let metadata = std::fs::metadata(&path)?;
            let file_type = metadata.file_type();
            let is_dir = file_type.is_dir();

            let file_metadata = FileMetadata {
                size: metadata.len(),
                created: metadata.created().ok(),
                modified: metadata.modified().ok(),
                is_dir,
            };

            #[cfg(unix)]
            let is_hidden = file_name.starts_with('.');

            #[cfg(windows)]
            let is_hidden = {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
                (metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN) != 0
            };

            #[cfg(not(any(unix, windows)))]
            let is_hidden = false;

            let name = if is_dir {
                format!("{}/", file_name)
            } else {
                file_name.to_string()
            };

            entries.push(FileEntry {
                name,
                path: path.clone(),
                is_dir,
                is_hidden,
                metadata: Some(file_metadata),
            });
        }

        Ok(entries)
    }

    async fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        let metadata = std::fs::metadata(path)?;
        Ok(FileMetadata {
            size: metadata.len(),
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
            is_dir: metadata.is_dir(),
        })
    }

    async fn is_dir(&self, path: &Path) -> io::Result<bool> {
        Ok(std::fs::metadata(path)?.is_dir())
    }

    async fn exists(&self, path: &Path) -> io::Result<bool> {
        Ok(path.exists())
    }

    async fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    async fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn current_dir(&self) -> io::Result<PathBuf> {
        std::env::current_dir()
    }
}

/// Remote filesystem implementation via p2p connection
pub struct RemoteFilesystem {
    root_path: PathBuf,
    send: Arc<tokio::sync::Mutex<iroh::endpoint::SendStream>>,
    recv: Arc<tokio::sync::Mutex<iroh::endpoint::RecvStream>>,
}

impl RemoteFilesystem {
    pub fn new(
        root_path: PathBuf,
        send: iroh::endpoint::SendStream,
        recv: iroh::endpoint::RecvStream,
    ) -> Self {
        Self {
            root_path,
            send: Arc::new(tokio::sync::Mutex::new(send)),
            recv: Arc::new(tokio::sync::Mutex::new(recv)),
        }
    }

    async fn send_request(&self, msg: crate::ClientMessage) -> io::Result<crate::ServerMessage> {
        let config = bincode::config::standard();

        // Serialize and send request
        let encoded = bincode::encode_to_vec(&msg, config)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let len = (encoded.len() as u32).to_be_bytes();

        let mut send: tokio::sync::MutexGuard<iroh::endpoint::SendStream> = self.send.lock().await;
        send.write_all(&len).await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        send.write_all(&encoded).await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        drop(send);

        // Read response
        let mut recv: tokio::sync::MutexGuard<iroh::endpoint::RecvStream> = self.recv.lock().await;

        let mut len_bytes = [0u8; 4];
        recv.read_exact(&mut len_bytes).await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let msg_len = u32::from_be_bytes(len_bytes) as usize;

        let mut msg_bytes = vec![0u8; msg_len];
        recv.read_exact(&mut msg_bytes).await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        drop(recv);

        // Deserialize response
        let (response, _): (crate::ServerMessage, _) = bincode::decode_from_slice(&msg_bytes, config)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(response)
    }
}

#[async_trait::async_trait]
impl Filesystem for RemoteFilesystem {
    async fn read_dir(&self, path: &Path) -> io::Result<Vec<FileEntry>> {
        let msg = crate::ClientMessage::FsReadDir {
            path: path.display().to_string(),
        };

        match self.send_request(msg).await? {
            crate::ServerMessage::FsDirListing { entries_json } => {
                serde_json::from_str(&entries_json)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            }
            crate::ServerMessage::Error { message } => {
                Err(io::Error::new(io::ErrorKind::Other, message))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Other,
                "Unexpected response type",
            )),
        }
    }

    async fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        let msg = crate::ClientMessage::FsMetadata {
            path: path.display().to_string(),
        };

        match self.send_request(msg).await? {
            crate::ServerMessage::FsMetadataResponse { metadata_json } => {
                serde_json::from_str(&metadata_json)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            }
            crate::ServerMessage::Error { message } => {
                Err(io::Error::new(io::ErrorKind::Other, message))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Other,
                "Unexpected response type",
            )),
        }
    }

    async fn is_dir(&self, path: &Path) -> io::Result<bool> {
        let metadata = self.metadata(path).await?;
        Ok(metadata.is_dir)
    }

    async fn exists(&self, path: &Path) -> io::Result<bool> {
        match self.metadata(path).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        let msg = crate::ClientMessage::FsReadFile {
            path: path.display().to_string(),
        };

        match self.send_request(msg).await? {
            crate::ServerMessage::FsFileContent { data } => Ok(data),
            crate::ServerMessage::Error { message } => {
                Err(io::Error::new(io::ErrorKind::Other, message))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Other,
                "Unexpected response type",
            )),
        }
    }

    fn current_dir(&self) -> io::Result<PathBuf> {
        Ok(self.root_path.clone())
    }
}
