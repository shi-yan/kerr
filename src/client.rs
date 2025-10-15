//! Kerr client - connects to server and provides interactive terminal

use iroh::{Endpoint, NodeAddr};
use n0_snafu::{Result, ResultExt};
use std::io::{self, Write};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{self, ClearType},
    ExecutableCommand,
};
use crate::{ClientMessage, ServerMessage, ALPN};
use bincode::config;
use base64::Engine;

pub async fn run_client(connection_string: String) -> Result<()> {
    // Decode the connection string (base64 -> JSON -> NodeAddr)
    let addr_json = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(connection_string.as_bytes())
        .expect("Failed to decode connection string");

    let addr_json_str = std::str::from_utf8(&addr_json)
        .expect("Invalid UTF-8 in connection string");

    let addr: NodeAddr = serde_json::from_str(addr_json_str)
        .expect("Failed to parse connection string");

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

    // Spawn task to handle keyboard input (use spawn_blocking for blocking I/O)
    let msg_tx_clone = msg_tx.clone();
    let input_task = tokio::task::spawn_blocking(move || {
        loop {
            // Poll for events with timeout
            if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
                match event::read() {
                    Ok(Event::Key(key_event)) => {
                        // Check for Ctrl+D to disconnect
                        if key_event.code == KeyCode::Char('d')
                            && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                            let _ = msg_tx_clone.send(ClientMessage::Disconnect);
                            break;
                        }

                        // Convert key event to bytes
                        let data = match key_event.code {
                            KeyCode::Char(c) => {
                                if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    // Handle Ctrl+key combinations
                                    if c.is_ascii_alphabetic() {
                                        let ctrl_char = (c.to_ascii_lowercase() as u8 - b'a' + 1) as char;
                                        vec![ctrl_char as u8]
                                    } else {
                                        vec![c as u8]
                                    }
                                } else {
                                    c.to_string().into_bytes()
                                }
                            }
                            KeyCode::Enter => vec![b'\r'],
                            KeyCode::Backspace => vec![0x7f], // DEL character
                            KeyCode::Tab => vec![b'\t'],
                            KeyCode::Esc => vec![0x1b],
                            KeyCode::Up => b"\x1b[A".to_vec(),
                            KeyCode::Down => b"\x1b[B".to_vec(),
                            KeyCode::Right => b"\x1b[C".to_vec(),
                            KeyCode::Left => b"\x1b[D".to_vec(),
                            KeyCode::Home => b"\x1b[H".to_vec(),
                            KeyCode::End => b"\x1b[F".to_vec(),
                            KeyCode::PageUp => b"\x1b[5~".to_vec(),
                            KeyCode::PageDown => b"\x1b[6~".to_vec(),
                            KeyCode::Delete => b"\x1b[3~".to_vec(),
                            KeyCode::Insert => b"\x1b[2~".to_vec(),
                            _ => continue,
                        };

                        // Send key event to server
                        let msg = ClientMessage::KeyEvent { data };
                        if msg_tx_clone.send(msg).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Resize(cols, rows)) => {
                        // Send resize event to server
                        let msg = ClientMessage::Resize { cols, rows };
                        let _ = msg_tx_clone.send(msg);
                    }
                    _ => {}
                }
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

    // Decode connection string
    let addr_json = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(connection_string.as_bytes())
        .expect("Failed to decode connection string");
    let addr_json_str = std::str::from_utf8(&addr_json)
        .expect("Invalid UTF-8 in connection string");
    let addr: NodeAddr = serde_json::from_str(addr_json_str)
        .expect("Failed to parse connection string");

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
