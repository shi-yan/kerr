use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{
    KerrError, FileEntry, FileMetadata, MessageEnvelope, MessagePayload,
    ClientMessage, ServerMessage, SessionType, send_envelope, recv_envelope,
};
use crate::types::{parse_entries, parse_metadata};

pub struct FileBrowser {
    send: Arc<Mutex<iroh::endpoint::SendStream>>,
    recv: Arc<Mutex<iroh::endpoint::RecvStream>>,
    session_id: String,
}

impl FileBrowser {
    pub async fn new(conn: Arc<iroh::endpoint::Connection>) -> Result<Arc<Self>, KerrError> {
        let (mut send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| KerrError::ConnectionFailed(e.to_string()))?;

        let session_id = format!("browser_{}", std::process::id());

        let hello_envelope = MessageEnvelope {
            session_id: session_id.clone(),
            payload: MessagePayload::Client(ClientMessage::Hello {
                session_type: SessionType::FileBrowser,
            }),
        };

        eprintln!("[kerr] FileBrowser::new: sending Hello");
        send_envelope(&mut send, &hello_envelope).await?;
        eprintln!("[kerr] FileBrowser::new: Hello sent");

        Ok(Arc::new(Self {
            send: Arc::new(Mutex::new(send)),
            recv: Arc::new(Mutex::new(recv)),
            session_id,
        }))
    }

    pub fn list_dir(&self, path: String) -> Result<Vec<FileEntry>, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            eprintln!("[kerr] list_dir: path={}", path);
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::FsReadDir { path }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;
            eprintln!("[kerr] list_dir: request sent, waiting for response");

            let response = recv_envelope(&mut *recv).await?;
            eprintln!("[kerr] list_dir: got response");

            match response.payload {
                MessagePayload::Server(ServerMessage::FsDirListing { entries_json }) => {
                    eprintln!("[kerr] list_dir: got FsDirListing, parsing {} bytes", entries_json.len());
                    parse_entries(&entries_json)
                }
                MessagePayload::Server(ServerMessage::FsError { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError("Unexpected response".to_string())),
            }
        })
    }

    pub fn metadata(&self, path: String) -> Result<FileMetadata, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::FsMetadata { path }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;

            let response = recv_envelope(&mut *recv).await?;

            match response.payload {
                MessagePayload::Server(ServerMessage::FsMetadataResponse { metadata_json }) => {
                    parse_metadata(&metadata_json)
                }
                MessagePayload::Server(ServerMessage::FsError { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError("Unexpected response".to_string())),
            }
        })
    }

    pub fn download_file(&self, path: String) -> Result<Vec<u8>, KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::FsReadFile { path }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;

            let response = recv_envelope(&mut *recv).await?;

            match response.payload {
                MessagePayload::Server(ServerMessage::FsFileContent { data }) => Ok(data),
                MessagePayload::Server(ServerMessage::FsError { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError("Unexpected response".to_string())),
            }
        })
    }

    pub fn upload_file(&self, path: String, data: Vec<u8>) -> Result<(), KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let size = data.len() as u64;

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            // StartUpload
            let start_envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::StartUpload {
                    path,
                    size,
                    is_dir: false,
                    force: true,
                }),
            };
            send_envelope(&mut *send, &start_envelope).await?;

            let ack = recv_envelope(&mut *recv).await?;
            match ack.payload {
                MessagePayload::Server(ServerMessage::UploadAck) => {}
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    return Err(KerrError::FileSystemError(message));
                }
                _ => return Err(KerrError::FileSystemError("Expected UploadAck".to_string())),
            }

            // FileChunk (send all data as one chunk)
            let chunk_envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::FileChunk { data }),
            };
            send_envelope(&mut *send, &chunk_envelope).await?;

            // EndUpload
            let end_envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::EndUpload),
            };
            send_envelope(&mut *send, &end_envelope).await?;

            Ok(())
        })
    }

    pub fn delete(&self, path: String) -> Result<(), KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::FsDelete { path }),
            };

            let mut send = self.send.lock().await;
            let mut recv = self.recv.lock().await;

            send_envelope(&mut *send, &envelope).await?;

            let response = recv_envelope(&mut *recv).await?;

            match response.payload {
                MessagePayload::Server(ServerMessage::FsDeleteResponse { success }) => {
                    if success {
                        Ok(())
                    } else {
                        Err(KerrError::FileSystemError("Delete failed".to_string()))
                    }
                }
                MessagePayload::Server(ServerMessage::FsError { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                MessagePayload::Server(ServerMessage::Error { message }) => {
                    Err(KerrError::FileSystemError(message))
                }
                _ => Err(KerrError::FileSystemError("Unexpected response".to_string())),
            }
        })
    }

    pub fn exists(&self, path: String) -> Result<bool, KerrError> {
        // Use FsMetadata — if it succeeds, the file exists; if FsError, it doesn't
        match self.metadata(path) {
            Ok(_) => Ok(true),
            Err(KerrError::FileSystemError(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}
