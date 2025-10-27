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
    let (mut send, recv) = conn.open_bi().await.e()?;

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
                ServerMessage::PingResponse { .. } => {
                    // Ping response - not used in run_client (only for ping test)
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
pub async fn pull_file(connection_string: String, remote_path: String, local_path: String) -> Result<()> {
    use std::path::Path;
    use std::fs;
    use std::io::Write;
    use indicatif::{ProgressBar, ProgressStyle};

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

    // Send RequestDownload message
    let request_msg = ClientMessage::RequestDownload {
        path: remote_path.clone(),
    };
    let encoded = bincode::encode_to_vec(&request_msg, config).unwrap();
    let len = (encoded.len() as u32).to_be_bytes();
    send.write_all(&len).await.e()?;
    send.write_all(&encoded).await.e()?;

    // Wait for StartDownload or Error
    let mut len_bytes = [0u8; 4];
    recv.read_exact(&mut len_bytes).await.e()?;
    let msg_len = u32::from_be_bytes(len_bytes) as usize;
    let mut msg_bytes = vec![0u8; msg_len];
    recv.read_exact(&mut msg_bytes).await.e()?;

    let (response, _): (ServerMessage, _) = bincode::decode_from_slice(&msg_bytes, config)
        .expect("Failed to decode server response");

    let (total_size, _is_dir) = match response {
        ServerMessage::StartDownload { size, is_dir } => (size, is_dir),
        ServerMessage::Error { message } => {
            eprintln!("Server error: {}", message);
            return Ok(());
        }
        _ => {
            eprintln!("Unexpected server response");
            return Ok(());
        }
    };

    println!("Downloading {} ({} bytes)...", remote_path, total_size);

    // Ensure parent directory exists
    let local = Path::new(&local_path);
    crate::transfer::ensure_parent_dir(local)
        .expect("Failed to create parent directory");

    // Open file for writing
    let mut output_file = fs::File::create(&local_path)
        .expect("Failed to create output file");

    // Create progress bar
    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    let mut bytes_received = 0u64;

    // Receive file chunks
    loop {
        // Read message length
        let mut len_bytes = [0u8; 4];
        recv.read_exact(&mut len_bytes).await.e()?;
        let msg_len = u32::from_be_bytes(len_bytes) as usize;
        let mut msg_bytes = vec![0u8; msg_len];
        recv.read_exact(&mut msg_bytes).await.e()?;

        let (msg, _): (ServerMessage, _) = bincode::decode_from_slice(&msg_bytes, config)
            .expect("Failed to decode server message");

        match msg {
            ServerMessage::FileChunk { data } => {
                output_file.write_all(&data)
                    .expect("Failed to write to file");
                bytes_received += data.len() as u64;
                pb.set_position(bytes_received);
            }
            ServerMessage::EndDownload => {
                pb.finish_with_message("Download complete!");
                break;
            }
            ServerMessage::Error { message } => {
                eprintln!("Server error: {}", message);
                pb.finish_with_message("Download failed");
                return Ok(());
            }
            _ => {
                eprintln!("Unexpected server message during download");
                break;
            }
        }
    }

    println!("Downloaded {} to {}", remote_path, local_path);

    conn.close(0u32.into(), b"done");
    endpoint.close().await;

    Ok(())
}

/// Test network performance with increasing payload sizes
pub async fn ping_test(connection_string: String) -> Result<()> {
    use std::time::Instant;

    // Decode the compressed connection string (base64 -> gzip -> JSON)
    let addr = crate::decode_connection_string(&connection_string)
        .expect("Failed to decode connection string");

    println!("Connecting to server...");
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;
    let conn = endpoint.connect(addr, ALPN).await?;
    let (mut send, mut recv) = conn.open_bi().await.e()?;

    // Send Hello message to indicate this is a ping test session
    let config = config::standard();
    let hello_msg = ClientMessage::Hello { session_type: crate::SessionType::Ping };
    let encoded = bincode::encode_to_vec(&hello_msg, config).unwrap();
    let len = (encoded.len() as u32).to_be_bytes();
    send.write_all(&len).await.e()?;
    send.write_all(&encoded).await.e()?;

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                    Network Performance Test                          ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");
    println!("{:<12} {:<15} {:<15} {:<15}", "Payload Size", "Round-Trip", "Throughput", "Effective BW");
    println!("{}", "─".repeat(70));

    // Test with exponentially growing payload sizes: 0, 1KB, 4KB, 16KB, 64KB, 256KB, 1MB
    let sizes = vec![0, 1024, 4096, 16384, 65536, 262144, 1048576];

    for size in sizes {
        // Create payload
        let payload = vec![0u8; size];

        // Start timer
        let start = Instant::now();

        // Send ping request
        let ping_msg = ClientMessage::PingRequest { data: payload };
        let encoded = bincode::encode_to_vec(&ping_msg, config).unwrap();
        let len = (encoded.len() as u32).to_be_bytes();
        send.write_all(&len).await.e()?;
        send.write_all(&encoded).await.e()?;

        // Receive response
        let mut len_bytes = [0u8; 4];
        recv.read_exact(&mut len_bytes).await.e()?;
        let msg_len = u32::from_be_bytes(len_bytes) as usize;
        let mut msg_bytes = vec![0u8; msg_len];
        recv.read_exact(&mut msg_bytes).await.e()?;

        // Stop timer
        let elapsed = start.elapsed();

        // Decode response
        let (response, _): (ServerMessage, _) = bincode::decode_from_slice(&msg_bytes, config)
            .expect("Failed to decode server response");

        match response {
            ServerMessage::PingResponse { data } => {
                // Verify we got the same size back
                if data.len() != size {
                    eprintln!("Warning: Expected {} bytes back, got {}", size, data.len());
                }

                // Calculate metrics
                let rtt_ms = elapsed.as_secs_f64() * 1000.0;

                // Total bytes transferred (both directions, including protocol overhead)
                let encoded_size = encoded.len();
                let response_size = msg_bytes.len() + 4; // +4 for length prefix
                let total_bytes = encoded_size + 4 + response_size; // +4 for request length prefix

                // Throughput in MB/s (total data / time)
                let throughput_mbps = if elapsed.as_secs_f64() > 0.0 {
                    (total_bytes as f64) / elapsed.as_secs_f64() / 1_000_000.0
                } else {
                    0.0
                };

                // Effective bandwidth (payload only, both directions) in Mbps
                let effective_bw_mbps = if elapsed.as_secs_f64() > 0.0 {
                    (size as f64 * 2.0 * 8.0) / elapsed.as_secs_f64() / 1_000_000.0
                } else {
                    0.0
                };

                // Format size nicely
                let size_str = if size == 0 {
                    "0 B".to_string()
                } else if size < 1024 {
                    format!("{} B", size)
                } else if size < 1048576 {
                    format!("{} KB", size / 1024)
                } else {
                    format!("{} MB", size / 1048576)
                };

                println!(
                    "{:<12} {:<15} {:<15} {:<15}",
                    size_str,
                    format!("{:.2} ms", rtt_ms),
                    format!("{:.2} MB/s", throughput_mbps),
                    format!("{:.2} Mbps", effective_bw_mbps)
                );
            }
            _ => {
                eprintln!("Unexpected server response");
                break;
            }
        }
    }

    println!("\n{}", "─".repeat(70));
    println!("Test complete!\n");

    // Send disconnect
    let disconnect_msg = ClientMessage::Disconnect;
    let encoded = bincode::encode_to_vec(&disconnect_msg, config).unwrap();
    let len = (encoded.len() as u32).to_be_bytes();
    send.write_all(&len).await.e()?;
    send.write_all(&encoded).await.e()?;

    conn.close(0u32.into(), b"done");
    endpoint.close().await;

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
