use anyhow::Result;
use axum::{
    body::Body,
    extract::{ws::WebSocketUpgrade, Query, State, WebSocket},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post},
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

use crate::custom_explorer::filesystem::{FileEntry, FileMetadata, RemoteFilesystem};

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Asset;

/// Shared state for the web UI
struct AppState {
    conn: Arc<Mutex<Option<Arc<iroh::endpoint::Connection>>>>,
    remote_fs: Arc<Mutex<Option<Arc<RemoteFilesystem>>>>,
    endpoint: Arc<iroh::endpoint::Endpoint>,
}

/// Run the web UI server
pub async fn run_web_ui(connection_string: Option<String>) -> Result<()> {
    // Create endpoint for future connections
    let endpoint = iroh::endpoint::Endpoint::builder()
        .discovery_n0()
        .bind()
        .await?;

    // If connection string is provided, connect immediately
    let (conn, remote_fs) = if let Some(conn_str) = connection_string {
        println!("Connecting to remote host...");
        let (c, fs) = connect_to_remote(&endpoint, &conn_str).await?;
        println!("Connected! Setting up file browser session...");
        (Some(Arc::new(c)), Some(Arc::new(fs)))
    } else {
        println!("Starting UI in connection selection mode...");
        (None, None)
    };

    // Create application state
    let state = Arc::new(AppState {
        conn: Arc::new(Mutex::new(conn)),
        remote_fs: Arc::new(Mutex::new(remote_fs)),
        endpoint: Arc::new(endpoint),
    });

    // Build our application router
    let app = Router::new()
        .route("/api/connection/status", get(connection_status))
        .route("/api/connection/list", get(list_connections))
        .route("/api/connection/connect", post(connect_to_connection))
        .route("/ws/shell", get(websocket_handler))
        .route("/api/files", get(list_files))
        .route("/api/file/content", get(read_file))
        .route("/api/file/content", post(write_file))
        .route("/api/file/metadata", get(get_metadata))
        .fallback(static_handler)
        .with_state(state);

    // Start the server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Web UI server running at http://{}", addr);
    println!("Open your browser to access the UI");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Connect to a remote host
async fn connect_to_remote(
    endpoint: &iroh::endpoint::Endpoint,
    connection_string: &str,
) -> Result<(iroh::endpoint::Connection, RemoteFilesystem)> {
    // Decode connection string and connect
    let addr = crate::decode_connection_string(connection_string)?;
    let conn = endpoint.connect(addr, crate::ALPN).await?;

    // Open bidirectional stream for file browser session
    let (send, recv) = conn.open_bi().await?;

    // Send Hello message with FileBrowser session type
    let hello_msg = crate::ClientMessage::Hello {
        session_type: crate::SessionType::FileBrowser,
    };
    let hello_data = bincode::encode_to_vec(&hello_msg, bincode::config::standard())?;
    send.write_all(&hello_data).await?;

    // Create remote filesystem
    let remote_fs = RemoteFilesystem::new(PathBuf::from("/"), send, recv);

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

/// Check connection status
async fn connection_status(State(state): State<Arc<AppState>>) -> Json<ConnectionStatusResponse> {
    let conn = state.conn.lock().await;
    Json(ConnectionStatusResponse {
        connected: conn.is_some(),
    })
}

#[derive(Serialize)]
struct ConnectionStatusResponse {
    connected: bool,
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
        let conn = state.conn.lock().await;
        if conn.is_some() {
            return Ok(Json(ConnectResponse {
                success: false,
                message: "Already connected".to_string(),
            }));
        }
    }

    // Try to connect
    match connect_to_remote(&state.endpoint, &request.connection_string).await {
        Ok((conn, remote_fs)) => {
            // Update state
            {
                let mut state_conn = state.conn.lock().await;
                *state_conn = Some(Arc::new(conn));
            }
            {
                let mut state_fs = state.remote_fs.lock().await;
                *state_fs = Some(Arc::new(remote_fs));
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

/// WebSocket handler for shell sessions
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_shell_socket(socket, state))
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
    // Get the connection
    let conn = {
        let conn_lock = state.conn.lock().await;
        match conn_lock.as_ref() {
            Some(c) => Arc::clone(c),
            None => {
                eprintln!("No connection available");
                return;
            }
        }
    };

    // Open a new bidirectional stream for shell session
    let (send, mut recv) = match conn.open_bi().await {
        Ok(streams) => streams,
        Err(e) => {
            eprintln!("Failed to open shell stream: {}", e);
            return;
        }
    };

    // Send Hello message with Shell session type
    let hello_msg = crate::ClientMessage::Hello {
        session_type: crate::SessionType::Shell,
    };
    let hello_data = match bincode::encode_to_vec(&hello_msg, bincode::config::standard()) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to encode hello message: {}", e);
            return;
        }
    };

    if let Err(e) = send.write_all(&hello_data).await {
        eprintln!("Failed to send hello message: {}", e);
        return;
    }

    let send = Arc::new(Mutex::new(send));
    let recv = Arc::new(Mutex::new(recv));

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Spawn task to read from remote shell and send to WebSocket
    let send_clone = send.clone();
    let recv_clone = recv.clone();
    let shell_to_ws = tokio::spawn(async move {
        let mut recv = recv_clone.lock().await;
        loop {
            let mut buf = vec![0u8; 8192];
            match recv.read(&mut buf).await {
                Ok(Some(n)) => {
                    if n == 0 {
                        break;
                    }
                    buf.truncate(n);

                    // Try to decode as ServerMessage
                    if let Ok((msg, _)) = bincode::decode_from_slice::<crate::ServerMessage, _>(
                        &buf,
                        bincode::config::standard()
                    ) {
                        match msg {
                            crate::ServerMessage::Output { data } => {
                                // Convert bytes to string for WebSocket
                                let text = String::from_utf8_lossy(&data).to_string();
                                if let Err(_) = ws_sender.send(axum::extract::ws::Message::Text(text)).await {
                                    break;
                                }
                            }
                            crate::ServerMessage::Error { message } => {
                                let error_msg = format!("\r\n\x1b[31mError: {}\x1b[0m\r\n", message);
                                if let Err(_) = ws_sender.send(axum::extract::ws::Message::Text(error_msg)).await {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(None) | Err(_) => {
                    break;
                }
            }
        }
    });

    // Spawn task to read from WebSocket and send to remote shell
    let ws_to_shell = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let axum::extract::ws::Message::Text(text) = msg {
                // Parse terminal message
                if let Ok(term_msg) = serde_json::from_str::<TerminalMessage>(&text) {
                    match term_msg {
                        TerminalMessage::Input { data } => {
                            let client_msg = crate::ClientMessage::KeyEvent {
                                data: data.into_bytes(),
                            };
                            if let Ok(msg_data) = bincode::encode_to_vec(&client_msg, bincode::config::standard()) {
                                let mut send = send.lock().await;
                                if let Err(_) = send.write_all(&msg_data).await {
                                    break;
                                }
                            }
                        }
                        TerminalMessage::Resize { cols, rows } => {
                            let client_msg = crate::ClientMessage::Resize { cols, rows };
                            if let Ok(msg_data) = bincode::encode_to_vec(&client_msg, bincode::config::standard()) {
                                let mut send = send.lock().await;
                                if let Err(_) = send.write_all(&msg_data).await {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Send disconnect message
        let disconnect_msg = crate::ClientMessage::Disconnect;
        if let Ok(msg_data) = bincode::encode_to_vec(&disconnect_msg, bincode::config::standard()) {
            let mut send = send.lock().await;
            let _ = send.write_all(&msg_data).await;
        }
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

    match remote_fs.read_dir(&path).await {
        Ok(entries) => {
            let response_entries: Vec<FileEntryResponse> = entries
                .into_iter()
                .map(|entry| FileEntryResponse {
                    name: entry.name,
                    path: entry.path.to_string_lossy().to_string(),
                    is_dir: entry.is_dir,
                    size: entry.size,
                    modified: entry.modified.map(|m| m.to_string()),
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
            modified: metadata.modified.map(|m| m.to_string()),
            permissions: metadata.permissions,
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
    State(state): State<Arc<AppState>>,
    Json(request): Json<WriteFileRequest>,
) -> Result<Json<WriteFileResponse>, (StatusCode, String)> {
    // Note: This would require adding write support to the RemoteFilesystem
    // and the corresponding server-side handling. For now, return not implemented.
    Err((
        StatusCode::NOT_IMPLEMENTED,
        "File writing not yet implemented".to_string(),
    ))
}
