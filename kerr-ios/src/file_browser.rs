use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{
    KerrError, FileEntry, FileMetadata, MessageEnvelope, MessagePayload,
    ClientMessage, ServerMessage, SessionType, send_envelope, recv_envelope,
};

pub struct FileBrowser {
    send: Arc<Mutex<iroh::endpoint::SendStream>>,
    recv: Arc<Mutex<iroh::endpoint::RecvStream>>,
    session_id: String,
}

impl FileBrowser {
    pub async fn new(conn: Arc<iroh::endpoint::Connection>) -> Result<Arc<Self>, KerrError> {
        // Open a bidirectional stream
        let (mut send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| KerrError::ConnectionFailed(e.to_string()))?;

        // Generate session ID
        let session_id = format!("browser_{}", std::process::id());

        // Send Hello envelope for FileBrowser session
        let hello_envelope = MessageEnvelope {
            session_id: session_id.clone(),
            payload: MessagePayload::Client(ClientMessage::Hello {
                session_type: SessionType::FileBrowser,
            }),
        };

        send_envelope(&mut send, &hello_envelope).await?;

        Ok(Arc::new(Self {
            send: Arc::new(Mutex::new(send)),
            recv: Arc::new(Mutex::new(recv)),
            session_id,
        }))
    }

    pub fn list_dir(&self, path: String) -> Result<Vec<FileEntry>, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::ListDir { path }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;

            let response = recv_envelope(&mut *recv).await?;

            match response.payload {
                MessagePayload::Server(ServerMessage::DirListing { entries }) => {
                    Ok(entries.iter().map(FileEntry::from_remote).collect())
                }
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError(
                    "Unexpected response".to_string(),
                )),
            }
        })
    }

    pub fn metadata(&self, path: String) -> Result<FileMetadata, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::GetMetadata { path }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;

            let response = recv_envelope(&mut *recv).await?;

            match response.payload {
                MessagePayload::Server(ServerMessage::Metadata { metadata }) => {
                    Ok(FileMetadata::from_remote(&metadata))
                }
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError(
                    "Unexpected response".to_string(),
                )),
            }
        })
    }

    pub fn download_file(&self, path: String) -> Result<Vec<u8>, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::ReadFile { path }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;

            let response = recv_envelope(&mut *recv).await?;

            match response.payload {
                MessagePayload::Server(ServerMessage::FileContent { data }) => Ok(data),
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError(
                    "Unexpected response".to_string(),
                )),
            }
        })
    }

    pub fn upload_file(&self, path: String, data: Vec<u8>) -> Result<(), KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::WriteFile { path, data }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;

            let response = recv_envelope(&mut *recv).await?;

            match response.payload {
                MessagePayload::Server(ServerMessage::Success) => Ok(()),
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError(
                    "Unexpected response".to_string(),
                )),
            }
        })
    }

    pub fn delete(&self, path: String) -> Result<(), KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::DeleteFile { path }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;

            let response = recv_envelope(&mut *recv).await?;

            match response.payload {
                MessagePayload::Server(ServerMessage::Success) => Ok(()),
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError(
                    "Unexpected response".to_string(),
                )),
            }
        })
    }

    pub fn exists(&self, path: String) -> Result<bool, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::FileExists { path }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;

            let response = recv_envelope(&mut *recv).await?;

            match response.payload {
                MessagePayload::Server(ServerMessage::Exists { exists }) => Ok(exists),
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError(
                    "Unexpected response".to_string(),
                )),
            }
        })
    }
}
