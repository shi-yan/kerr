use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::collections::HashMap;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc, oneshot};
use crate::{
    KerrError, MessageEnvelope, MessagePayload, ClientMessage, ServerMessage,
    SessionType, send_envelope, recv_envelope,
};

// ---------------------------------------------------------------------------
// Stream ID counter — unique per TCP connection within a VPN session
// ---------------------------------------------------------------------------
static STREAM_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

// ---------------------------------------------------------------------------
// Per-connection events routed from the shared QUIC reader task
// ---------------------------------------------------------------------------
enum TcpConnEvent {
    Opened { success: bool, error: Option<String> },
    Data(Vec<u8>),
    Closed(Option<String>),
}

// ---------------------------------------------------------------------------
// VpnTunnel
//
// Starts a local SOCKS5 server that forwards TCP connections through the
// existing Iroh TcpRelay session (multiplexed over one QUIC bi-directional
// stream).  All SOCKS5 connections share that one QUIC stream; they are
// distinguished by the stream_id embedded in every TcpOpen/TcpData message.
// ---------------------------------------------------------------------------
pub struct VpnTunnel {
    socks_port: u16,
    /// Signals the accept-loop task to stop.
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
}

impl VpnTunnel {
    pub async fn new(
        conn: Arc<iroh::endpoint::Connection>,
        requested_port: u16,
        handle_udp: bool,  // reserved for future UDP/DNS relay support
    ) -> Result<Arc<Self>, KerrError> {
        let _ = handle_udp; // suppressed until UDP relay is implemented

        // ── Open one QUIC bi-directional stream for TcpRelay ──────────────
        let (mut quic_send, mut quic_recv) = conn
            .open_bi()
            .await
            .map_err(|e| KerrError::ConnectionFailed(format!("open_bi: {e}")))?;

        let session_id = format!("vpn_{}", std::process::id());

        // Send Hello so the server activates a TcpRelay session handler.
        send_envelope(
            &mut quic_send,
            &MessageEnvelope {
                session_id: session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::Hello {
                    session_type: SessionType::TcpRelay,
                }),
            },
        )
        .await?;

        // ── Bind the SOCKS5 TCP listener ──────────────────────────────────
        let bind_addr = format!("127.0.0.1:{}", requested_port);
        let tcp_listener = TcpListener::bind(&bind_addr)
            .await
            .map_err(|e| KerrError::NetworkError(format!("SOCKS5 bind({bind_addr}): {e}")))?;
        let socks_port = tcp_listener
            .local_addr()
            .map_err(|e| KerrError::NetworkError(e.to_string()))?
            .port();

        // ── Shared state ──────────────────────────────────────────────────
        // Mutex around the single QUIC SendStream — all SOCKS5 tasks share it.
        let quic_send = Arc::new(Mutex::new(quic_send));

        // Map: stream_id → channel sender for per-connection events.
        let conn_map: Arc<Mutex<HashMap<u32, mpsc::Sender<TcpConnEvent>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // ── QUIC reader task: routes server responses to SOCKS5 handlers ──
        {
            let conn_map = Arc::clone(&conn_map);
            let sid = session_id.clone();
            tokio::spawn(async move {
                loop {
                    match recv_envelope(&mut quic_recv).await {
                        Ok(env) => {
                            if env.session_id != sid {
                                continue;
                            }
                            dispatch_tcp_event(&conn_map, env.payload).await;
                        }
                        Err(e) => {
                            eprintln!("[vpn] QUIC recv error: {e}");
                            break;
                        }
                    }
                }
                // Drain all pending connections on QUIC error/close.
                let map = conn_map.lock().await;
                for tx in map.values() {
                    let _ = tx
                        .send(TcpConnEvent::Closed(Some(
                            "QUIC stream closed".into(),
                        )))
                        .await;
                }
            });
        }

        // ── SOCKS5 accept loop ────────────────────────────────────────────
        {
            let quic_send_clone = Arc::clone(&quic_send);
            let conn_map_clone = Arc::clone(&conn_map);
            let sid_clone = session_id.clone();
            tokio::spawn(async move {
                tokio::pin!(shutdown_rx);
                loop {
                    tokio::select! {
                        biased;
                        _ = &mut shutdown_rx => {
                            eprintln!("[vpn] SOCKS5 accept loop: shutdown received");
                            break;
                        }
                        result = tcp_listener.accept() => {
                            match result {
                                Ok((client, _)) => {
                                    let qs = Arc::clone(&quic_send_clone);
                                    let cm = Arc::clone(&conn_map_clone);
                                    let sid = sid_clone.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) = socks5_handle(client, qs, cm, sid).await {
                                            eprintln!("[vpn] SOCKS5 conn error: {e}");
                                        }
                                    });
                                }
                                Err(e) => {
                                    eprintln!("[vpn] accept error: {e}");
                                    break;
                                }
                            }
                        }
                    }
                }
            });
        }

        Ok(Arc::new(Self {
            socks_port,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
        }))
    }

    /// Returns the port the SOCKS5 server is listening on.
    pub fn get_socks_port(&self) -> u16 {
        self.socks_port
    }

    /// Gracefully stops the SOCKS5 server and closes the QUIC relay stream.
    pub fn stop(&self) {
        let rt = crate::get_runtime();
        rt.block_on(async {
            if let Some(tx) = self.shutdown_tx.lock().await.take() {
                let _ = tx.send(());
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Route a server message payload to the appropriate per-connection channel.
async fn dispatch_tcp_event(
    conn_map: &Arc<Mutex<HashMap<u32, mpsc::Sender<TcpConnEvent>>>>,
    payload: MessagePayload,
) {
    match payload {
        MessagePayload::Server(ServerMessage::TcpOpenResponse {
            stream_id,
            success,
            error,
        }) => {
            let map = conn_map.lock().await;
            if let Some(tx) = map.get(&stream_id) {
                let _ = tx.send(TcpConnEvent::Opened { success, error }).await;
            }
        }
        MessagePayload::Server(ServerMessage::TcpDataResponse { stream_id, data }) => {
            let map = conn_map.lock().await;
            if let Some(tx) = map.get(&stream_id) {
                let _ = tx.send(TcpConnEvent::Data(data)).await;
            }
        }
        MessagePayload::Server(ServerMessage::TcpCloseResponse { stream_id, error }) => {
            let mut map = conn_map.lock().await;
            if let Some(tx) = map.get(&stream_id) {
                let _ = tx.send(TcpConnEvent::Closed(error)).await;
            }
            map.remove(&stream_id);
        }
        _ => {}
    }
}

/// Top-level handler for one accepted SOCKS5 TCP connection.
async fn socks5_handle(
    client: TcpStream,
    quic_send: Arc<Mutex<iroh::endpoint::SendStream>>,
    conn_map: Arc<Mutex<HashMap<u32, mpsc::Sender<TcpConnEvent>>>>,
    session_id: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    socks5_negotiate(client, quic_send, conn_map, session_id).await
}

/// Perform SOCKS5 handshake, then dispatch based on the requested command.
async fn socks5_negotiate(
    mut client: TcpStream,
    quic_send: Arc<Mutex<iroh::endpoint::SendStream>>,
    conn_map: Arc<Mutex<HashMap<u32, mpsc::Sender<TcpConnEvent>>>>,
    session_id: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // ── Greeting ─────────────────────────────────────────────────────────
    let mut greeting = [0u8; 2];
    client.read_exact(&mut greeting).await?;
    if greeting[0] != 0x05 {
        return Err(format!("Not SOCKS5 (VER={})", greeting[0]).into());
    }
    let n_methods = greeting[1] as usize;
    let mut _methods = vec![0u8; n_methods];
    client.read_exact(&mut _methods).await?;
    // Always choose NO_AUTH (0x00)
    client.write_all(&[0x05, 0x00]).await?;

    // ── Request ───────────────────────────────────────────────────────────
    let mut req = [0u8; 4];
    client.read_exact(&mut req).await?;
    if req[0] != 0x05 {
        return Err("Bad SOCKS5 version in request".into());
    }
    let cmd = req[1];
    let atyp = req[3];

    let dest_host: String = match atyp {
        0x01 => {
            // IPv4
            let mut ip = [0u8; 4];
            client.read_exact(&mut ip).await?;
            format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
        }
        0x03 => {
            // Domain name
            let mut len = [0u8; 1];
            client.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            client.read_exact(&mut domain).await?;
            String::from_utf8(domain)?
        }
        0x04 => {
            // IPv6
            let mut ip = [0u8; 16];
            client.read_exact(&mut ip).await?;
            let segs: Vec<String> = ip
                .chunks(2)
                .map(|c| format!("{:02x}{:02x}", c[0], c[1]))
                .collect();
            segs.join(":")
        }
        _ => return Err(format!("Unknown ATYP: {atyp}").into()),
    };

    let mut port_bytes = [0u8; 2];
    client.read_exact(&mut port_bytes).await?;
    let dest_port = u16::from_be_bytes(port_bytes);

    match cmd {
        0x01 => {
            // CONNECT
            socks5_connect(client, quic_send, conn_map, session_id, dest_host, dest_port).await
        }
        _ => {
            // Command not supported (0x07)
            client
                .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            Err(format!("Unsupported SOCKS5 command: {cmd}").into())
        }
    }
}

/// Handle a SOCKS5 CONNECT request: open TcpRelay, relay data both ways.
async fn socks5_connect(
    mut client: TcpStream,
    quic_send: Arc<Mutex<iroh::endpoint::SendStream>>,
    conn_map: Arc<Mutex<HashMap<u32, mpsc::Sender<TcpConnEvent>>>>,
    session_id: String,
    dest_host: String,
    dest_port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stream_id = STREAM_ID_COUNTER.fetch_add(1, Ordering::Relaxed);

    // Register the per-connection event channel before sending TcpOpen so we
    // never miss the TcpOpenResponse that the reader task may deliver.
    let (event_tx, mut event_rx) = mpsc::channel::<TcpConnEvent>(64);
    conn_map.lock().await.insert(stream_id, event_tx);

    // ── Send TcpOpen ──────────────────────────────────────────────────────
    {
        let mut send = quic_send.lock().await;
        send_envelope(
            &mut *send,
            &MessageEnvelope {
                session_id: session_id.clone(),
                payload: MessagePayload::Client(ClientMessage::TcpOpen {
                    stream_id,
                    destination_host: Some(dest_host.clone()),
                    destination_port: dest_port,
                }),
            },
        )
        .await
        .map_err(|e| format!("TcpOpen send: {e}"))?;
    }

    // ── Wait for TcpOpenResponse (30 s timeout) ───────────────────────────
    let opened = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        event_rx.recv(),
    )
    .await
    .map_err(|_| "TcpOpen timed out after 30 s")?
    .ok_or("event channel closed before TcpOpenResponse")?;

    match opened {
        TcpConnEvent::Opened { success: true, .. } => {
            // SOCKS5 success reply: VER REP RSV ATYP BND.ADDR BND.PORT
            client
                .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
        }
        TcpConnEvent::Opened { success: false, error } => {
            // Connection refused (REP=0x05)
            let _ = client
                .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
            conn_map.lock().await.remove(&stream_id);
            return Err(format!("TcpOpen rejected: {:?}", error).into());
        }
        TcpConnEvent::Closed(e) => {
            let _ = client
                .write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
            conn_map.lock().await.remove(&stream_id);
            return Err(format!("Connection closed before open completed: {:?}", e).into());
        }
        TcpConnEvent::Data(_) => {
            conn_map.lock().await.remove(&stream_id);
            return Err("Unexpected Data before TcpOpenResponse".into());
        }
    }

    // ── Relay data bidirectionally ─────────────────────────────────────────
    let (mut client_rd, mut client_wr) = client.into_split();
    let quic_send_c2q = Arc::clone(&quic_send);
    let session_id_c2q = session_id.clone();
    let conn_map_c2q = Arc::clone(&conn_map);

    // Task: client → QUIC (forward client bytes as TcpData)
    let c2q = tokio::spawn(async move {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            match client_rd.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let mut send = quic_send_c2q.lock().await;
                    if send_envelope(
                        &mut *send,
                        &MessageEnvelope {
                            session_id: session_id_c2q.clone(),
                            payload: MessagePayload::Client(ClientMessage::TcpData {
                                stream_id,
                                data: buf[..n].to_vec(),
                            }),
                        },
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        // Signal server that this stream is finished.
        let mut send = quic_send_c2q.lock().await;
        let _ = send_envelope(
            &mut *send,
            &MessageEnvelope {
                session_id: session_id_c2q,
                payload: MessagePayload::Client(ClientMessage::TcpClose { stream_id }),
            },
        )
        .await;
        conn_map_c2q.lock().await.remove(&stream_id);
    });

    // This task: QUIC → client (forward TcpDataResponse bytes to client)
    while let Some(event) = event_rx.recv().await {
        match event {
            TcpConnEvent::Data(data) => {
                if client_wr.write_all(&data).await.is_err() {
                    break;
                }
            }
            TcpConnEvent::Closed(_) => break,
            TcpConnEvent::Opened { .. } => {} // shouldn't occur here
        }
    }

    c2q.abort();
    conn_map.lock().await.remove(&stream_id);
    Ok(())
}
