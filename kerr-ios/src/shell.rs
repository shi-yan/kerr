use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{
    KerrError, MessageEnvelope, MessagePayload, ClientMessage, ServerMessage,
    SessionType, send_envelope, recv_envelope,
};

// Callback trait for shell output - will be implemented in Swift
pub trait ShellCallback: Send + Sync {
    fn on_output(&self, data: String);
    fn on_error(&self, message: String);
    fn on_close(&self);
}

pub struct ShellSession {
    send: Arc<Mutex<iroh::endpoint::SendStream>>,
    recv: Arc<Mutex<iroh::endpoint::RecvStream>>,
    session_id: String,
    callback: Arc<Box<dyn ShellCallback>>,
    receiver_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl ShellSession {
    pub async fn new(
        conn: Arc<iroh::endpoint::Connection>,
        callback: Box<dyn ShellCallback>,
    ) -> Result<Arc<Self>, KerrError> {
        // Open a bidirectional stream
        let (mut send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| KerrError::ConnectionFailed(e.to_string()))?;

        // Generate session ID
        let session_id = format!("shell_{}", std::process::id());

        // Send Hello envelope for Shell session
        let hello_envelope = MessageEnvelope {
            session_id: session_id.clone(),
            payload: MessagePayload::Client(ClientMessage::Hello {
                session_type: SessionType::Shell,
            }),
        };

        send_envelope(&mut send, &hello_envelope).await?;

        let callback = Arc::new(callback);
        let recv_shared = Arc::new(Mutex::new(recv));

        // Spawn background task to receive shell output
        let receiver_task = {
            let recv_clone = Arc::clone(&recv_shared);
            let callback_clone = Arc::clone(&callback);
            let session_id_clone = session_id.clone();

            tokio::spawn(async move {
                Self::receive_loop(recv_clone, callback_clone, session_id_clone).await;
            })
        };

        Ok(Arc::new(Self {
            send: Arc::new(Mutex::new(send)),
            recv: recv_shared,
            session_id,
            callback,
            receiver_task: Arc::new(Mutex::new(Some(receiver_task))),
        }))
    }

    async fn receive_loop(
        recv: Arc<Mutex<iroh::endpoint::RecvStream>>,
        callback: Arc<Box<dyn ShellCallback>>,
        session_id: String,
    ) {
        loop {
            let mut recv_guard = recv.lock().await;
            let result = recv_envelope(&mut *recv_guard).await;
            drop(recv_guard);

            match result {
                Ok(envelope) => {
                    if envelope.session_id != session_id {
                        continue;
                    }

                    match envelope.payload {
                        MessagePayload::Server(ServerMessage::Output { data }) => {
                            let text = String::from_utf8_lossy(&data).to_string();
                            callback.on_output(text);
                        }
                        MessagePayload::Server(ServerMessage::Error { message }) => {
                            callback.on_error(message);
                        }
                        _ => {}
                    }
                }
                Err(_) => {
                    callback.on_close();
                    break;
                }
            }
        }
    }

    pub fn send_input(&self, data: String) -> Result<(), KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::Input {
                    data: data.into_bytes(),
                }),
            };

            let mut send = self.send.lock().await;
            send_envelope(&mut *send, &envelope).await
        })
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), KerrError> {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            let envelope = MessageEnvelope {
                session_id: self.session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::Resize { cols, rows }),
            };

            let mut send = self.send.lock().await;
            send_envelope(&mut *send, &envelope).await
        })
    }

    pub fn close(&self) {
        let runtime = crate::get_runtime();
        runtime.block_on(async {
            // Abort the receiver task
            if let Some(task) = self.receiver_task.lock().await.take() {
                task.abort();
            }

            // Notify callback
            self.callback.on_close();
        });
    }
}
