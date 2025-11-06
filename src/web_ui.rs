use anyhow::Result;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use base64::Engine;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use crate::custom_explorer::filesystem::{FileEntry, FileMetadata, RemoteFilesystem};

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Asset;

/// Shared state for the web UI
struct AppState {
    remote_fs: Arc<RemoteFilesystem>,
}

/// Run the web UI server
pub async fn run_web_ui(connection_string: String) -> Result<()> {
    println!("Connecting to remote host...");

    // Decode connection string and connect
    let addr = crate::decode_connection_string(&connection_string)?;
    let endpoint = iroh::endpoint::Endpoint::builder()
        .discovery_n0()
        .bind()
        .await?;
    let conn = endpoint.connect(addr, crate::ALPN).await?;

    // Open bidirectional stream for file browser session
    let (send, recv) = conn.open_bi().await?;

    // Send Hello message with FileBrowser session type
    let hello_msg = crate::ClientMessage::Hello {
        session_type: crate::SessionType::FileBrowser,
    };
    let hello_data = bincode::encode_to_vec(&hello_msg, bincode::config::standard())?;
    send.write_all(&hello_data).await?;

    println!("Connected! Setting up file browser session...");

    // Create remote filesystem
    let remote_fs = Arc::new(RemoteFilesystem::new(
        PathBuf::from("/"),
        send,
        recv,
    ));

    // Create application state
    let state = Arc::new(AppState { remote_fs });

    // Build our application router
    let app = Router::new()
        .route("/api/files", get(list_files))
        .route("/api/file/content", get(read_file))
        .route("/api/file/content", post(write_file))
        .route("/api/file/metadata", get(get_metadata))
        .fallback(static_handler)
        .with_state(state);

    // Start the server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Web UI server running at http://{}", addr);
    println!("Open your browser to access the file browser and editor");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
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
    let path = PathBuf::from(&query.path);

    match state.remote_fs.read_dir(&path).await {
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
    let path = PathBuf::from(&query.path);

    match state.remote_fs.metadata(&path).await {
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
    let path = PathBuf::from(&query.path);

    match state.remote_fs.read_file(&path).await {
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
