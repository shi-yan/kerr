use anyhow::Result;
use axum::{
    body::Body,
    extract::{ws::{WebSocket, WebSocketUpgrade, Message}, Multipart, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use base64::Engine;
use futures::{sink::SinkExt, stream::StreamExt};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::custom_explorer::filesystem::{Filesystem, RemoteFilesystem};
use crate::debug_log;
use std::collections::HashMap;
use tokio::sync::mpsc;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Asset;

/// Port forwarding session
struct PortForwardingSession {
    id: String,
    name: Option<String>,
    local_port: u16,
    remote_port: u16,
    stop_tx: mpsc::UnboundedSender<()>,
}

/// Shared state for the web UI
struct AppState {
    remote_fs: Arc<Mutex<Option<Arc<RemoteFilesystem>>>>,
    endpoint: Arc<iroh::endpoint::Endpoint>,
    node_addr: Arc<Mutex<Option<iroh::EndpointAddr>>>,
    connection: Arc<Mutex<Option<Arc<iroh::endpoint::Connection>>>>,
    connection_string: Arc<Mutex<Option<String>>>,
    connection_alias: Arc<Mutex<Option<String>>>,
    port_forwardings: Arc<Mutex<HashMap<String, PortForwardingSession>>>,
}

/// Run the web UI server
pub async fn run_web_ui(connection_string: Option<String>, port: u16) -> Result<()> {
    // Create endpoint for future connections
    let endpoint = iroh::endpoint::Endpoint::bind().await?;

    // If connection string is provided, connect immediately
    let (node_addr, connection, remote_fs, conn_str_stored, conn_alias) = if let Some(conn_str) = connection_string {
        println!("Connecting to remote host...");
        let addr = crate::decode_connection_string(&conn_str)
            .map_err(|e| anyhow::anyhow!("Failed to decode connection string: {}", e))?;
        let (conn, fs) = connect_to_remote(&endpoint, &addr).await?;
        println!("Connected! Setting up file browser session...");
        (Some(addr), Some(Arc::new(conn)), Some(Arc::new(fs)), Some(conn_str), None)
    } else {
        println!("Starting UI in connection selection mode...");
        (None, None, None, None, None)
    };

    // Create application state
    let state = Arc::new(AppState {
        remote_fs: Arc::new(Mutex::new(remote_fs)),
        endpoint: Arc::new(endpoint),
        node_addr: Arc::new(Mutex::new(node_addr)),
        connection: Arc::new(Mutex::new(connection)),
        connection_string: Arc::new(Mutex::new(conn_str_stored)),
        connection_alias: Arc::new(Mutex::new(conn_alias)),
        port_forwardings: Arc::new(Mutex::new(HashMap::new())),
    });

    // Build our application router
    let app = Router::new()
        .route("/api/auth/session", get(check_session))
        .route("/api/auth/login", get(initiate_login))
        .route("/api/auth/callback", get(handle_oauth_callback))
        .route("/api/connection/status", get(connection_status))
        .route("/api/connection/list", get(list_connections))
        .route("/api/connection/connect", post(connect_to_connection))
        .route("/api/connection/disconnect", post(disconnect_connection))
        .route("/ws/shell", get(websocket_handler))
        .route("/api/files", get(list_files))
        .route("/api/files/download", get(download_file))
        .route("/api/files/upload", post(upload_file))
        .route("/api/file/content", get(read_file))
        .route("/api/file/content", post(write_file))
        .route("/api/file/metadata", get(get_metadata))
        .route("/api/file/delete", delete(delete_file))
        .route("/api/port-forward/create", post(create_port_forward))
        .route("/api/port-forward/disconnect", post(disconnect_port_forward))
        .fallback(static_handler)
        .with_state(state);

    // Start the server
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("Web UI server running at http://{}", addr);
    println!("Open your browser to access the UI");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Connect to a remote host using single-stream multiplexing
async fn connect_to_remote(
    endpoint: &iroh::endpoint::Endpoint,
    addr: &iroh::EndpointAddr,
) -> Result<(iroh::endpoint::Connection, RemoteFilesystem)> {
    eprintln!("[CONNECT] Connecting to remote host (single-stream mode)...");
    // Connect to the remote host
    let conn = endpoint.connect(addr.clone(), crate::ALPN).await?;
    eprintln!("[CONNECT] Connection established!");

    eprintln!("[CONNECT] Opening single bidirectional stream for multiplexing...");
    // Open ONE bidirectional stream that will handle all sessions
    let (mut send, recv) = conn.open_bi().await?;
    eprintln!("[CONNECT] Bidirectional stream opened!");

    // Send Hello envelope for FileBrowser session
    eprintln!("[CONNECT] Sending Hello envelope for FileBrowser session...");
    let hello_envelope = crate::MessageEnvelope {
        session_id: "browser_1".to_string(),
        payload: crate::MessagePayload::Client(crate::ClientMessage::Hello {
            session_type: crate::SessionType::FileBrowser,
        }),
    };
    crate::send_envelope(&mut send, &hello_envelope).await
        .map_err(|e| anyhow::anyhow!("Failed to send Hello envelope: {}", e))?;
    eprintln!("[CONNECT] Hello envelope sent!");

    // Create remote filesystem with session_id
    eprintln!("[CONNECT] Creating RemoteFilesystem with multiplexed stream...");
    let remote_fs = RemoteFilesystem::new_with_session_id(
        PathBuf::from("/"),
        send,
        recv,
        "browser_1".to_string(),
    );
    eprintln!("[CONNECT] RemoteFilesystem created successfully!");

    Ok((conn, remote_fs))
}

/// Serve static files from embedded assets
async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // If path is empty, serve index.html
    let path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path
    };

    match Asset::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.to_vec()))
                .unwrap()
        }
        None => {
            // For SPA routing, serve index.html for non-API routes
            if !path.starts_with("api/") {
                if let Some(index) = Asset::get("index.html") {
                    return Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/html")
                        .body(Body::from(index.data.to_vec()))
                        .unwrap();
                }
            }
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("404 Not Found"))
                .unwrap()
        }
    }
}

/// Check if user is logged in (has valid session)
async fn check_session() -> Json<SessionCheckResponse> {
    let has_session = crate::auth::load_session().is_ok();
    Json(SessionCheckResponse {
        logged_in: has_session,
    })
}

#[derive(Serialize)]
struct SessionCheckResponse {
    logged_in: bool,
}

/// Initiate Google OAuth login by redirecting to Google
async fn initiate_login(Query(params): Query<LoginParams>) -> Result<Response, (StatusCode, String)> {
    use rand::Rng;

    // Generate state token for CSRF protection
    let mut rng = rand::rng();
    let token: [u8; 32] = rng.random();
    let state = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &token);

    // Build redirect URI - use the port from the request
    let port = params.port.unwrap_or(3000);
    let redirect_uri = format!("http://127.0.0.1:{}/api/auth/callback", port);

    // Build Google OAuth URL using url crate for proper encoding
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to build URL: {}", e)))?;

    let client_id = "402230363945-pmkrgrkkashlcdkf8oso0pptneioqn2o.apps.googleusercontent.com";
    let scope = "email profile";

    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", scope)
        .append_pair("state", &state);

    let auth_url = url.to_string();

    // Redirect to Google
    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, auth_url)
        .body(Body::empty())
        .unwrap())
}

#[derive(Deserialize)]
struct LoginParams {
    port: Option<u16>,
}

/// Handle OAuth callback from Google
async fn handle_oauth_callback(Query(params): Query<CallbackParams>) -> Result<Response, (StatusCode, String)> {
    // Extract auth code from query params
    let auth_code = params.code.ok_or_else(|| {
        (StatusCode::BAD_REQUEST, "Missing authorization code".to_string())
    })?;

    // Get port from state or default
    let port = params.port.unwrap_or(3000);
    let redirect_uri = format!("http://127.0.0.1:{}/api/auth/callback", port);

    // Exchange code with backend server
    let client = reqwest::Client::new();
    let request_payload = serde_json::json!({
        "code": auth_code,
        "redirect_uri": redirect_uri,
        "login_from": "web_ui",
    });

    let response = client
        .post("https://0hepe5jz44.execute-api.us-west-2.amazonaws.com/default/login_with_code")
        .json(&request_payload)
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to contact auth server: {}", e)))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Auth server error: {}", error_text)));
    }

    let login_response: crate::auth::LoginResponse = response
        .json()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to parse response: {}", e)))?;

    // Save session to file
    let config_dir = directories::ProjectDirs::from("app", "freewill", "kerr")
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get config dir".to_string()))?
        .config_dir()
        .to_path_buf();

    std::fs::create_dir_all(&config_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create config dir: {}", e)))?;

    let session_file = config_dir.join("session.json");
    let json_data = serde_json::to_string_pretty(&login_response)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize session: {}", e)))?;

    std::fs::write(&session_file, json_data)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write session: {}", e)))?;

    // Redirect back to home page
    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/")
        .body(Body::empty())
        .unwrap())
}

#[derive(Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    port: Option<u16>,
}

/// Check connection status
async fn connection_status(State(state): State<Arc<AppState>>) -> Json<ConnectionStatusResponse> {
    let node_addr = state.node_addr.lock().await;
    let conn_str = state.connection_string.lock().await;
    let conn_alias = state.connection_alias.lock().await;

    Json(ConnectionStatusResponse {
        connected: node_addr.is_some(),
        connection_string: conn_str.clone(),
        connection_alias: conn_alias.clone(),
    })
}

#[derive(Serialize)]
struct ConnectionStatusResponse {
    connected: bool,
    connection_string: Option<String>,
    connection_alias: Option<String>,
}

/// List registered connections
async fn list_connections() -> Result<Json<crate::auth::ConnectionsListResponse>, (StatusCode, String)> {
    match crate::auth::fetch_connections().await {
        Ok(response) => Ok(Json(response)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to fetch connections: {}", e),
        )),
    }
}

#[derive(Deserialize)]
struct ConnectRequest {
    connection_string: String,
    alias: Option<String>,
}

#[derive(Serialize)]
struct ConnectResponse {
    success: bool,
    message: String,
}

/// Connect to a selected connection
async fn connect_to_connection(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ConnectRequest>,
) -> Result<Json<ConnectResponse>, (StatusCode, String)> {
    // Check if already connected
    {
        let node_addr = state.node_addr.lock().await;
        if node_addr.is_some() {
            return Ok(Json(ConnectResponse {
                success: false,
                message: "Already connected".to_string(),
            }));
        }
    }

    // Decode connection string
    let addr = match crate::decode_connection_string(&request.connection_string) {
        Ok(addr) => addr,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Invalid connection string: {}", e),
            ))
        }
    };

    // Try to connect
    match connect_to_remote(&state.endpoint, &addr).await {
        Ok((conn, remote_fs)) => {
            // Update state
            {
                let mut state_addr = state.node_addr.lock().await;
                *state_addr = Some(addr);
            }
            {
                let mut state_conn = state.connection.lock().await;
                *state_conn = Some(Arc::new(conn));
            }
            {
                let mut state_fs = state.remote_fs.lock().await;
                *state_fs = Some(Arc::new(remote_fs));
            }
            {
                let mut conn_str = state.connection_string.lock().await;
                *conn_str = Some(request.connection_string.clone());
            }
            {
                let mut conn_alias = state.connection_alias.lock().await;
                *conn_alias = request.alias.clone();
            }

            Ok(Json(ConnectResponse {
                success: true,
                message: "Connected successfully".to_string(),
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to connect: {}", e),
        )),
    }
}

/// Disconnect from current connection
async fn disconnect_connection(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ConnectResponse>, (StatusCode, String)> {
    // Clear all connection state
    {
        let mut state_fs = state.remote_fs.lock().await;
        *state_fs = None;
    }
    {
        let mut state_conn = state.connection.lock().await;
        *state_conn = None;
    }
    {
        let mut state_addr = state.node_addr.lock().await;
        *state_addr = None;
    }
    {
        let mut conn_str = state.connection_string.lock().await;
        *conn_str = None;
    }
    {
        let mut conn_alias = state.connection_alias.lock().await;
        *conn_alias = None;
    }

    Ok(Json(ConnectResponse {
        success: true,
        message: "Disconnected successfully".to_string(),
    }))
}

/// WebSocket handler for shell sessions
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    eprintln!("[WEBSOCKET] WebSocket upgrade request received for /ws/shell");
    tracing::info!("WebSocket upgrade request received for /ws/shell");
    ws.on_upgrade(move |socket| async move {
        eprintln!("[WEBSOCKET] WebSocket upgraded, about to call handle_shell_socket");
        tracing::info!("WebSocket upgraded successfully, calling handle_shell_socket");
        handle_shell_socket(socket, state).await;
        eprintln!("[WEBSOCKET] handle_shell_socket returned");
    })
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type")]
enum TerminalMessage {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
}

/// Handle shell WebSocket connection
async fn handle_shell_socket(socket: WebSocket, state: Arc<AppState>) {
    eprintln!("[HANDLE_SHELL_SOCKET] Function entered!");

    // Create a session ID for logging
    let session_id = format!("ws_{}", std::process::id());
    let session_id_short = &session_id[..std::cmp::min(8, session_id.len())];

    eprintln!("[HANDLE_SHELL_SOCKET] Session ID created: {}", session_id_short);

    // Log session start with clear separator
    debug_log::log_new_session_separator(session_id_short, "WebSocket Shell");

    debug_log::log_ws_connection_start(session_id_short);
    tracing::info!(session_id = session_id_short, "WebSocket shell connection started");

    // Reuse the existing QUIC connection instead of creating a new one
    // This allows multiple sessions (FileBrowser + Shell) over a single connection
    let conn = {
        let conn_lock = state.connection.lock().await;
        match conn_lock.as_ref() {
            Some(c) => {
                tracing::info!(session_id = session_id_short, "Reusing existing QUIC connection for shell session");
                debug_log::log_debug(session_id_short, "Reusing existing QUIC connection (Arc::clone)");
                Arc::clone(c)  // Clone the Arc, NOT the Connection!
            },
            None => {
                eprintln!("[WEBSOCKET] No connection available");
                tracing::error!(session_id = session_id_short, "No QUIC connection available");
                debug_log::log_debug(session_id_short, "ERROR: No connection available");
                return;
            }
        }
    };

    // Note: With single-stream architecture, we should reuse the existing stream
    // For now, we'll open a new stream - full multiplexing needs more refactoring
    tracing::debug!(session_id = session_id_short, "Opening stream for shell session (TEMPORARY - needs full mux refactor)");
    let (mut send, recv) = match conn.open_bi().await {
        Ok(streams) => {
            tracing::info!(session_id = session_id_short, "Bidirectional stream opened successfully for shell");
            debug_log::log_bi_stream_accepted(session_id_short);
            streams
        },
        Err(e) => {
            eprintln!("[WEBSOCKET] Failed to open shell stream: {}", e);
            tracing::error!(session_id = session_id_short, error = %e, "Failed to open bidirectional stream");
            debug_log::log_debug(session_id_short, &format!("ERROR: Failed to open bi stream: {}", e));
            return;
        }
    };

    // Send Hello envelope with Shell session type
    debug_log::log_debug(session_id_short, "Sending Hello envelope for Shell session");
    let hello_envelope = crate::MessageEnvelope {
        session_id: format!("shell_{}", std::process::id()),
        payload: crate::MessagePayload::Client(crate::ClientMessage::Hello {
            session_type: crate::SessionType::Shell,
        }),
    };

    if let Err(e) = crate::send_envelope(&mut send, &hello_envelope).await {
        eprintln!("Failed to send hello envelope: {}", e);
        debug_log::log_debug(session_id_short, &format!("ERROR: Failed to send Hello envelope: {}", e));
        return;
    }
    debug_log::log_debug(session_id_short, "Hello envelope sent");

    let send = Arc::new(Mutex::new(send));
    let recv = Arc::new(Mutex::new(recv));

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Clone session_id for the spawned tasks
    let session_id_shell_to_ws = session_id_short.to_string();
    let session_id_ws_to_shell = session_id_short.to_string();

    // Spawn task to read from remote shell and send to WebSocket
    let recv_clone = recv.clone();
    let shell_to_ws = tokio::spawn(async move {
        debug_log::log_quic_to_ws_task_started(&session_id_shell_to_ws);
        let mut recv_guard = recv_clone.lock().await;
        let mut msg_count = 0;
        loop {
            // Receive envelope
            debug_log::log_quic_read_start(&session_id_shell_to_ws);
            let envelope = match crate::recv_envelope(&mut *recv_guard).await {
                Ok(env) => {
                    debug_log::log_quic_read_done(&session_id_shell_to_ws, 0);
                    env
                },
                Err(e) => {
                    eprintln!("[WS->SHELL] Failed to receive envelope: {}", e);
                    debug_log::log_quic_read_failed(&session_id_shell_to_ws, &e.to_string());
                    break;
                }
            };

            // Extract server message from envelope
            let msg = match envelope.payload {
                crate::MessagePayload::Server(server_msg) => server_msg,
                _ => {
                    eprintln!("[WS->SHELL] Received non-server message");
                    continue;
                }
            };

            debug_log::log_decode_done(&session_id_shell_to_ws, "ServerMessage");
            eprintln!("[WS->SHELL] Decoded envelope successfully");
            msg_count += 1;
            match msg {
                crate::ServerMessage::Output { data } => {
                    // Convert bytes to string for WebSocket
                    let text = String::from_utf8_lossy(&data).to_string();
                    eprintln!("[WS->SHELL] Sending output to WebSocket: {} bytes", text.len());
                    debug_log::log_ws_msg_sent(&session_id_shell_to_ws, text.len());
                    if let Err(e) = ws_sender.send(Message::Text(text.into())).await {
                        eprintln!("[WS->SHELL] Failed to send to WebSocket: {}", e);
                        debug_log::log_debug(&session_id_shell_to_ws, &format!("ERROR: WS send failed: {}", e));
                        break;
                    }
                }
                crate::ServerMessage::Error { message } => {
                    let error_msg = format!("\r\n\x1b[31mError: {}\x1b[0m\r\n", message);
                    eprintln!("[WS->SHELL] Sending error to WebSocket: {}", message);
                    debug_log::log_debug(&session_id_shell_to_ws, &format!("Sending error to WS: {}", message));
                    if let Err(e) = ws_sender.send(Message::Text(error_msg.into())).await {
                        eprintln!("[WS->SHELL] Failed to send error to WebSocket: {}", e);
                        debug_log::log_debug(&session_id_shell_to_ws, &format!("ERROR: WS error send failed: {}", e));
                        break;
                    }
                }
                _ => {}
            }
        }
        debug_log::log_quic_to_ws_task_ended(&session_id_shell_to_ws, &format!("processed {} messages", msg_count));
        eprintln!("[WS->SHELL] shell_to_ws task ended");
    });

    // Spawn task to read from WebSocket and send to remote shell
    let ws_to_shell = tokio::spawn(async move {
        debug_log::log_ws_to_quic_task_started(&session_id_ws_to_shell);
        let mut msg_count = 0;
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let Message::Text(text) = msg {
                eprintln!("[SHELL->WS] Received WebSocket message: {} bytes", text.len());
                debug_log::log_ws_msg_received(&session_id_ws_to_shell, text.len());
                // Parse terminal message
                if let Ok(term_msg) = serde_json::from_str::<TerminalMessage>(&text) {
                    match term_msg {
                        TerminalMessage::Input { data } => {
                            eprintln!("[SHELL->WS] Terminal input: {} bytes", data.len());
                            debug_log::log_debug(&session_id_ws_to_shell, &format!("Terminal input: {} bytes", data.len()));

                            let envelope = crate::MessageEnvelope {
                                session_id: format!("shell_{}", std::process::id()),
                                payload: crate::MessagePayload::Client(crate::ClientMessage::KeyEvent {
                                    data: data.into_bytes(),
                                }),
                            };

                            let mut send_guard = send.lock().await;
                            debug_log::log_quic_write_start(&session_id_ws_to_shell, 0);
                            if let Err(e) = crate::send_envelope(&mut *send_guard, &envelope).await {
                                eprintln!("[SHELL->WS] Failed to send envelope: {}", e);
                                debug_log::log_quic_write_failed(&session_id_ws_to_shell, 0, &e.to_string());
                                break;
                            }
                            debug_log::log_quic_write_done(&session_id_ws_to_shell, 0);
                            eprintln!("[SHELL->WS] Sent KeyEvent envelope");
                            msg_count += 1;
                        }
                        TerminalMessage::Resize { cols, rows } => {
                            eprintln!("[SHELL->WS] Terminal resize: {}x{}", cols, rows);
                            debug_log::log_debug(&session_id_ws_to_shell, &format!("Terminal resize: {}x{}", cols, rows));

                            let envelope = crate::MessageEnvelope {
                                session_id: format!("shell_{}", std::process::id()),
                                payload: crate::MessagePayload::Client(crate::ClientMessage::Resize { cols, rows }),
                            };

                            let mut send_guard = send.lock().await;
                            debug_log::log_quic_write_start(&session_id_ws_to_shell, 0);
                            if let Err(e) = crate::send_envelope(&mut *send_guard, &envelope).await {
                                eprintln!("[SHELL->WS] Failed to send envelope: {}", e);
                                debug_log::log_quic_write_failed(&session_id_ws_to_shell, 0, &e.to_string());
                                break;
                            }
                            debug_log::log_quic_write_done(&session_id_ws_to_shell, 0);
                            eprintln!("[SHELL->WS] Sent Resize envelope");
                            msg_count += 1;
                        }
                    }
                } else {
                    eprintln!("[SHELL->WS] Failed to parse terminal message");
                    debug_log::log_debug(&session_id_ws_to_shell, "ERROR: Failed to parse terminal message");
                }
            }
        }

        // Send disconnect envelope
        eprintln!("[SHELL->WS] Sending disconnect envelope");
        debug_log::log_debug(&session_id_ws_to_shell, "Sending disconnect message");
        let disconnect_envelope = crate::MessageEnvelope {
            session_id: format!("shell_{}", std::process::id()),
            payload: crate::MessagePayload::Client(crate::ClientMessage::Disconnect),
        };
        let mut send_guard = send.lock().await;
        let _ = crate::send_envelope(&mut *send_guard, &disconnect_envelope).await;
        debug_log::log_ws_to_quic_task_ended(&session_id_ws_to_shell, &format!("sent {} messages", msg_count));
        eprintln!("[SHELL->WS] ws_to_shell task ended");
    });

    // Wait for either task to complete
    tokio::select! {
        _ = shell_to_ws => {},
        _ = ws_to_shell => {},
    }
}

#[derive(Deserialize)]
struct FilePathQuery {
    path: String,
}

#[derive(Serialize)]
struct ListFilesResponse {
    entries: Vec<FileEntryResponse>,
}

#[derive(Serialize)]
struct FileEntryResponse {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: Option<String>,
}

/// List files in a directory
async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<ListFilesResponse>, (StatusCode, String)> {
    eprintln!("[API] list_files called for path: {}", query.path);

    // Get the remote filesystem
    let remote_fs = {
        eprintln!("[API] Acquiring remote_fs lock...");
        let fs_lock = state.remote_fs.lock().await;
        eprintln!("[API] Lock acquired, checking if fs exists...");
        match fs_lock.as_ref() {
            Some(fs) => {
                eprintln!("[API] Remote filesystem found, cloning...");
                Arc::clone(fs)
            },
            None => {
                eprintln!("[API] ERROR: No remote filesystem available");
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Not connected to remote host".to_string(),
                ))
            }
        }
    };

    let path = PathBuf::from(&query.path);
    eprintln!("[API] Calling remote_fs.read_dir for: {:?}", path);

    match remote_fs.read_dir(&path).await {
        Ok(entries) => {
            let response_entries: Vec<FileEntryResponse> = entries
                .into_iter()
                .map(|entry| FileEntryResponse {
                    name: entry.name,
                    path: entry.path.to_string_lossy().to_string(),
                    is_dir: entry.is_dir,
                    size: entry.metadata.as_ref().map(|m| m.size).unwrap_or(0),
                    modified: entry.metadata.as_ref()
                        .and_then(|m| m.modified)
                        .map(|m| {
                            let duration = m.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                            duration.as_secs().to_string()
                        }),
                })
                .collect();

            Ok(Json(ListFilesResponse {
                entries: response_entries,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list directory: {}", e),
        )),
    }
}

#[derive(Serialize)]
struct FileMetadataResponse {
    path: String,
    is_dir: bool,
    size: u64,
    modified: Option<String>,
    permissions: Option<u32>,
}

/// Get file metadata
async fn get_metadata(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<FileMetadataResponse>, (StatusCode, String)> {
    // Get the remote filesystem
    let remote_fs = {
        let fs_lock = state.remote_fs.lock().await;
        match fs_lock.as_ref() {
            Some(fs) => Arc::clone(fs),
            None => {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Not connected to remote host".to_string(),
                ))
            }
        }
    };

    let path = PathBuf::from(&query.path);

    match remote_fs.metadata(&path).await {
        Ok(metadata) => Ok(Json(FileMetadataResponse {
            path: query.path,
            is_dir: metadata.is_dir,
            size: metadata.size,
            modified: metadata.modified.map(|m| {
                let duration = m.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                duration.as_secs().to_string()
            }),
            permissions: None, // Permissions not available in FileMetadata
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get metadata: {}", e),
        )),
    }
}

#[derive(Serialize)]
struct FileContentResponse {
    content: String,
    size: u64,
}

/// Read file content
async fn read_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<FileContentResponse>, (StatusCode, String)> {
    // Get the remote filesystem
    let remote_fs = {
        let fs_lock = state.remote_fs.lock().await;
        match fs_lock.as_ref() {
            Some(fs) => Arc::clone(fs),
            None => {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Not connected to remote host".to_string(),
                ))
            }
        }
    };

    let path = PathBuf::from(&query.path);

    match remote_fs.read_file(&path).await {
        Ok(content) => {
            let size = content.len() as u64;
            // Try to convert to string, if it fails, return base64
            let content_str = String::from_utf8(content.clone())
                .unwrap_or_else(|_| base64::engine::general_purpose::STANDARD.encode(&content));

            Ok(Json(FileContentResponse {
                content: content_str,
                size,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read file: {}", e),
        )),
    }
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct WriteFileRequest {
    path: String,
    content: String,
}

#[derive(Serialize)]
struct WriteFileResponse {
    success: bool,
    message: String,
}

/// Write file content (placeholder - not implemented yet in RemoteFilesystem)
async fn write_file(
    State(_state): State<Arc<AppState>>,
    Json(_request): Json<WriteFileRequest>,
) -> Result<Json<WriteFileResponse>, (StatusCode, String)> {
    // Note: This would require adding write support to the RemoteFilesystem
    // and the corresponding server-side handling. For now, return not implemented.
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "File writing not yet implemented".to_string(),
    ))
}

/// Download a file
async fn download_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilePathQuery>,
) -> Result<Response, (StatusCode, String)> {
    // Get the remote filesystem
    let remote_fs = {
        let fs_lock = state.remote_fs.lock().await;
        match fs_lock.as_ref() {
            Some(fs) => Arc::clone(fs),
            None => {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Not connected to remote host".to_string(),
                ))
            }
        }
    };

    let path = PathBuf::from(&query.path);

    // Read the file content
    match remote_fs.read_file(&path).await {
        Ok(content) => {
            // Extract filename from path
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("download");

            // Determine MIME type
            let mime_type = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string();

            // Build response with appropriate headers
            let response = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", filename),
                )
                .body(Body::from(content))
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to build response: {}", e),
                    )
                })?;

            Ok(response)
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read file: {}", e),
        )),
    }
}

/// Upload a file
async fn upload_file(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut target_path: Option<String> = None;

    // Parse multipart form data
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to read multipart field: {}", e),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            // Read file content
            let data = field.bytes().await.map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Failed to read file data: {}", e),
                )
            })?;
            file_data = Some(data.to_vec());
        } else if name == "path" {
            // Read target path
            let text = field.text().await.map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Failed to read path: {}", e),
                )
            })?;
            target_path = Some(text);
        }
    }

    let file_data = file_data.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Missing file data".to_string(),
        )
    })?;

    let target_path = target_path.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Missing target path".to_string(),
        )
    })?;

    // Get the endpoint to create a new connection
    let endpoint = state.endpoint.clone();
    let node_addr = {
        let addr_lock = state.node_addr.lock().await;
        match addr_lock.as_ref() {
            Some(a) => a.clone(),
            None => {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Not connected to remote host".to_string(),
                ))
            }
        }
    };

    // Create a new connection for file transfer
    let conn = endpoint.connect(node_addr, crate::ALPN).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to connect: {}", e),
        )
    })?;

    let (mut send, mut recv) = conn.open_bi().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to open stream: {}", e),
        )
    })?;

    // Send Hello message with FileTransfer session type
    let hello_msg = crate::ClientMessage::Hello {
        session_type: crate::SessionType::FileTransfer,
    };
    let hello_data = bincode::encode_to_vec(&hello_msg, bincode::config::standard())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to encode hello: {}", e),
            )
        })?;

    // Send length prefix then message
    let len = (hello_data.len() as u32).to_be_bytes();
    send.write_all(&len).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send hello length: {}", e),
        )
    })?;
    send.write_all(&hello_data).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send hello: {}", e),
        )
    })?;

    // Send StartUpload message
    let start_msg = crate::ClientMessage::StartUpload {
        path: target_path.clone(),
        size: file_data.len() as u64,
        is_dir: false,
        force: true, // Overwrite if exists
    };
    let start_data = bincode::encode_to_vec(&start_msg, bincode::config::standard())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to encode start upload: {}", e),
            )
        })?;

    let len = (start_data.len() as u32).to_be_bytes();
    send.write_all(&len).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send start upload length: {}", e),
        )
    })?;
    send.write_all(&start_data).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send start upload: {}", e),
        )
    })?;

    // Wait for server response (it may ask for confirmation if file exists)
    let mut len_bytes = [0u8; 4];
    recv.read_exact(&mut len_bytes).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read response length: {}", e),
        )
    })?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    let mut msg_bytes = vec![0u8; len];
    recv.read_exact(&mut msg_bytes).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read response: {}", e),
        )
    })?;

    // Send file data in chunks
    const CHUNK_SIZE: usize = 65536; // 64KB chunks
    for chunk in file_data.chunks(CHUNK_SIZE) {
        let chunk_msg = crate::ClientMessage::FileChunk {
            data: chunk.to_vec(),
        };
        let chunk_data = bincode::encode_to_vec(&chunk_msg, bincode::config::standard())
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to encode chunk: {}", e),
                )
            })?;

        let len = (chunk_data.len() as u32).to_be_bytes();
        send.write_all(&len).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to send chunk length: {}", e),
            )
        })?;
        send.write_all(&chunk_data).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to send chunk: {}", e),
            )
        })?;
    }

    // Send EndUpload message
    let end_msg = crate::ClientMessage::EndUpload;
    let end_data = bincode::encode_to_vec(&end_msg, bincode::config::standard())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to encode end upload: {}", e),
            )
        })?;

    let len = (end_data.len() as u32).to_be_bytes();
    send.write_all(&len).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send end upload length: {}", e),
        )
    })?;
    send.write_all(&end_data).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to send end upload: {}", e),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "path": target_path,
    })))
}

/// Delete a file or directory
async fn delete_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilePathQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Get the remote filesystem
    let remote_fs = {
        let fs_lock = state.remote_fs.lock().await;
        match fs_lock.as_ref() {
            Some(fs) => Arc::clone(fs),
            None => {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Not connected to remote host".to_string(),
                ))
            }
        }
    };

    let path = PathBuf::from(&query.path);

    match remote_fs.delete_file(&path).await {
        Ok(()) => Ok(Json(serde_json::json!({
            "success": true,
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete file: {}", e),
        )),
    }
}

/// Request to create a port forwarding
#[derive(Deserialize)]
struct CreatePortForwardRequest {
    name: Option<String>,
    local_port: u16,
    remote_port: u16,
}

/// Response for port forwarding creation
#[derive(Serialize)]
struct CreatePortForwardResponse {
    id: String,
}

/// Request to disconnect a port forwarding
#[derive(Deserialize)]
struct DisconnectPortForwardRequest {
    id: String,
}

/// Create a new port forwarding
async fn create_port_forward(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreatePortForwardRequest>,
) -> Result<Json<CreatePortForwardResponse>, (StatusCode, String)> {
    // Generate unique ID
    use std::time::SystemTime;
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let id = format!("pf_{}_{}", timestamp, rand::random::<u32>());

    // Get connection
    let connection = {
        let conn_lock = state.connection.lock().await;
        match conn_lock.as_ref() {
            Some(conn) => Arc::clone(conn),
            None => {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Not connected to remote host".to_string(),
                ))
            }
        }
    };

    // Bind TCP listener on local port
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", payload.local_port))
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to bind to local port {}: {}", payload.local_port, e),
        ))?;

    // Create stop channel
    let (stop_tx, mut stop_rx) = mpsc::unbounded_channel::<()>();

    // Spawn task to accept connections and forward them
    let session_id = format!("tcp_relay_{}", id);
    let remote_port = payload.remote_port;
    let id_for_task = id.clone();
    let local_port_for_task = payload.local_port;
    tokio::spawn(async move {
        // Open single QUIC stream for TcpRelay session (multiplexed)
        let (mut send, recv) = match connection.open_bi().await {
            Ok(streams) => streams,
            Err(e) => {
                eprintln!("[PORT_FORWARD] Failed to open QUIC stream: {}", e);
                return;
            }
        };

        // Send Hello envelope for TcpRelay session
        let hello_envelope = crate::MessageEnvelope {
            session_id: session_id.clone(),
            payload: crate::MessagePayload::Client(crate::ClientMessage::Hello {
                session_type: crate::SessionType::TcpRelay,
            }),
        };
        if let Err(e) = crate::send_envelope(&mut send, &hello_envelope).await {
            eprintln!("[PORT_FORWARD] Failed to send Hello envelope: {}", e);
            return;
        }

        // Wrap streams in Arc<Mutex> for sharing
        let send = Arc::new(Mutex::new(send));
        let recv = Arc::new(Mutex::new(recv));

        // Track TCP connections with stream_ids
        let next_stream_id = Arc::new(Mutex::new(1u32));

        // Create demux channels map: stream_id -> channel for routing responses
        let demux_channels: Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Spawn demux task to read from QUIC and route to TCP connections
        let recv_for_demux = Arc::clone(&recv);
        let demux_channels_for_task = Arc::clone(&demux_channels);
        let session_id_for_demux = session_id.clone();
        tokio::spawn(async move {
            println!("[PORT_FORWARD] Demux task started for session {}", session_id_for_demux);
            loop {
                // Read envelope from QUIC stream
                let envelope = {
                    let mut recv_lock = recv_for_demux.lock().await;
                    match crate::recv_envelope(&mut *recv_lock).await {
                        Ok(env) => env,
                        Err(e) => {
                            eprintln!("[PORT_FORWARD] Demux task: failed to receive envelope: {}", e);
                            break;
                        }
                    }
                };

                // Route message based on type and stream_id
                match envelope.payload {
                    crate::MessagePayload::Server(crate::ServerMessage::TcpDataResponse { stream_id, data }) => {
                        println!("[PORT_FORWARD] Demux: received {} bytes for stream_id={}", data.len(), stream_id);
                        let channels_lock = demux_channels_for_task.lock().await;
                        if let Some(tx) = channels_lock.get(&stream_id) {
                            if tx.send(data).is_err() {
                                eprintln!("[PORT_FORWARD] Demux: failed to send to stream_id={} (channel closed)", stream_id);
                            }
                        } else {
                            eprintln!("[PORT_FORWARD] Demux: no handler for stream_id={}", stream_id);
                        }
                    }
                    crate::MessagePayload::Server(crate::ServerMessage::TcpCloseResponse { stream_id, error }) => {
                        println!("[PORT_FORWARD] Demux: received TcpClose for stream_id={}, error={:?}", stream_id, error);
                        // Close the channel by dropping the sender
                        let mut channels_lock = demux_channels_for_task.lock().await;
                        channels_lock.remove(&stream_id);
                    }
                    _ => {
                        eprintln!("[PORT_FORWARD] Demux: unexpected message type");
                    }
                }
            }
            println!("[PORT_FORWARD] Demux task ended for session {}", session_id_for_demux);
        });

        loop {
            tokio::select! {
                // Accept new TCP connection
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((tcp_stream, _)) => {
                            let stream_id = {
                                let mut id_lock = next_stream_id.lock().await;
                                let id = *id_lock;
                                *id_lock += 1;
                                id
                            };

                            println!("[PORT_FORWARD] Accepted connection on local port {}, forwarding to remote port {} (stream_id={})",
                                local_port_for_task, remote_port, stream_id);

                            // Send TcpOpen message
                            let open_msg = crate::MessageEnvelope {
                                session_id: session_id.clone(),
                                payload: crate::MessagePayload::Client(crate::ClientMessage::TcpOpen {
                                    stream_id,
                                    destination_host: None,  // Connect to localhost on remote server
                                    destination_port: remote_port,
                                }),
                            };
                            {
                                let mut send_lock = send.lock().await;
                                if let Err(e) = crate::send_envelope(&mut *send_lock, &open_msg).await {
                                    eprintln!("[PORT_FORWARD] Failed to send TcpOpen: {}", e);
                                    continue;
                                }
                            }

                            // Wait for TcpOpenResponse
                            let open_response = {
                                let mut recv_lock = recv.lock().await;
                                match crate::recv_envelope(&mut *recv_lock).await {
                                    Ok(envelope) => envelope,
                                    Err(e) => {
                                        eprintln!("[PORT_FORWARD] Failed to receive TcpOpenResponse: {}", e);
                                        continue;
                                    }
                                }
                            };

                            // Check if connection was successful
                            match open_response.payload {
                                crate::MessagePayload::Server(crate::ServerMessage::TcpOpenResponse { success, error, .. }) => {
                                    if !success {
                                        eprintln!("[PORT_FORWARD] Remote connection failed: {:?}", error);
                                        continue;
                                    }
                                }
                                _ => {
                                    eprintln!("[PORT_FORWARD] Unexpected response to TcpOpen");
                                    continue;
                                }
                            }

                            println!("[PORT_FORWARD] Remote connection established for stream_id={}", stream_id);

                            // Create channel for this TCP connection to receive data from the demux task
                            let (data_tx, mut data_rx) = mpsc::unbounded_channel::<Vec<u8>>();

                            // Register this stream_id with the demux channels
                            {
                                let mut demux_lock = demux_channels.lock().await;
                                demux_lock.insert(stream_id, data_tx);
                                println!("[PORT_FORWARD] Registered stream_id={} with demux (total streams: {})",
                                    stream_id, demux_lock.len());
                            }

                            // Spawn task to handle this TCP connection bidirectionally
                            let send_clone = Arc::clone(&send);
                            let session_id_clone = session_id.clone();
                            let demux_channels_clone = Arc::clone(&demux_channels);
                            tokio::spawn(async move {
                                use tokio::io::{AsyncReadExt, AsyncWriteExt};

                                let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();

                                // Task to read from local TCP and send to remote via QUIC
                                let send_task = {
                                    let send_for_task = Arc::clone(&send_clone);
                                    let session_id_for_task = session_id_clone.clone();
                                    tokio::spawn(async move {
                                        let mut buf = vec![0u8; 8192];
                                        loop {
                                            match tcp_read.read(&mut buf).await {
                                                Ok(0) => {
                                                    println!("[PORT_FORWARD] TCP EOF for stream_id={}", stream_id);
                                                    break;
                                                },  // EOF
                                                Ok(n) => {
                                                    println!("[PORT_FORWARD] Read {} bytes from local TCP stream_id={}", n, stream_id);
                                                    let data_msg = crate::MessageEnvelope {
                                                        session_id: session_id_for_task.clone(),
                                                        payload: crate::MessagePayload::Client(crate::ClientMessage::TcpData {
                                                            stream_id,
                                                            data: buf[..n].to_vec(),
                                                        }),
                                                    };
                                                    let mut send_lock = send_for_task.lock().await;
                                                    if crate::send_envelope(&mut *send_lock, &data_msg).await.is_err() {
                                                        eprintln!("[PORT_FORWARD] Failed to send data for stream_id={}", stream_id);
                                                        break;
                                                    }
                                                }
                                                Err(e) => {
                                                    eprintln!("[PORT_FORWARD] TCP read error for stream_id={}: {}", stream_id, e);
                                                    break;
                                                }
                                            }
                                        }

                                        // Send TcpClose
                                        println!("[PORT_FORWARD] Sending TcpClose for stream_id={}", stream_id);
                                        let close_msg = crate::MessageEnvelope {
                                            session_id: session_id_for_task.clone(),
                                            payload: crate::MessagePayload::Client(crate::ClientMessage::TcpClose {
                                                stream_id,
                                            }),
                                        };
                                        let mut send_lock = send_for_task.lock().await;
                                        let _ = crate::send_envelope(&mut *send_lock, &close_msg).await;
                                    })
                                };

                                // Task to receive from our dedicated channel and write to local TCP
                                let recv_task = tokio::spawn(async move {
                                    while let Some(data) = data_rx.recv().await {
                                        println!("[PORT_FORWARD] Writing {} bytes to local TCP stream_id={}", data.len(), stream_id);
                                        if tcp_write.write_all(&data).await.is_err() {
                                            eprintln!("[PORT_FORWARD] Failed to write to local TCP stream_id={}", stream_id);
                                            break;
                                        }
                                    }
                                    println!("[PORT_FORWARD] Recv task ended for stream_id={}", stream_id);
                                });

                                // Wait for either task to complete
                                tokio::select! {
                                    _ = send_task => {
                                        println!("[PORT_FORWARD] Send task completed for stream_id={}", stream_id);
                                    }
                                    _ = recv_task => {
                                        println!("[PORT_FORWARD] Recv task completed for stream_id={}", stream_id);
                                    }
                                }

                                // Cleanup: remove from demux channels
                                {
                                    let mut demux_lock = demux_channels_clone.lock().await;
                                    demux_lock.remove(&stream_id);
                                    println!("[PORT_FORWARD] Unregistered stream_id={} from demux (remaining: {})",
                                        stream_id, demux_lock.len());
                                }

                                println!("[PORT_FORWARD] Connection closed for stream_id={}", stream_id);
                            });
                        }
                        Err(e) => {
                            eprintln!("[PORT_FORWARD] Failed to accept TCP connection: {}", e);
                        }
                    }
                }
                // Stop signal received
                _ = stop_rx.recv() => {
                    println!("[PORT_FORWARD] Stopping port forwarding {}", id_for_task);

                    // Send Disconnect message
                    let disconnect_msg = crate::MessageEnvelope {
                        session_id: session_id.clone(),
                        payload: crate::MessagePayload::Client(crate::ClientMessage::Disconnect),
                    };
                    let mut send_lock = send.lock().await;
                    let _ = crate::send_envelope(&mut *send_lock, &disconnect_msg).await;
                    break;
                }
            }
        }
    });

    // Store session info
    {
        let mut forwardings = state.port_forwardings.lock().await;
        forwardings.insert(id.clone(), PortForwardingSession {
            id: id.clone(),
            name: payload.name,
            local_port: payload.local_port,
            remote_port: payload.remote_port,
            stop_tx,
        });
    }

    println!("[PORT_FORWARD] Created port forwarding {}: {} -> {}", id, payload.local_port, payload.remote_port);

    Ok(Json(CreatePortForwardResponse { id }))
}

/// Disconnect a port forwarding
async fn disconnect_port_forward(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DisconnectPortForwardRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut forwardings = state.port_forwardings.lock().await;

    if let Some(session) = forwardings.remove(&payload.id) {
        // Send stop signal
        let _ = session.stop_tx.send(());
        println!("[PORT_FORWARD] Disconnected port forwarding {}", payload.id);
        Ok(Json(serde_json::json!({ "success": true })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            format!("Port forwarding {} not found", payload.id),
        ))
    }
}
