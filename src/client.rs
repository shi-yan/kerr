//! Kerr client - connects to server and provides interactive terminal

use iroh::Endpoint;
use n0_snafu::{Result, ResultExt};
use std::io::{self, Write};
use crossterm::{
    terminal::{self, ClearType},
    ExecutableCommand,
};
use crate::{ClientMessage, ServerMessage, ALPN};
use bincode::config;

/// Convert a crossterm KeyEvent to raw terminal bytes
fn key_event_to_bytes(event: crossterm::event::KeyEvent) -> Vec<u8> {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mut bytes = Vec::new();

    match event.code {
        KeyCode::Char(c) => {
            if event.modifiers.contains(KeyModifiers::CONTROL) {
                // Control characters
                if c.is_ascii_lowercase() || c.is_ascii_uppercase() {
                    // Ctrl+A = 1, Ctrl+B = 2, etc.
                    let ctrl_code = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                    bytes.push(ctrl_code);
                } else {
                    // For other chars with Ctrl, just send the char
                    bytes.extend_from_slice(c.to_string().as_bytes());
                }
            } else if event.modifiers.contains(KeyModifiers::ALT) {
                // Alt/Meta sends ESC followed by the character
                bytes.push(27); // ESC
                bytes.extend_from_slice(c.to_string().as_bytes());
            } else {
                // Regular character
                bytes.extend_from_slice(c.to_string().as_bytes());
            }
        }
        KeyCode::Enter => bytes.push(b'\r'),
        KeyCode::Backspace => bytes.push(127), // DEL
        KeyCode::Tab => bytes.push(b'\t'),
        KeyCode::Esc => bytes.push(27),
        KeyCode::Up => bytes.extend_from_slice(b"\x1b[A"),
        KeyCode::Down => bytes.extend_from_slice(b"\x1b[B"),
        KeyCode::Right => bytes.extend_from_slice(b"\x1b[C"),
        KeyCode::Left => bytes.extend_from_slice(b"\x1b[D"),
        KeyCode::Home => bytes.extend_from_slice(b"\x1b[H"),
        KeyCode::End => bytes.extend_from_slice(b"\x1b[F"),
        KeyCode::PageUp => bytes.extend_from_slice(b"\x1b[5~"),
        KeyCode::PageDown => bytes.extend_from_slice(b"\x1b[6~"),
        KeyCode::Delete => bytes.extend_from_slice(b"\x1b[3~"),
        KeyCode::Insert => bytes.extend_from_slice(b"\x1b[2~"),
        KeyCode::F(n) => {
            match n {
                1 => bytes.extend_from_slice(b"\x1bOP"),
                2 => bytes.extend_from_slice(b"\x1bOQ"),
                3 => bytes.extend_from_slice(b"\x1bOR"),
                4 => bytes.extend_from_slice(b"\x1bOS"),
                5 => bytes.extend_from_slice(b"\x1b[15~"),
                6 => bytes.extend_from_slice(b"\x1b[17~"),
                7 => bytes.extend_from_slice(b"\x1b[18~"),
                8 => bytes.extend_from_slice(b"\x1b[19~"),
                9 => bytes.extend_from_slice(b"\x1b[20~"),
                10 => bytes.extend_from_slice(b"\x1b[21~"),
                11 => bytes.extend_from_slice(b"\x1b[23~"),
                12 => bytes.extend_from_slice(b"\x1b[24~"),
                _ => {}
            }
        }
        _ => {
            // Unsupported key, ignore
        }
    }

    bytes
}

pub async fn run_client(connection_string: String) -> Result<()> {
    // Decode the compressed connection string (base64 -> gzip -> JSON -> NodeAddr)
    let addr = crate::decode_connection_string(&connection_string)
        .expect("Failed to decode connection string");

    println!("Connecting to: {}", addr.node_id);

    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    // Open a connection to the accepting node
    println!("Connecting to Kerr server...");
    let conn = endpoint.connect(addr, ALPN).await?;
    println!("Connected! Starting terminal session...");
    println!("Press Ctrl+D to disconnect.");

    // Open a bidirectional QUIC stream
    let (mut send, mut recv) = conn.open_bi().await.e()?;

    // Send Hello message to indicate this is a shell session
    let config = config::standard();
    let hello_msg = ClientMessage::Hello { session_type: crate::SessionType::Shell };
    if let Ok(encoded) = bincode::encode_to_vec(&hello_msg, config) {
        let len = (encoded.len() as u32).to_be_bytes();
        send.write_all(&len).await.e()?;
        send.write_all(&encoded).await.e()?;
    }

    // Enter raw mode
    terminal::enable_raw_mode().expect("Failed to enable raw mode");
    let mut stdout = io::stdout();
    stdout.execute(terminal::Clear(ClearType::All)).unwrap();

    // Send initial terminal size
    let config = config::standard();
    if let Ok((cols, rows)) = terminal::size() {
        let resize_msg = ClientMessage::Resize { cols, rows };
        if let Ok(encoded) = bincode::encode_to_vec(&resize_msg, config) {
            let len = (encoded.len() as u32).to_be_bytes();
            send.write_all(&len).await.e().ok();
            send.write_all(&encoded).await.e().ok();
        }
    }

    // Channel to send messages to the server
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMessage>();

    // Spawn task to write messages to send stream
    let send_task = tokio::spawn(async move {
        let config = config::standard();
        while let Some(msg) = msg_rx.recv().await {
            if let Ok(encoded) = bincode::encode_to_vec(&msg, config) {
                let len = (encoded.len() as u32).to_be_bytes();
                if send.write_all(&len).await.is_err() {
                    break;
                }
                if send.write_all(&encoded).await.is_err() {
                    break;
                }
            }
        }
    });

    // Spawn task to handle stdin input in raw mode using crossterm events
    // This handles both keyboard input and terminal resize events
    let msg_tx_clone = msg_tx.clone();
    let input_task = tokio::spawn(async move {
        use futures::StreamExt;
        use crossterm::event::{EventStream, Event, KeyCode, KeyEvent, KeyModifiers};

        let mut event_stream = EventStream::new();
        while let Some(event_result) = event_stream.next().await {
            match event_result {
                Ok(Event::Key(KeyEvent { code: KeyCode::Char('d'), modifiers: KeyModifiers::CONTROL, .. })) => {
                    // Ctrl+D - disconnect
                    let _ = msg_tx_clone.send(ClientMessage::Disconnect);
                    break;
                }
                Ok(Event::Key(key_event)) => {
                    // Convert key event to raw bytes
                    let data = key_event_to_bytes(key_event);
                    if msg_tx_clone.send(ClientMessage::KeyEvent { data }).is_err() {
                        break;
                    }
                }
                Ok(Event::Resize(cols, rows)) => {
                    // Handle terminal resize
                    let _ = msg_tx_clone.send(ClientMessage::Resize { cols, rows });
                }
                Ok(_) => {
                    // Ignore other events (mouse, focus, etc.)
                }
                Err(_) => break,
            }
        }
    });

    // Main task: receive output from server and display
    let output_task = tokio::spawn(async move {
        let mut stdout = io::stdout();
        let config = config::standard();
        loop {
            // Read message length (4 bytes)
            let mut len_bytes = [0u8; 4];
            match recv.read_exact(&mut len_bytes).await {
                Ok(_) => {},
                Err(_) => break, // Connection closed
            }
            let len = u32::from_be_bytes(len_bytes) as usize;

            // Read message data
            let mut msg_bytes = vec![0u8; len];
            match recv.read_exact(&mut msg_bytes).await {
                Ok(_) => {},
                Err(_) => break,
            }

            // Deserialize message
            let msg: ServerMessage = match bincode::decode_from_slice(&msg_bytes, config) {
                Ok((m, _)) => m,
                Err(_) => continue,
            };

            match msg {
                ServerMessage::Output { data } => {
                    // Write output to terminal
                    let _ = stdout.write_all(&data);
                    let _ = stdout.flush();
                }
                ServerMessage::Error { message } => {
                    // Display error message
                    eprintln!("\r\n{}\r\n", message);

                    // If this is a session end message, break the loop to exit
                    if message.contains("Session ended") || message.contains("bash exited") {
                        break;
                    }
                }
                ServerMessage::UploadAck => {
                    // Acknowledgment for file upload - not used in run_client
                }
                ServerMessage::ConfirmPrompt { .. } => {
                    // Confirmation prompt - not used in run_client
                }
                ServerMessage::StartDownload { .. } => {
                    // Download start - not used in run_client
                }
                ServerMessage::FileChunk { .. } => {
                    // File chunk - not used in run_client
                }
                ServerMessage::EndDownload => {
                    // Download end - not used in run_client
                }
                ServerMessage::Progress { .. } => {
                    // Progress update - not used in run_client
                }
                ServerMessage::FsDirListing { .. } => {
                    // Directory listing - not used in run_client (only for browse)
                }
                ServerMessage::FsMetadataResponse { .. } => {
                    // Metadata response - not used in run_client (only for browse)
                }
                ServerMessage::FsFileContent { .. } => {
                    // File content - not used in run_client (only for browse)
                }
                ServerMessage::FsHashResponse { .. } => {
                    // File hash response - not used in run_client (only for browse)
                }
                ServerMessage::FsError { .. } => {
                    // Filesystem error - not used in run_client (only for browse)
                }
                ServerMessage::TcpOpenResponse { .. } => {
                    // TCP open response - not used in run_client (only for relay)
                }
                ServerMessage::TcpDataResponse { .. } => {
                    // TCP data response - not used in run_client (only for relay)
                }
                ServerMessage::TcpCloseResponse { .. } => {
                    // TCP close response - not used in run_client (only for relay)
                }
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = input_task => {},
        _ = output_task => {},
        _ = send_task => {},
    }

    // Restore terminal
    terminal::disable_raw_mode().expect("Failed to disable raw mode");
    println!("\r\nDisconnected from server.");

    // Explicitly close the connection
    conn.close(0u32.into(), b"bye!");
    endpoint.close().await;

    Ok(())
}

/// Send a file or directory to the server
pub async fn send_file(connection_string: String, local_path: String, remote_path: String, force: bool) -> Result<()> {
    use std::path::Path;
    use std::fs;
    use indicatif::{ProgressBar, ProgressStyle};
    use crate::transfer::{calculate_size, get_files_recursive, CHUNK_SIZE};

    // Decode the compressed connection string (base64 -> gzip -> JSON)
    let addr = crate::decode_connection_string(&connection_string)
        .expect("Failed to decode connection string");

    println!("Connecting to server...");
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;
    let conn = endpoint.connect(addr, ALPN).await?;
    let (mut send, mut recv) = conn.open_bi().await.e()?;

    // Send Hello message to indicate this is a file transfer session
    let config = config::standard();
    let hello_msg = ClientMessage::Hello { session_type: crate::SessionType::FileTransfer };
    let encoded = bincode::encode_to_vec(&hello_msg, config).unwrap();
    let len = (encoded.len() as u32).to_be_bytes();
    send.write_all(&len).await.e()?;
    send.write_all(&encoded).await.e()?;

    let local = Path::new(&local_path);
    let is_dir = local.is_dir();

    // Determine the actual remote file path
    // If remote_path ends with / or is a directory name, append the local filename
    let actual_remote_path = if is_dir {
        // If sending a directory, use the remote_path as-is
        remote_path.clone()
    } else {
        // If sending a single file, determine the destination filename
        let local_filename = local.file_name()
            .expect("Failed to get local filename")
            .to_str()
            .expect("Invalid filename");

        // If remote_path looks like a directory (ends with /), append the filename
        if remote_path.ends_with('/') {
            format!("{}{}", remote_path, local_filename)
        } else {
            // Otherwise use remote_path as the exact filename
            remote_path.clone()
        }
    };

    println!("Calculating size...");
    let total_size = calculate_size(local)
        .expect("Failed to calculate file size");

    // Send upload start message
    let start_msg = ClientMessage::StartUpload {
        path: actual_remote_path.clone(),
        size: total_size,
        is_dir,
        force,
    };
    let config = config::standard();
    let encoded = bincode::encode_to_vec(&start_msg, config).unwrap();
    let len = (encoded.len() as u32).to_be_bytes();
    send.write_all(&len).await.e()?;
    send.write_all(&encoded).await.e()?;

    // Wait for ack or error
    let mut len_bytes = [0u8; 4];
    recv.read_exact(&mut len_bytes).await.e()?;
    let msg_len = u32::from_be_bytes(len_bytes) as usize;
    let mut msg_bytes = vec![0u8; msg_len];
    recv.read_exact(&mut msg_bytes).await.e()?;

    // Check if we got UploadAck, ConfirmPrompt, or Error
    let (response, _): (ServerMessage, _) = bincode::decode_from_slice(&msg_bytes, config)
        .expect("Failed to decode server response");

    match response {
        ServerMessage::UploadAck => {
            // Good to proceed
        }
        ServerMessage::ConfirmPrompt { message } => {
            // Ask user for confirmation
            use std::io::{stdin, stdout, Write as _};
            print!("{} [y/N]: ", message);
            stdout().flush().unwrap();

            let mut input = String::new();
            stdin().read_line(&mut input).unwrap();
            let confirmed = input.trim().eq_ignore_ascii_case("y");

            // Send confirmation response
            let confirm_msg = ClientMessage::ConfirmResponse { confirmed };
            let encoded = bincode::encode_to_vec(&confirm_msg, config).unwrap();
            let len = (encoded.len() as u32).to_be_bytes();
            send.write_all(&len).await.e()?;
            send.write_all(&encoded).await.e()?;

            if !confirmed {
                println!("Upload cancelled.");
                return Ok(());
            }

            // Wait for final ack after confirmation
            let mut len_bytes = [0u8; 4];
            recv.read_exact(&mut len_bytes).await.e()?;
            let msg_len = u32::from_be_bytes(len_bytes) as usize;
            let mut msg_bytes = vec![0u8; msg_len];
            recv.read_exact(&mut msg_bytes).await.e()?;

            let (final_response, _): (ServerMessage, _) = bincode::decode_from_slice(&msg_bytes, config)
                .expect("Failed to decode server response");

            match final_response {
                ServerMessage::UploadAck => {
                    // Good to proceed
                }
                ServerMessage::Error { message } => {
                    eprintln!("Server error: {}", message);
                    return Ok(());
                }
                _ => {
                    eprintln!("Unexpected server response");
                    return Ok(());
                }
            }
        }
        ServerMessage::Error { message } => {
            eprintln!("Server error: {}", message);
            return Ok(());
        }
        _ => {
            eprintln!("Unexpected server response");
            return Ok(());
        }
    }

    // Create progress bar
    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    // Send file data
    let mut bytes_sent = 0u64;
    let files = get_files_recursive(local)
        .expect("Failed to get files");

    for file in files {
        let mut f = fs::File::open(&file)
            .expect("Failed to open file");
        let mut buffer = vec![0u8; CHUNK_SIZE];

        loop {
            use std::io::Read;
            let n = f.read(&mut buffer)
                .expect("Failed to read file");
            if n == 0 {
                break;
            }

            let chunk_msg = ClientMessage::FileChunk {
                data: buffer[..n].to_vec(),
            };
            let encoded = bincode::encode_to_vec(&chunk_msg, config).unwrap();
            let len = (encoded.len() as u32).to_be_bytes();
            send.write_all(&len).await.e()?;
            send.write_all(&encoded).await.e()?;

            bytes_sent += n as u64;
            pb.set_position(bytes_sent);
        }
    }

    // Send end message
    let end_msg = ClientMessage::EndUpload;
    let encoded = bincode::encode_to_vec(&end_msg, config).unwrap();
    let len = (encoded.len() as u32).to_be_bytes();
    send.write_all(&len).await.e()?;
    send.write_all(&encoded).await.e()?;

    pb.finish_with_message("Upload complete!");

    conn.close(0u32.into(), b"done");
    endpoint.close().await;

    Ok(())
}

/// Pull a file or directory from the server
pub async fn pull_file(_connection_string: String, remote_path: String, local_path: String) -> Result<()> {
    println!("Pull functionality not yet implemented");
    println!("Would pull {} to {}", remote_path, local_path);
    Ok(())
}

/// Browse remote filesystem
pub async fn browse_remote(connection_string: String) -> Result<()> {
    use std::sync::Arc;
    use std::path::PathBuf;

    // Decode connection string
    let addr = crate::decode_connection_string(&connection_string)
        .expect("Failed to decode connection string");

    println!("Connecting to server for file browsing...");
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;
    let conn = endpoint.connect(addr, ALPN).await?;

    let (mut send, recv) = conn.open_bi().await.e()?;

    // Send Hello message with FileBrowser session type
    let hello = ClientMessage::Hello {
        session_type: crate::SessionType::FileBrowser,
    };

    let config = bincode::config::standard();
    let encoded = bincode::encode_to_vec(&hello, config).unwrap();
    let len = (encoded.len() as u32).to_be_bytes();

    send.write_all(&len).await.e()?;
    send.write_all(&encoded).await.e()?;

    println!("Connected! Starting file browser...");

    // Create RemoteFilesystem
    use crate::custom_explorer::filesystem::RemoteFilesystem;
    let remote_fs = Arc::new(RemoteFilesystem::new(
        PathBuf::from("/"),
        send,
        recv,
    ));

    // Run the browser with remote filesystem
    // Pass remote_fs as both the filesystem trait object and as the concrete type for caching
    let filesystem: Arc<dyn crate::custom_explorer::Filesystem> = Arc::clone(&remote_fs) as Arc<dyn crate::custom_explorer::Filesystem>;
    crate::browser::run_browser_with_fs(filesystem, Some(remote_fs))
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Browser error: {}", e)))?;

    conn.close(0u32.into(), b"done");
    endpoint.close().await;

    Ok(())
}

/// Run a TCP relay proxy that forwards local port to remote port
pub async fn run_tcp_relay(
    connection_string: &str,
    local_port: u16,
    remote_port: u16,
) -> Result<()> {
    use tokio::net::TcpListener;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Decode connection string and connect to server
    let node_addr = crate::decode_connection_string(connection_string)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to decode connection string: {}", e)))?;

    let endpoint = iroh::Endpoint::builder()
        .discovery(iroh::discovery::dns::DnsDiscovery::n0_dns())
        .bind()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to create endpoint: {}", e)))?;

    let conn = endpoint.connect(node_addr, crate::ALPN)
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to connect: {}", e)))?;

    let (mut send, mut recv) = conn.open_bi()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to open stream: {}", e)))?;

    // Send Hello message with TcpRelay session type
    let hello = crate::ClientMessage::Hello {
        session_type: crate::SessionType::TcpRelay,
    };
    let config = bincode::config::standard();
    let encoded = bincode::encode_to_vec(&hello, config)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to encode hello: {}", e)))?;
    let len = (encoded.len() as u32).to_be_bytes();
    send.write_all(&len).await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to send length: {}", e)))?;
    send.write_all(&encoded).await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to send hello: {}", e)))?;

    // Traffic counters
    let upload_bytes = Arc::new(AtomicU64::new(0));
    let download_bytes = Arc::new(AtomicU64::new(0));

    // Listen on local port
    let listener = TcpListener::bind(format!("127.0.0.1:{}", local_port))
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to bind to port {}: {}", local_port, e)))?;

    // Start TUI in a blocking task
    let upload_bytes_ui = Arc::clone(&upload_bytes);
    let download_bytes_ui = Arc::clone(&download_bytes);
    let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    let ui_task = tokio::task::spawn_blocking(move || {
        crate::traffic_ui::run_traffic_ui(local_port, remote_port, upload_bytes_ui, download_bytes_ui, shutdown_rx)
    });

    // Shared state for tracking TCP connections
    let tcp_connections: Arc<Mutex<HashMap<u32, tokio::sync::mpsc::Sender<Vec<u8>>>>> = Arc::new(Mutex::new(HashMap::new()));
    let next_stream_id = Arc::new(Mutex::new(1u32));

    // Wrap send stream in Arc<Mutex> for sharing between tasks
    let send = Arc::new(Mutex::new(send));
    let send_clone = Arc::clone(&send);

    // Task to handle incoming messages from server
    let tcp_connections_clone = Arc::clone(&tcp_connections);
    let download_bytes_recv = Arc::clone(&download_bytes);
    let recv_task = tokio::spawn(async move {
        loop {
            // Read message length
            let mut len_bytes = [0u8; 4];
            if let Err(_) = recv.read_exact(&mut len_bytes).await {
                break;
            }
            let msg_len = u32::from_be_bytes(len_bytes) as usize;

            // Read message
            let mut msg_bytes = vec![0u8; msg_len];
            if let Err(_) = recv.read_exact(&mut msg_bytes).await {
                break;
            }

            // Decode message
            let config = bincode::config::standard();
            let (msg, _): (crate::ServerMessage, _) = match bincode::decode_from_slice(&msg_bytes, config) {
                Ok(m) => m,
                Err(_) => break,
            };

            // Handle server messages
            match msg {
                crate::ServerMessage::TcpDataResponse { stream_id, data } => {
                    // Track download bytes
                    download_bytes_recv.fetch_add(data.len() as u64, Ordering::Relaxed);

                    // Forward data to local TCP connection
                    let connections = tcp_connections_clone.lock().await;
                    if let Some(tx) = connections.get(&stream_id) {
                        let _ = tx.send(data).await;
                    }
                }
                crate::ServerMessage::TcpCloseResponse { stream_id, error } => {
                    if let Some(err) = error {
                        eprintln!("Remote TCP connection {} closed with error: {}", stream_id, err);
                    }
                    // Remove connection from map (this will cause the local connection to close)
                    tcp_connections_clone.lock().await.remove(&stream_id);
                }
                crate::ServerMessage::TcpOpenResponse { stream_id, success, error } => {
                    if !success {
                        eprintln!("Failed to open remote connection {}: {}", stream_id, error.unwrap_or_default());
                        tcp_connections_clone.lock().await.remove(&stream_id);
                    }
                }
                _ => {}
            }
        }
    });

    // Accept incoming TCP connections
    loop {
        let (tcp_stream, addr) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                continue;
            }
        };

        println!("New connection from {}", addr);

        // Get next stream ID
        let stream_id = {
            let mut id = next_stream_id.lock().await;
            let current = *id;
            *id += 1;
            current
        };

        // Send TcpOpen message
        let open_msg = crate::ClientMessage::TcpOpen {
            stream_id,
            destination_port: remote_port,
        };
        let config = bincode::config::standard();
        let encoded = match bincode::encode_to_vec(&open_msg, config) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to encode TcpOpen: {}", e);
                continue;
            }
        };
        let len = (encoded.len() as u32).to_be_bytes();

        {
            let mut send_locked = send_clone.lock().await;
            if let Err(e) = send_locked.write_all(&len).await {
                eprintln!("Failed to send length: {}", e);
                break;
            }
            if let Err(e) = send_locked.write_all(&encoded).await {
                eprintln!("Failed to send TcpOpen: {}", e);
                break;
            }
        }

        // Create channel for receiving data from server
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(100);
        tcp_connections.lock().await.insert(stream_id, tx);

        let send_for_task = Arc::clone(&send_clone);
        let tcp_connections_for_task = Arc::clone(&tcp_connections);
        let upload_bytes_task = Arc::clone(&upload_bytes);

        // Spawn task to handle this TCP connection
        tokio::spawn(async move {
            let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();

            // Task to read from local TCP and send to remote
            let send_task = {
                let send_for_read = Arc::clone(&send_for_task);
                let upload_bytes_send = Arc::clone(&upload_bytes_task);
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    loop {
                        match tcp_read.read(&mut buf).await {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                // Track upload bytes
                                upload_bytes_send.fetch_add(n as u64, Ordering::Relaxed);

                                // Send data to remote
                                let data_msg = crate::ClientMessage::TcpData {
                                    stream_id,
                                    data: buf[..n].to_vec(),
                                };
                                let config = bincode::config::standard();
                                let encoded = match bincode::encode_to_vec(&data_msg, config) {
                                    Ok(e) => e,
                                    Err(_) => break,
                                };
                                let len = (encoded.len() as u32).to_be_bytes();

                                let mut send_locked = send_for_read.lock().await;
                                if send_locked.write_all(&len).await.is_err() {
                                    break;
                                }
                                if send_locked.write_all(&encoded).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                })
            };

            // Task to receive from remote and write to local TCP
            let write_task = tokio::spawn(async move {
                while let Some(data) = rx.recv().await {
                    if tcp_write.write_all(&data).await.is_err() {
                        break;
                    }
                }
            });

            // Wait for either task to complete
            tokio::select! {
                _ = send_task => {}
                _ = write_task => {}
            }

            // Send TcpClose message
            let close_msg = crate::ClientMessage::TcpClose { stream_id };
            let config = bincode::config::standard();
            if let Ok(encoded) = bincode::encode_to_vec(&close_msg, config) {
                let len = (encoded.len() as u32).to_be_bytes();
                let mut send_locked = send_for_task.lock().await;
                let _ = send_locked.write_all(&len).await;
                let _ = send_locked.write_all(&encoded).await;
            }

            // Remove from connections map
            tcp_connections_for_task.lock().await.remove(&stream_id);
        });
    }

    // Wait for UI to exit (when user presses 'q')
    let _ = ui_task.await;

    // Send shutdown signal
    let _ = shutdown_tx.send(()).await;

    // Cleanup
    recv_task.abort();

    Ok(())
}
