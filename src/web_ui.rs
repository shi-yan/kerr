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

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Asset;

/// Shared state for the web UI
struct AppState {
    remote_fs: Arc<Mutex<Option<Arc<RemoteFilesystem>>>>,
    endpoint: Arc<iroh::endpoint::Endpoint>,
    node_addr: Arc<Mutex<Option<iroh::NodeAddr>>>,
    connection_string: Arc<Mutex<Option<String>>>,
    connection_alias: Arc<Mutex<Option<String>>>,
}

/// Run the web UI server
pub async fn run_web_ui(connection_string: Option<String>) -> Result<()> {
    // Create endpoint for future connections
    let endpoint = iroh::endpoint::Endpoint::builder()
        .discovery_n0()
        .bind()
        .await?;

    // If connection string is provided, connect immediately
    let (node_addr, remote_fs, conn_str_stored, conn_alias) = if let Some(conn_str) = connection_string {
        println!("Connecting to remote host...");
        let addr = crate::decode_connection_string(&conn_str)
            .map_err(|e| anyhow::anyhow!("Failed to decode connection string: {}", e))?;
        let (_conn, fs) = connect_to_remote(&endpoint, &addr).await?;
        println!("Connected! Setting up file browser session...");
        (Some(addr), Some(Arc::new(fs)), Some(conn_str), None)
    } else {
        println!("Starting UI in connection selection mode...");
        (None, None, None, None)
    };

    // Create application state
    let state = Arc::new(AppState {
        remote_fs: Arc::new(Mutex::new(remote_fs)),
        endpoint: Arc::new(endpoint),
        node_addr: Arc::new(Mutex::new(node_addr)),
        connection_string: Arc::new(Mutex::new(conn_str_stored)),
        connection_alias: Arc::new(Mutex::new(conn_alias)),
    });

    // Build our application router
    let app = Router::new()
        .route("/api/connection/status", get(connection_status))
        .route("/api/connection/list", get(list_connections))
        .route("/api/connection/connect", post(connect_to_connection))
        .route("/ws/shell", get(websocket_handler))
        .route("/api/files", get(list_files))
        .route("/api/files/download", get(download_file))
        .route("/api/files/upload", post(upload_file))
        .route("/api/file/content", get(read_file))
        .route("/api/file/content", post(write_file))
        .route("/api/file/metadata", get(get_metadata))
        .route("/api/file/delete", delete(delete_file))
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
    addr: &iroh::NodeAddr,
) -> Result<(iroh::endpoint::Connection, RemoteFilesystem)> {
    eprintln!("[CONNECT] Connecting to remote host...");
    // Connect to the remote host
    let conn = endpoint.connect(addr.clone(), crate::ALPN).await?;
    eprintln!("[CONNECT] Connection established!");

    eprintln!("[CONNECT] Opening bidirectional stream...");
    // Open bidirectional stream for file browser session
    let (mut send, recv) = conn.open_bi().await?;
    eprintln!("[CONNECT] Bidirectional stream opened!");

    // Send Hello message with FileBrowser session type
    eprintln!("[CONNECT] Sending Hello message with FileBrowser session type...");
    let hello_msg = crate::ClientMessage::Hello {
        session_type: crate::SessionType::FileBrowser,
    };
    let hello_data = bincode::encode_to_vec(&hello_msg, bincode::config::standard())?;

    // Send length prefix (4 bytes) then the message
    let len = (hello_data.len() as u32).to_be_bytes();
    send.write_all(&len).await?;
    send.write_all(&hello_data).await?;
    eprintln!("[CONNECT] Hello message sent!");

    // Create remote filesystem
    eprintln!("[CONNECT] Creating RemoteFilesystem...");
    let remote_fs = RemoteFilesystem::new(PathBuf::from("/"), send, recv);
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
        Ok((_conn, remote_fs)) => {
            // Update state
            {
                let mut state_addr = state.node_addr.lock().await;
                *state_addr = Some(addr);
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
    // Get the node address
    let addr = {
        let addr_lock = state.node_addr.lock().await;
        match addr_lock.as_ref() {
            Some(a) => a.clone(),
            None => {
                eprintln!("No connection available");
                return;
            }
        }
    };

    // Create a NEW connection for the shell session
    // (Each session type needs its own connection due to server architecture)
    let conn = match state.endpoint.connect(addr, crate::ALPN).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to connect for shell session: {}", e);
            return;
        }
    };

    // Open bidirectional stream for shell session
    let (mut send, recv) = match conn.open_bi().await {
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

    // Send length prefix (4 bytes) then the message
    let len = (hello_data.len() as u32).to_be_bytes();
    if let Err(e) = send.write_all(&len).await {
        eprintln!("Failed to send hello message length: {}", e);
        return;
    }
    if let Err(e) = send.write_all(&hello_data).await {
        eprintln!("Failed to send hello message: {}", e);
        return;
    }

    let send = Arc::new(Mutex::new(send));
    let recv = Arc::new(Mutex::new(recv));

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Spawn task to read from remote shell and send to WebSocket
    let recv_clone = recv.clone();
    let shell_to_ws = tokio::spawn(async move {
        let mut recv = recv_clone.lock().await;
        loop {
            // Read length prefix (4 bytes)
            let mut len_bytes = [0u8; 4];
            match recv.read_exact(&mut len_bytes).await {
                Ok(()) => {},
                Err(e) => {
                    eprintln!("[WS->SHELL] Failed to read message length: {}", e);
                    break;
                }
            }
            let len = u32::from_be_bytes(len_bytes) as usize;
            eprintln!("[WS->SHELL] Reading message of length: {}", len);

            // Read message data
            let mut msg_bytes = vec![0u8; len];
            match recv.read_exact(&mut msg_bytes).await {
                Ok(()) => {},
                Err(e) => {
                    eprintln!("[WS->SHELL] Failed to read message data: {}", e);
                    break;
                }
            }

            // Decode as ServerMessage
            if let Ok((msg, _)) = bincode::decode_from_slice::<crate::ServerMessage, _>(
                &msg_bytes,
                bincode::config::standard()
            ) {
                eprintln!("[WS->SHELL] Decoded message successfully");
                match msg {
                    crate::ServerMessage::Output { data } => {
                        // Convert bytes to string for WebSocket
                        let text = String::from_utf8_lossy(&data).to_string();
                        eprintln!("[WS->SHELL] Sending output to WebSocket: {} bytes", text.len());
                        if let Err(e) = ws_sender.send(Message::Text(text)).await {
                            eprintln!("[WS->SHELL] Failed to send to WebSocket: {}", e);
                            break;
                        }
                    }
                    crate::ServerMessage::Error { message } => {
                        let error_msg = format!("\r\n\x1b[31mError: {}\x1b[0m\r\n", message);
                        eprintln!("[WS->SHELL] Sending error to WebSocket: {}", message);
                        if let Err(e) = ws_sender.send(Message::Text(error_msg)).await {
                            eprintln!("[WS->SHELL] Failed to send error to WebSocket: {}", e);
                            break;
                        }
                    }
                    _ => {}
                }
            } else {
                eprintln!("[WS->SHELL] Failed to decode message");
            }
        }
        eprintln!("[WS->SHELL] shell_to_ws task ended");
    });

    // Spawn task to read from WebSocket and send to remote shell
    let ws_to_shell = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let Message::Text(text) = msg {
                eprintln!("[SHELL->WS] Received WebSocket message: {} bytes", text.len());
                // Parse terminal message
                if let Ok(term_msg) = serde_json::from_str::<TerminalMessage>(&text) {
                    match term_msg {
                        TerminalMessage::Input { data } => {
                            eprintln!("[SHELL->WS] Terminal input: {} bytes", data.len());
                            let client_msg = crate::ClientMessage::KeyEvent {
                                data: data.into_bytes(),
                            };
                            if let Ok(msg_data) = bincode::encode_to_vec(&client_msg, bincode::config::standard()) {
                                // Send length prefix (4 bytes) then message
                                let len = (msg_data.len() as u32).to_be_bytes();
                                let mut send = send.lock().await;
                                if let Err(e) = send.write_all(&len).await {
                                    eprintln!("[SHELL->WS] Failed to send length prefix: {}", e);
                                    break;
                                }
                                if let Err(e) = send.write_all(&msg_data).await {
                                    eprintln!("[SHELL->WS] Failed to send message: {}", e);
                                    break;
                                }
                                eprintln!("[SHELL->WS] Sent KeyEvent message");
                            }
                        }
                        TerminalMessage::Resize { cols, rows } => {
                            eprintln!("[SHELL->WS] Terminal resize: {}x{}", cols, rows);
                            let client_msg = crate::ClientMessage::Resize { cols, rows };
                            if let Ok(msg_data) = bincode::encode_to_vec(&client_msg, bincode::config::standard()) {
                                // Send length prefix (4 bytes) then message
                                let len = (msg_data.len() as u32).to_be_bytes();
                                let mut send = send.lock().await;
                                if let Err(e) = send.write_all(&len).await {
                                    eprintln!("[SHELL->WS] Failed to send length prefix: {}", e);
                                    break;
                                }
                                if let Err(e) = send.write_all(&msg_data).await {
                                    eprintln!("[SHELL->WS] Failed to send message: {}", e);
                                    break;
                                }
                                eprintln!("[SHELL->WS] Sent Resize message");
                            }
                        }
                    }
                } else {
                    eprintln!("[SHELL->WS] Failed to parse terminal message");
                }
            }
        }

        // Send disconnect message
        eprintln!("[SHELL->WS] Sending disconnect message");
        let disconnect_msg = crate::ClientMessage::Disconnect;
        if let Ok(msg_data) = bincode::encode_to_vec(&disconnect_msg, bincode::config::standard()) {
            let len = (msg_data.len() as u32).to_be_bytes();
            let mut send = send.lock().await;
            let _ = send.write_all(&len).await;
            let _ = send.write_all(&msg_data).await;
        }
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
