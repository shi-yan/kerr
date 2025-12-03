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
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;

/// Resume metadata stored in .{filename}.resume_json
#[derive(Debug, Serialize, Deserialize)]
struct ResumeMetadata {
    /// Number of bytes successfully received
    bytes_received: u64,
    /// Total file size expected
    total_size: u64,
    /// Remote path being downloaded
    remote_path: String,
}

/// Get the resume metadata file path for a given local file
fn get_resume_metadata_path(local_path: &str) -> PathBuf {
    let path = Path::new(local_path);
    let parent = path.parent().unwrap_or(Path::new("."));
    let filename = path.file_name().unwrap().to_string_lossy();
    parent.join(format!(".{}.resume_json", filename))
}

/// Read resume metadata if it exists
fn read_resume_metadata(local_path: &str) -> Option<ResumeMetadata> {
    let metadata_path = get_resume_metadata_path(local_path);
    if metadata_path.exists() {
        let content = fs::read_to_string(&metadata_path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

/// Write resume metadata
fn write_resume_metadata(local_path: &str, metadata: &ResumeMetadata) -> std::io::Result<()> {
    let metadata_path = get_resume_metadata_path(local_path);
    let json = serde_json::to_string_pretty(metadata)?;
    fs::write(&metadata_path, json)?;
    Ok(())
}

/// Delete resume metadata file
fn delete_resume_metadata(local_path: &str) -> std::io::Result<()> {
    let metadata_path = get_resume_metadata_path(local_path);
    if metadata_path.exists() {
        fs::remove_file(&metadata_path)?;
    }
    Ok(())
}

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
    use rand::Rng;

    // Decode the compressed connection string (base64 -> gzip -> JSON -> NodeAddr)
    let addr = crate::decode_connection_string(&connection_string)
        .expect("Failed to decode connection string");

    println!("Connecting to: {}", addr.id);

    let endpoint = Endpoint::bind().await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    // Open a connection to the accepting node
    println!("Connecting to Kerr server...");
    let conn = endpoint.connect(addr, ALPN).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;
    println!("Connected! Starting terminal session...");
    println!("Press Ctrl+D to disconnect.");

    // Open a bidirectional QUIC stream
    let (mut send, mut recv) = conn.open_bi().await.e()?;

    // Generate a unique session ID for this shell session
    let session_id = format!("shell_{}", rand::rng().random::<u64>());
    let session_id_for_send = session_id.clone();

    // Send Hello message using the multiplexed protocol
    let hello_msg = ClientMessage::Hello { session_type: crate::SessionType::Shell };
    let hello_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(hello_msg),
    };
    crate::send_envelope(&mut send, &hello_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    // Enter raw mode
    terminal::enable_raw_mode().expect("Failed to enable raw mode");
    let mut stdout = io::stdout();
    stdout.execute(terminal::Clear(ClearType::All)).unwrap();

    // Send initial terminal size using the multiplexed protocol
    if let Ok((cols, rows)) = terminal::size() {
        let resize_msg = ClientMessage::Resize { cols, rows };
        let resize_envelope = crate::MessageEnvelope {
            session_id: session_id.clone(),
            payload: crate::MessagePayload::Client(resize_msg),
        };
        let _ = crate::send_envelope(&mut send, &resize_envelope).await;
    }

    // Channel to send messages to the server
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMessage>();

    // Spawn task to write messages to send stream using the multiplexed protocol
    let send_task = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            let envelope = crate::MessageEnvelope {
                session_id: session_id_for_send.clone(),
                payload: crate::MessagePayload::Client(msg),
            };
            if crate::send_envelope(&mut send, &envelope).await.is_err() {
                break;
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
        loop {
            // Receive message using the multiplexed protocol
            let envelope = match crate::recv_envelope(&mut recv).await {
                Ok(env) => env,
                Err(_) => break, // Connection closed
            };

            // Extract server message from envelope
            let msg = match envelope.payload {
                crate::MessagePayload::Server(server_msg) => server_msg,
                _ => continue, // Ignore non-server messages
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
                ServerMessage::FsDeleteResponse { .. } => {
                    // File deletion response - not used in run_client (only for browse)
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
                ServerMessage::PingResponse { .. } => {
                    // Ping response - not used in run_client (only for ping test)
                }
                ServerMessage::DnsResponse { .. } => {
                    // DNS response - not used in run_client (only for dns proxy)
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
    use rand::Rng;

    // Decode the compressed connection string (base64 -> gzip -> JSON)
    let addr = crate::decode_connection_string(&connection_string)
        .expect("Failed to decode connection string");

    println!("Connecting to server...");
    let endpoint = Endpoint::bind().await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;
    let conn = endpoint.connect(addr, ALPN).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;
    let (mut send, mut recv) = conn.open_bi().await.e()?;

    // Generate a unique session ID for this file transfer
    let session_id = format!("send_{}", rand::rng().random::<u64>());

    // Send Hello message using the multiplexed protocol
    let hello_msg = ClientMessage::Hello { session_type: crate::SessionType::FileTransfer };
    let hello_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(hello_msg),
    };
    crate::send_envelope(&mut send, &hello_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

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

    // Send upload start message using the multiplexed protocol
    let start_msg = ClientMessage::StartUpload {
        path: actual_remote_path.clone(),
        size: total_size,
        is_dir,
        force,
    };
    let start_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(start_msg),
    };
    crate::send_envelope(&mut send, &start_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    // Wait for ack or error
    let response_envelope = crate::recv_envelope(&mut recv).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    // Check if we got UploadAck, ConfirmPrompt, or Error
    match response_envelope.payload {
        crate::MessagePayload::Server(response) => match response {
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

            // Send confirmation response using the multiplexed protocol
            let confirm_msg = ClientMessage::ConfirmResponse { confirmed };
            let confirm_envelope = crate::MessageEnvelope {
                session_id: session_id.clone(),
                payload: crate::MessagePayload::Client(confirm_msg),
            };
            crate::send_envelope(&mut send, &confirm_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

            if !confirmed {
                println!("Upload cancelled.");
                return Ok(());
            }

            // Wait for final ack after confirmation
            let final_envelope = crate::recv_envelope(&mut recv).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

            match final_envelope.payload {
                crate::MessagePayload::Server(ServerMessage::UploadAck) => {
                    // Good to proceed
                }
                crate::MessagePayload::Server(ServerMessage::Error { message }) => {
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
        _ => {
            eprintln!("Unexpected message type");
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

            // Send chunk using the multiplexed protocol
            let chunk_msg = ClientMessage::FileChunk {
                data: buffer[..n].to_vec(),
            };
            let chunk_envelope = crate::MessageEnvelope {
                session_id: session_id.clone(),
                payload: crate::MessagePayload::Client(chunk_msg),
            };
            crate::send_envelope(&mut send, &chunk_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

            bytes_sent += n as u64;
            pb.set_position(bytes_sent);
        }
    }

    // Send end message using the multiplexed protocol
    let end_msg = ClientMessage::EndUpload;
    let end_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(end_msg),
    };
    crate::send_envelope(&mut send, &end_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    pb.finish_with_message("Upload complete!");

    conn.close(0u32.into(), b"done");
    endpoint.close().await;

    Ok(())
}

/// Pull a file or directory from the server
pub async fn pull_file(connection_string: String, remote_path: String, local_path: String) -> Result<()> {
    use std::path::Path;
    use std::fs;
    use std::io::{Write, Seek, SeekFrom};
    use indicatif::{ProgressBar, ProgressStyle};
    use rand::Rng;

    // Check for existing resume metadata
    let resume_metadata = read_resume_metadata(&local_path);
    let resume_offset = resume_metadata.as_ref().map(|m| m.bytes_received).unwrap_or(0);

    if let Some(ref metadata) = resume_metadata {
        if metadata.remote_path == remote_path {
            println!("Found incomplete download, resuming from {} bytes...", metadata.bytes_received);
        } else {
            println!("Warning: Resume metadata points to different remote file, starting fresh");
            let _ = delete_resume_metadata(&local_path);
        }
    }

    // Decode the compressed connection string (base64 -> gzip -> JSON)
    let addr = crate::decode_connection_string(&connection_string)
        .expect("Failed to decode connection string");

    println!("Connecting to server...");
    let endpoint = Endpoint::bind().await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;
    let conn = endpoint.connect(addr, ALPN).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;
    let (mut send, mut recv) = conn.open_bi().await.e()?;

    // Generate a unique session ID for this file transfer
    let session_id = format!("pull_{}", rand::rng().random::<u64>());

    // Send Hello message using the multiplexed protocol
    let hello_msg = ClientMessage::Hello { session_type: crate::SessionType::FileTransfer };
    let hello_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(hello_msg),
    };
    crate::send_envelope(&mut send, &hello_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    // Send RequestDownload message with offset for resume support
    let request_msg = ClientMessage::RequestDownload {
        path: remote_path.clone(),
        offset: resume_offset,
    };
    let request_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(request_msg),
    };
    crate::send_envelope(&mut send, &request_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    // Wait for StartDownload or Error
    let response_envelope = crate::recv_envelope(&mut recv).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    let (total_size, _is_dir) = match response_envelope.payload {
        crate::MessagePayload::Server(ServerMessage::StartDownload { size, is_dir }) => (size, is_dir),
        crate::MessagePayload::Server(ServerMessage::Error { message }) => {
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

    // Open file for writing - append if resuming, create if new
    let mut output_file = if resume_offset > 0 {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&local_path)
            .expect("Failed to open file for resuming");
        // Verify file size matches resume offset
        let file_size = file.metadata().expect("Failed to get file metadata").len();
        if file_size != resume_offset {
            eprintln!("Warning: File size mismatch, starting fresh");
            drop(file);
            let _ = delete_resume_metadata(&local_path);
            fs::File::create(&local_path).expect("Failed to create output file")
        } else {
            file
        }
    } else {
        fs::File::create(&local_path).expect("Failed to create output file")
    };

    // Create progress bar
    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    let mut bytes_received = resume_offset;
    pb.set_position(bytes_received);

    // Receive file chunks using the multiplexed protocol
    let mut chunk_count = 0u64;
    loop {
        let envelope = crate::recv_envelope(&mut recv).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

        match envelope.payload {
            crate::MessagePayload::Server(ServerMessage::FileChunk { data }) => {
                output_file.write_all(&data)
                    .expect("Failed to write to file");
                bytes_received += data.len() as u64;
                pb.set_position(bytes_received);

                // Update resume metadata every 10 chunks to avoid excessive I/O
                chunk_count += 1;
                if chunk_count % 10 == 0 {
                    let metadata = ResumeMetadata {
                        bytes_received,
                        total_size,
                        remote_path: remote_path.clone(),
                    };
                    let _ = write_resume_metadata(&local_path, &metadata);
                }
            }
            crate::MessagePayload::Server(ServerMessage::EndDownload) => {
                pb.finish_with_message("Download complete!");
                // Delete resume metadata on successful completion
                let _ = delete_resume_metadata(&local_path);
                break;
            }
            crate::MessagePayload::Server(ServerMessage::Error { message }) => {
                eprintln!("Server error: {}", message);
                pb.finish_with_message("Download failed");
                // Save resume metadata on error
                let metadata = ResumeMetadata {
                    bytes_received,
                    total_size,
                    remote_path: remote_path.clone(),
                };
                let _ = write_resume_metadata(&local_path, &metadata);
                return Ok(());
            }
            _ => {
                eprintln!("Unexpected server message during download");
                // Save resume metadata on unexpected error
                let metadata = ResumeMetadata {
                    bytes_received,
                    total_size,
                    remote_path: remote_path.clone(),
                };
                let _ = write_resume_metadata(&local_path, &metadata);
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
    let endpoint = Endpoint::bind().await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;
    let conn = endpoint.connect(addr, ALPN).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;
    let (mut send, mut recv) = conn.open_bi().await.e()?;

    // Generate a unique session ID for this ping session
    use rand::Rng;
    let session_id = format!("ping_{}", rand::rng().random::<u64>());

    // Send Hello message to indicate this is a ping test session
    let hello_msg = ClientMessage::Hello { session_type: crate::SessionType::Ping };
    let hello_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(hello_msg),
    };
    crate::send_envelope(&mut send, &hello_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

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
        let ping_envelope = crate::MessageEnvelope {
            session_id: session_id.clone(),
            payload: crate::MessagePayload::Client(ping_msg),
        };
        crate::send_envelope(&mut send, &ping_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

        // Receive response
        let response_envelope = crate::recv_envelope(&mut recv).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

        // Stop timer
        let elapsed = start.elapsed();

        // Extract the server message from the envelope
        match response_envelope.payload {
            crate::MessagePayload::Server(ServerMessage::PingResponse { data }) => {
                // Verify we got the same size back
                if data.len() != size {
                    eprintln!("Warning: Expected {} bytes back, got {}", size, data.len());
                }

                // Calculate metrics
                let rtt_ms = elapsed.as_secs_f64() * 1000.0;

                // Estimate total bytes transferred (both directions, including protocol overhead)
                // Envelope overhead includes session_id string + bincode encoding overhead
                let estimated_request_overhead = session_id.len() + 50; // rough estimate
                let estimated_response_overhead = session_id.len() + 50;
                let total_bytes = size + estimated_request_overhead + size + estimated_response_overhead;

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
    let disconnect_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(disconnect_msg),
    };
    crate::send_envelope(&mut send, &disconnect_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    conn.close(0u32.into(), b"done");
    endpoint.close().await;

    Ok(())
}

/// Browse remote filesystem
pub async fn browse_remote(connection_string: String) -> Result<()> {
    use std::sync::Arc;
    use std::path::PathBuf;
    use rand::Rng;

    // Decode connection string
    let addr = crate::decode_connection_string(&connection_string)
        .expect("Failed to decode connection string");

    println!("Connecting to server for file browsing...");
    let endpoint = Endpoint::bind().await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;
    let conn = endpoint.connect(addr, ALPN).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    let (mut send, recv) = conn.open_bi().await.e()?;

    // Generate a unique session ID for this browser session
    let session_id = format!("browser_{}", rand::rng().random::<u64>());

    // Send Hello message using the multiplexed protocol
    let hello = ClientMessage::Hello {
        session_type: crate::SessionType::FileBrowser,
    };
    let hello_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(hello),
    };
    crate::send_envelope(&mut send, &hello_envelope).await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    println!("Connected! Starting file browser...");

    // Create RemoteFilesystem
    use crate::custom_explorer::filesystem::RemoteFilesystem;
    let remote_fs = Arc::new(RemoteFilesystem::new_with_session_id(
        PathBuf::from("/"),
        send,
        recv,
        session_id,
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
    use rand::Rng;

    // Decode connection string and connect to server
    let node_addr = crate::decode_connection_string(connection_string)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to decode connection string: {}", e)))?;

    let endpoint = iroh::Endpoint::bind()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to create endpoint: {}", e)))?;

    let conn = endpoint.connect(node_addr, crate::ALPN)
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to connect: {}", e)))?;

    let (mut send, mut recv) = conn.open_bi()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to open stream: {}", e)))?;

    // Generate a unique session ID for this relay session
    let session_id = format!("relay_{}", rand::rng().random::<u64>());
    let session_id_for_send = session_id.clone();

    // Send Hello message using the multiplexed protocol
    let hello = crate::ClientMessage::Hello {
        session_type: crate::SessionType::TcpRelay,
    };
    let hello_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(hello),
    };
    crate::send_envelope(&mut send, &hello_envelope).await
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
            // Receive message using the multiplexed protocol
            let envelope = match crate::recv_envelope(&mut recv).await {
                Ok(env) => env,
                Err(_) => break,
            };

            // Extract server message from envelope
            let msg = match envelope.payload {
                crate::MessagePayload::Server(server_msg) => server_msg,
                _ => continue,
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

        // Send TcpOpen message using the multiplexed protocol
        let open_msg = crate::ClientMessage::TcpOpen {
            stream_id,
            destination_host: None,  // Connect to localhost on remote server
            destination_port: remote_port,
        };
        let open_envelope = crate::MessageEnvelope {
            session_id: session_id_for_send.clone(),
            payload: crate::MessagePayload::Client(open_msg),
        };

        {
            let mut send_locked = send_clone.lock().await;
            if let Err(e) = crate::send_envelope(&mut *send_locked, &open_envelope).await {
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
        let session_id_for_task = session_id_for_send.clone();

        // Spawn task to handle this TCP connection
        tokio::spawn(async move {
            let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();

            // Task to read from local TCP and send to remote
            let send_task = {
                let send_for_read = Arc::clone(&send_for_task);
                let upload_bytes_send = Arc::clone(&upload_bytes_task);
                let session_id_for_read = session_id_for_task.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    loop {
                        match tcp_read.read(&mut buf).await {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                // Track upload bytes
                                upload_bytes_send.fetch_add(n as u64, Ordering::Relaxed);

                                // Send data to remote using the multiplexed protocol
                                let data_msg = crate::ClientMessage::TcpData {
                                    stream_id,
                                    data: buf[..n].to_vec(),
                                };
                                let data_envelope = crate::MessageEnvelope {
                                    session_id: session_id_for_read.clone(),
                                    payload: crate::MessagePayload::Client(data_msg),
                                };

                                let mut send_locked = send_for_read.lock().await;
                                if crate::send_envelope(&mut *send_locked, &data_envelope).await.is_err() {
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

            // Send TcpClose message using the multiplexed protocol
            let close_msg = crate::ClientMessage::TcpClose { stream_id };
            let close_envelope = crate::MessageEnvelope {
                session_id: session_id_for_task.clone(),
                payload: crate::MessagePayload::Client(close_msg),
            };
            let mut send_locked = send_for_task.lock().await;
            let _ = crate::send_envelope(&mut *send_locked, &close_envelope).await;

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

/// Run an HTTP/HTTPS proxy that relays traffic through the Kerr connection
pub async fn run_proxy(
    connection_string: &str,
    port: u16,
    enable_dns: bool,
) -> Result<()> {
    use tokio::net::TcpListener;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
    use rand::Rng;

    // Decode connection string and connect to server
    let node_addr = crate::decode_connection_string(connection_string)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to decode connection string: {}", e)))?;

    let endpoint = iroh::Endpoint::bind()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to create endpoint: {}", e)))?;

    let conn = endpoint.connect(node_addr, crate::ALPN)
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to connect: {}", e)))?;

    // Start DNS proxy if requested
    let _dns_task = if enable_dns {
        let conn_clone = conn.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = start_dns_proxy_task(conn_clone).await {
                eprintln!("DNS proxy error: {}", e);
            }
        }))
    } else {
        None
    };

    let (mut send, mut recv) = conn.open_bi()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to open stream: {}", e)))?;

    // Generate a unique session ID for this proxy session
    let session_id = format!("proxy_{}", rand::rng().random::<u64>());
    let session_id_for_send = session_id.clone();

    // Send Hello message using the multiplexed protocol
    let hello = crate::ClientMessage::Hello {
        session_type: crate::SessionType::HttpProxy,
    };
    let hello_envelope = crate::MessageEnvelope {
        session_id: session_id.clone(),
        payload: crate::MessagePayload::Client(hello),
    };
    crate::send_envelope(&mut send, &hello_envelope).await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to send hello: {}", e)))?;

    // Listen on local port
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to bind to port {}: {}", port, e)))?;

    println!("HTTP/HTTPS proxy listening on 127.0.0.1:{}", port);
    println!("Configure your browser to use this as an HTTP proxy");
    if enable_dns {
        println!("DNS proxy also running on 127.0.0.1:53");
    }
    println!("Press Ctrl+C to stop");

    // Shared state for tracking TCP connections
    let tcp_connections: Arc<Mutex<HashMap<u32, tokio::sync::mpsc::Sender<Vec<u8>>>>> = Arc::new(Mutex::new(HashMap::new()));
    let next_stream_id = Arc::new(AtomicU32::new(1));

    // Wrap send stream in Arc<Mutex> for sharing between tasks
    let send = Arc::new(Mutex::new(send));
    let send_clone = Arc::clone(&send);

    // Task to handle incoming messages from server
    let tcp_connections_clone = Arc::clone(&tcp_connections);
    let _recv_task = tokio::spawn(async move {
        loop {
            // Receive message using the multiplexed protocol
            let envelope = match crate::recv_envelope(&mut recv).await {
                Ok(env) => env,
                Err(_) => break,
            };

            // Extract server message from envelope
            let msg = match envelope.payload {
                crate::MessagePayload::Server(server_msg) => server_msg,
                _ => continue,
            };

            // Handle server messages
            match msg {
                crate::ServerMessage::TcpDataResponse { stream_id, data } => {
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

    // Accept incoming HTTP/HTTPS connections
    loop {
        let (mut client_socket, client_addr) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                continue;
            }
        };

        println!("Accepted connection from {}", client_addr);

        let send_for_task = Arc::clone(&send_clone);
        let tcp_connections_for_task = Arc::clone(&tcp_connections);
        let next_stream_id_for_task = Arc::clone(&next_stream_id);
        let session_id_for_task = session_id_for_send.clone();

        // Spawn task to handle this HTTP connection
        tokio::spawn(async move {
            // Read the initial HTTP request to determine the target
            let mut buffer = vec![0u8; 8192];
            let bytes_read = match client_socket.read(&mut buffer).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            let request = String::from_utf8_lossy(&buffer[..bytes_read]);
            let mut headers = request.split("\r\n");

            let request_line = match headers.next() {
                Some(line) => line,
                None => return,
            };

            // Parse the target host and port
            let (target_host, target_port, is_connect) = if request_line.starts_with("CONNECT") {
                // CONNECT method for HTTPS
                let parts: Vec<&str> = request_line.split_whitespace().collect();
                if parts.len() < 2 {
                    return;
                }
                let host_port = parts[1];
                let (host, port) = if let Some(colon_pos) = host_port.rfind(':') {
                    let h = &host_port[..colon_pos];
                    let p = host_port[colon_pos + 1..].parse::<u16>().unwrap_or(443);
                    (h.to_string(), p)
                } else {
                    (host_port.to_string(), 443)
                };
                println!("CONNECT request to {}:{}", host, port);
                (host, port, true)
            } else {
                // Regular HTTP request - extract Host header
                let mut host_value = None;
                for line in headers {
                    if line.to_lowercase().starts_with("host:") {
                        host_value = line.split_whitespace().nth(1);
                        break;
                    }
                }

                let host_port = match host_value {
                    Some(h) => h,
                    None => {
                        eprintln!("No Host header found in HTTP request");
                        return;
                    }
                };

                let (host, port) = if let Some(colon_pos) = host_port.rfind(':') {
                    let h = &host_port[..colon_pos];
                    let p = host_port[colon_pos + 1..].parse::<u16>().unwrap_or(80);
                    (h.to_string(), p)
                } else {
                    (host_port.to_string(), 80)
                };

                println!("HTTP request to {}:{}", host, port);
                (host, port, false)
            };

            // Get next stream ID
            let stream_id = next_stream_id_for_task.fetch_add(1, Ordering::Relaxed);

            // Send TcpOpen message using the multiplexed protocol
            let open_msg = crate::ClientMessage::TcpOpen {
                stream_id,
                destination_host: Some(target_host),
                destination_port: target_port,
            };
            let open_envelope = crate::MessageEnvelope {
                session_id: session_id_for_task.clone(),
                payload: crate::MessagePayload::Client(open_msg),
            };

            {
                let mut send_locked = send_for_task.lock().await;
                if let Err(e) = crate::send_envelope(&mut *send_locked, &open_envelope).await {
                    eprintln!("Failed to send TcpOpen: {}", e);
                    return;
                }
            }

            // Create channel for receiving data from server
            let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(100);
            tcp_connections_for_task.lock().await.insert(stream_id, tx);

            // For CONNECT, send 200 OK response to client
            if is_connect {
                let response = b"HTTP/1.1 200 Connection Established\r\n\r\n";
                if let Err(_) = client_socket.write_all(response).await {
                    return;
                }
            } else {
                // For HTTP, send the original request to the remote server using the multiplexed protocol
                let data_msg = crate::ClientMessage::TcpData {
                    stream_id,
                    data: buffer[..bytes_read].to_vec(),
                };
                let data_envelope = crate::MessageEnvelope {
                    session_id: session_id_for_task.clone(),
                    payload: crate::MessagePayload::Client(data_msg),
                };

                let mut send_locked = send_for_task.lock().await;
                if crate::send_envelope(&mut *send_locked, &data_envelope).await.is_err() {
                    return;
                }
                drop(send_locked);
            }

            let (mut client_read, mut client_write) = client_socket.into_split();

            // Task to read from client and send to remote
            let send_task = {
                let send_for_read = Arc::clone(&send_for_task);
                let session_id_for_read = session_id_for_task.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    loop {
                        match client_read.read(&mut buf).await {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                // Send data to remote using the multiplexed protocol
                                let data_msg = crate::ClientMessage::TcpData {
                                    stream_id,
                                    data: buf[..n].to_vec(),
                                };
                                let data_envelope = crate::MessageEnvelope {
                                    session_id: session_id_for_read.clone(),
                                    payload: crate::MessagePayload::Client(data_msg),
                                };

                                let mut send_locked = send_for_read.lock().await;
                                if crate::send_envelope(&mut *send_locked, &data_envelope).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                })
            };

            // Task to receive from remote and write to client
            let write_task = tokio::spawn(async move {
                while let Some(data) = rx.recv().await {
                    if client_write.write_all(&data).await.is_err() {
                        break;
                    }
                }
            });

            // Wait for either task to complete
            tokio::select! {
                _ = send_task => {}
                _ = write_task => {}
            }

            // Send TcpClose message using the multiplexed protocol
            let close_msg = crate::ClientMessage::TcpClose { stream_id };
            let close_envelope = crate::MessageEnvelope {
                session_id: session_id_for_task.clone(),
                payload: crate::MessagePayload::Client(close_msg),
            };
            let mut send_locked = send_for_task.lock().await;
            let _ = crate::send_envelope(&mut *send_locked, &close_envelope).await;

            // Remove from connections map
            tcp_connections_for_task.lock().await.remove(&stream_id);

            println!("Connection closed for stream {}", stream_id);
        });
    }
}

/// Helper function to start DNS proxy using an existing connection
async fn start_dns_proxy_task(conn: iroh::endpoint::Connection) -> Result<()> {
    use tokio::net::UdpSocket;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::net::SocketAddr;

    let (mut send, mut recv) = conn.open_bi()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to open stream: {}", e)))?;

    // Send Hello message with Dns session type
    let hello = crate::ClientMessage::Hello {
        session_type: crate::SessionType::Dns,
    };
    let config = bincode::config::standard();
    let encoded = bincode::encode_to_vec(&hello, config)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to encode hello: {}", e)))?;
    let len = (encoded.len() as u32).to_be_bytes();
    send.write_all(&len).await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to send length: {}", e)))?;
    send.write_all(&encoded).await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to send hello: {}", e)))?;

    // Bind UDP socket for DNS (port 53)
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:53")
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to bind to UDP port 53: {}. You may need sudo/admin privileges.", e)))?);

    println!("DNS server listening on 127.0.0.1:53");

    // Track pending queries: query_id -> (client_addr, original_transaction_id)
    let pending_queries: Arc<Mutex<HashMap<u32, (SocketAddr, u16)>>> = Arc::new(Mutex::new(HashMap::new()));
    let next_query_id = Arc::new(AtomicU32::new(1));

    // Wrap send stream in Arc<Mutex> for sharing between tasks
    let send = Arc::new(Mutex::new(send));
    let send_clone = Arc::clone(&send);
    let socket_clone = Arc::clone(&socket);

    // Task to handle incoming DNS responses from server
    let pending_queries_clone = Arc::clone(&pending_queries);
    let _recv_task = tokio::spawn(async move {
        let config = bincode::config::standard();
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
            let (msg, _): (crate::ServerMessage, _) = match bincode::decode_from_slice(&msg_bytes, config) {
                Ok(m) => m,
                Err(_) => break,
            };

            // Handle DNS response
            match msg {
                crate::ServerMessage::DnsResponse { query_id, response_data } => {
                    // Look up the original client address and transaction ID
                    let pending = pending_queries_clone.lock().await.remove(&query_id);

                    if let Some((client_addr, _original_tid)) = pending {
                        // Send response back to client
                        if let Err(e) = socket_clone.send_to(&response_data, &client_addr).await {
                            eprintln!("Failed to send DNS response to {}: {}", client_addr, e);
                        } else {
                            println!("Sent DNS response to {} ({} bytes)", client_addr, response_data.len());
                        }
                    }
                }
                _ => {}
            }
        }
    });

    // Main loop: accept incoming DNS queries
    let mut buffer = vec![0u8; 512]; // Standard DNS UDP packet size
    loop {
        let (len, client_addr) = match socket.recv_from(&mut buffer).await {
            Ok(x) => x,
            Err(e) => {
                eprintln!("Failed to receive DNS query: {}", e);
                continue;
            }
        };

        println!("Received DNS query from {} ({} bytes)", client_addr, len);

        // Parse DNS query to extract transaction ID and ensure it's valid
        let query_data = buffer[..len].to_vec();

        // Log the query for debugging
        if let Ok(packet) = simple_dns::Packet::parse(&query_data) {
            if let Some(question) = packet.questions.first() {
                println!("  Query: {} (type: {:?})", question.qname, question.qtype);
            }
        }

        // Get next query ID for tracking
        let query_id = next_query_id.fetch_add(1, Ordering::Relaxed);

        // Extract transaction ID from the DNS packet (first 2 bytes)
        let transaction_id = if query_data.len() >= 2 {
            u16::from_be_bytes([query_data[0], query_data[1]])
        } else {
            0
        };

        // Store the mapping so we can send the response back to the right client
        pending_queries.lock().await.insert(query_id, (client_addr, transaction_id));

        // Send DNS query to remote server via P2P
        let dns_msg = crate::ClientMessage::DnsQuery {
            query_id,
            query_data,
        };

        let config = bincode::config::standard();
        let encoded = match bincode::encode_to_vec(&dns_msg, config) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to encode DnsQuery: {}", e);
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
                eprintln!("Failed to send DnsQuery: {}", e);
                break;
            }
        }

        println!("Forwarded DNS query {} to remote server", query_id);
    }

    #[allow(unreachable_code)]
    Ok(())
}

/// Run a DNS server that forwards queries through the Kerr connection
pub async fn run_dns_proxy(
    connection_string: &str,
    port: u16,
) -> Result<()> {
    use tokio::net::UdpSocket;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::net::SocketAddr;

    // Decode connection string and connect to server
    let node_addr = crate::decode_connection_string(connection_string)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to decode connection string: {}", e)))?;

    let endpoint = iroh::Endpoint::bind()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to create endpoint: {}", e)))?;

    let conn = endpoint.connect(node_addr, crate::ALPN)
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to connect: {}", e)))?;

    let (mut send, mut recv) = conn.open_bi()
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to open stream: {}", e)))?;

    // Send Hello message with Dns session type
    let hello = crate::ClientMessage::Hello {
        session_type: crate::SessionType::Dns,
    };
    let config = bincode::config::standard();
    let encoded = bincode::encode_to_vec(&hello, config)
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to encode hello: {}", e)))?;
    let len = (encoded.len() as u32).to_be_bytes();
    send.write_all(&len).await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to send length: {}", e)))?;
    send.write_all(&encoded).await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to send hello: {}", e)))?;

    // Bind UDP socket for DNS
    let socket = Arc::new(UdpSocket::bind(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to bind to UDP port {}: {}. You may need sudo/admin privileges.", port, e)))?);

    println!("DNS server listening on 127.0.0.1:{}", port);
    println!("Configure your system to use this as DNS server (127.0.0.1)");
    println!("Press Ctrl+C to stop");

    // Track pending queries: query_id -> (client_addr, original_transaction_id)
    let pending_queries: Arc<Mutex<HashMap<u32, (SocketAddr, u16)>>> = Arc::new(Mutex::new(HashMap::new()));
    let next_query_id = Arc::new(AtomicU32::new(1));

    // Wrap send stream in Arc<Mutex> for sharing between tasks
    let send = Arc::new(Mutex::new(send));
    let send_clone = Arc::clone(&send);
    let socket_clone = Arc::clone(&socket);

    // Task to handle incoming DNS responses from server
    let pending_queries_clone = Arc::clone(&pending_queries);
    let _recv_task = tokio::spawn(async move {
        let config = bincode::config::standard();
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
            let (msg, _): (crate::ServerMessage, _) = match bincode::decode_from_slice(&msg_bytes, config) {
                Ok(m) => m,
                Err(_) => break,
            };

            // Handle DNS response
            match msg {
                crate::ServerMessage::DnsResponse { query_id, response_data } => {
                    // Look up the original client address and transaction ID
                    let pending = pending_queries_clone.lock().await.remove(&query_id);

                    if let Some((client_addr, _original_tid)) = pending {
                        // Send response back to client
                        // Note: The response_data already contains the correct transaction ID
                        // because the server preserved it
                        if let Err(e) = socket_clone.send_to(&response_data, &client_addr).await {
                            eprintln!("Failed to send DNS response to {}: {}", client_addr, e);
                        } else {
                            println!("Sent DNS response to {} ({} bytes)", client_addr, response_data.len());
                        }
                    }
                }
                _ => {}
            }
        }
    });

    // Main loop: accept incoming DNS queries
    let mut buffer = vec![0u8; 512]; // Standard DNS UDP packet size
    loop {
        let (len, client_addr) = match socket.recv_from(&mut buffer).await {
            Ok(x) => x,
            Err(e) => {
                eprintln!("Failed to receive DNS query: {}", e);
                continue;
            }
        };

        println!("Received DNS query from {} ({} bytes)", client_addr, len);

        // Parse DNS query to extract transaction ID and ensure it's valid
        let query_data = buffer[..len].to_vec();

        // We can optionally parse the DNS packet here to log what's being queried
        // but we preserve the entire packet for forwarding
        if let Ok(packet) = simple_dns::Packet::parse(&query_data) {
            // Log the query for debugging
            if let Some(question) = packet.questions.first() {
                println!("  Query: {} (type: {:?})", question.qname, question.qtype);
            }
        }

        // Get next query ID for tracking
        let query_id = next_query_id.fetch_add(1, Ordering::Relaxed);

        // Extract transaction ID from the DNS packet (first 2 bytes)
        let transaction_id = if query_data.len() >= 2 {
            u16::from_be_bytes([query_data[0], query_data[1]])
        } else {
            0
        };

        // Store the mapping so we can send the response back to the right client
        pending_queries.lock().await.insert(query_id, (client_addr, transaction_id));

        // Send DNS query to remote server via P2P
        let dns_msg = crate::ClientMessage::DnsQuery {
            query_id,
            query_data,
        };

        let config = bincode::config::standard();
        let encoded = match bincode::encode_to_vec(&dns_msg, config) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to encode DnsQuery: {}", e);
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
                eprintln!("Failed to send DnsQuery: {}", e);
                break;
            }
        }

        println!("Forwarded DNS query {} to remote server", query_id);
    }

    // This line is unreachable in practice, but needed for type checking
    #[allow(unreachable_code)]
    Ok(())
}
