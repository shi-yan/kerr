//! Kerr server - accepts incoming connections, creates PTY, and spawns bash

use iroh::{
    Endpoint,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_snafu::{Result, ResultExt};
use std::sync::Arc;
use std::io::Write as IoWrite;
use std::path::Path;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use crate::{ClientMessage, ServerMessage, ALPN};
use crate::debug_log;
use arboard::Clipboard;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;

#[derive(Debug)]
struct PtyError(String);

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PtyError {}

/// Register the connection with the backend server
async fn register_with_backend(connection_string: &str, alias: Option<String>) -> Result<String> {
    // Get hostname
    let host_name = hostname::get()
        .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Failed to get hostname: {}", e)))?
        .to_string_lossy()
        .to_string();

    // Call the auth module to register
    crate::auth::register_connection(
        connection_string.to_string(),
        alias,
        host_name,
    )
    .await?;

    Ok(connection_string.to_string())
}

/// Unregister the connection from the backend server
async fn unregister_from_backend(alias: String) -> Result<()> {
    crate::auth::unregister_connection(alias).await
}

pub async fn run_server(register_alias: Option<String>, session_path: Option<String>) -> Result<()> {
    // Print session status
    crate::auth::print_session_status(session_path);
    println!();

    let endpoint = Endpoint::bind().await.map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("{}", e)))?;

    // Build our protocol handler and add our protocol, identified by its ALPN, and spawn the node.
    let router = Router::builder(endpoint).accept(ALPN.to_vec(), KerrServer).spawn();

    // Get the node address from the router's endpoint
    let _node_id = router.endpoint().id();
    let addr = router.endpoint().addr();

    // Encode the address as a compressed connection string (JSON -> gzip -> base64)
    let connection_string = crate::encode_connection_string(&addr);

    // Register with backend if alias was provided
    let registered_alias = if let Some(alias) = register_alias {
        match register_with_backend(&connection_string, Some(alias.clone())).await {
            Ok(_) => {
                println!("\n✓ Successfully registered with backend server");
                Some(alias)
            }
            Err(e) => {
                eprintln!("\n✗ Failed to register with backend: {}", e);
                eprintln!("  Continuing without registration...\n");
                None
            }
        }
    } else {
        None
    };

    // Build the connection commands
    let connect_command = format!("kerr connect {}", connection_string);
    let send_command = format!("kerr send {}", connection_string);
    let pull_command = format!("kerr pull {}", connection_string);
    let browse_command = format!("kerr browse {}", connection_string);
    let relay_command = format!("kerr relay {}", connection_string);
    let ping_command = format!("kerr ping {}", connection_string);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    Kerr Server Online                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    println!("Commands:");
    println!("  Connect: {}", connect_command);
    println!("  Send:    {} <local> <remote>", send_command);
    println!("  Pull:    {} <remote> <local>", pull_command);
    println!("  Browse:  {}", browse_command);
    println!("  Relay:   {} <local_port> <remote_port>", relay_command);
    println!("\n─────────────────────────────────────────────────────────────────");
    println!("Keys: [c]onnect | [s]end | [p]ull | [b]rowse | [r]elay | Ctrl+C");
    println!("  Ping:    {}", ping_command);
    println!("\n─────────────────────────────────────────────────────────────────");
    println!("Keys: [c]onnect | [s]end | [p]ull | [b]rowse | p[i]ng | Ctrl+C");
    println!("─────────────────────────────────────────────────────────────────\n");

    // Enable raw mode for keyboard event handling
    enable_raw_mode().unwrap_or_else(|err| eprintln!("Failed to enable raw mode: {err}"));

    // Spawn task to handle keyboard events
    let connect_clone = connect_command.clone();
    let send_clone = send_command.clone();
    let pull_clone = pull_command.clone();
    let browse_clone = browse_command.clone();
    let relay_clone = relay_command.clone();
    let ping_clone = ping_command.clone();

    let keyboard_task = tokio::task::spawn(async move {
        let mut event_stream = EventStream::new();

        loop {
            if let Some(event_result) = event_stream.next().await {
                match event_result {
                    Ok(Event::Key(key_event)) => {
                        match (key_event.code, key_event.modifiers, key_event.kind) {
                            // Handle 'c' key press to copy connect command
                            (KeyCode::Char('c'), KeyModifiers::NONE, KeyEventKind::Press) => {
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        if clipboard.set_text(&connect_clone).is_ok() {
                                            println!("\r\n✓ Connect command copied to clipboard!\r\n");
                                        } else {
                                            eprintln!("\r\n✗ Failed to copy to clipboard\r\n");
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("\r\n✗ Failed to access clipboard: {}\r\n", e);
                                    }
                                }
                            }
                            // Handle 's' key press to copy send command
                            (KeyCode::Char('s'), KeyModifiers::NONE, KeyEventKind::Press) => {
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        if clipboard.set_text(&send_clone).is_ok() {
                                            println!("\r\n✓ Send command copied to clipboard!\r\n");
                                        } else {
                                            eprintln!("\r\n✗ Failed to copy to clipboard\r\n");
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("\r\n✗ Failed to access clipboard: {}\r\n", e);
                                    }
                                }
                            }
                            // Handle 'p' key press to copy pull command
                            (KeyCode::Char('p'), KeyModifiers::NONE, KeyEventKind::Press) => {
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        if clipboard.set_text(&pull_clone).is_ok() {
                                            println!("\r\n✓ Pull command copied to clipboard!\r\n");
                                        } else {
                                            eprintln!("\r\n✗ Failed to copy to clipboard\r\n");
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("\r\n✗ Failed to access clipboard: {}\r\n", e);
                                    }
                                }
                            }
                            // Handle 'b' key press to copy browse command
                            (KeyCode::Char('b'), KeyModifiers::NONE, KeyEventKind::Press) => {
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        if clipboard.set_text(&browse_clone).is_ok() {
                                            println!("\r\n✓ Browse command copied to clipboard!\r\n");
                                        } else {
                                            eprintln!("\r\n✗ Failed to copy to clipboard\r\n");
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("\r\n✗ Failed to access clipboard: {}\r\n", e);
                                    }
                                }
                            }
                            // Handle 'r' key press to copy relay command
                            (KeyCode::Char('r'), KeyModifiers::NONE, KeyEventKind::Press) => {
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        if clipboard.set_text(&relay_clone).is_ok() {
                                            println!("\r\n✓ Relay command copied to clipboard!\r\n");
                                        }
                                        else {
                                            eprintln!("\r\n✗ Failed to copy to clipboard\r\n");
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("\r\n✗ Failed to access clipboard: {}\r\n", e);
                                    }
                                }
                            }
                            // Handle 'i' key press to copy ping command
                            (KeyCode::Char('i'), KeyModifiers::NONE, KeyEventKind::Press) => {
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        if clipboard.set_text(&ping_clone).is_ok() {
                                            println!("\r\n✓ Ping command copied to clipboard!\r\n");
                                        } else {
                                            eprintln!("\r\n✗ Failed to copy to clipboard\r\n");
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("\r\n✗ Failed to access clipboard: {}\r\n", e);
                                    }
                                }
                            }
                            // Handle Ctrl+C to exit
                            (KeyCode::Char('c'), KeyModifiers::CONTROL, KeyEventKind::Press) => {
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to process event: {}", e);
                    }
                    _ => {}
                }
            }
        }
    });

    // Wait for either Ctrl+C signal or keyboard task to complete
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\r\nShutting down...");
        }
        _ = keyboard_task => {
            println!("\r\nShutting down...");
        }
    }

    // Unregister from backend if we registered
    if let Some(alias) = registered_alias {
        match unregister_from_backend(alias).await {
            Ok(()) => {
                println!("✓ Successfully unregistered from backend server");
            }
            Err(e) => {
                eprintln!("✗ Failed to unregister from backend: {}", e);
            }
        }
    }

    // Disable raw mode before exiting
    disable_raw_mode().unwrap_or_else(|e| eprintln!("Failed to disable raw mode: {}", e));

    // Shutdown the router
    router.shutdown().await.e()?;

    Ok(())
}

#[derive(Debug, Clone)]
struct KerrServer;

impl ProtocolHandler for KerrServer {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let node_id = connection.remote_id();
        println!("\r\nAccepted connection from {node_id}\r");

        // Create session ID for logging
        let session_id = node_id.to_string();
        let session_id_short = &session_id[..8];

        // Accept a bi-directional stream
        eprintln!("\r\n[SERVER] Calling accept_bi...\r");
        let (send, mut recv) = connection.accept_bi().await?;
        eprintln!("\r\n[SERVER] accept_bi successful!\r");
        debug_log::log_bi_stream_accepted(session_id_short);

        // Read the Hello message to determine session type
        let config = bincode::config::standard();
        eprintln!("\r\n[SERVER] Reading Hello message length...\r");
        let mut len_bytes = [0u8; 4];
        if recv.read_exact(&mut len_bytes).await.is_err() {
            eprintln!("\r\nFailed to read Hello message length\r");
            debug_log::log_quic_read_failed(session_id_short, "failed to read Hello message length");
            return Ok(());
        }
        let len = u32::from_be_bytes(len_bytes) as usize;
        eprintln!("\r\n[SERVER] Hello message length: {} bytes\r", len);
        let mut msg_bytes = vec![0u8; len];
        if recv.read_exact(&mut msg_bytes).await.is_err() {
            eprintln!("\r\nFailed to read Hello message\r");
            debug_log::log_quic_read_failed(session_id_short, "failed to read Hello message body");
            return Ok(());
        }

        debug_log::log_quic_read_done(session_id_short, msg_bytes.len());

        let hello_msg: crate::ClientMessage = match bincode::decode_from_slice(&msg_bytes, config) {
            Ok((m, _)) => m,
            Err(e) => {
                eprintln!("\r\nFailed to deserialize Hello message: {}\r", e);
                debug_log::log_decode_failed(session_id_short, &e.to_string());
                return Ok(());
            }
        };

        let session_type = match hello_msg {
            crate::ClientMessage::Hello { session_type } => {
                debug_log::log_hello_received(session_id_short, &format!("{:?}", session_type));
                session_type
            },
            _ => {
                eprintln!("\r\nExpected Hello message, got something else\r");
                return Ok(());
            }
        };

        match session_type {
            crate::SessionType::Shell => {
                println!("\r\nStarting shell session for {node_id}\r");
                Self::handle_shell_session(node_id, send, recv).await
            }
            crate::SessionType::FileTransfer => {
                println!("\r\nStarting file transfer session for {node_id}\r");
                Self::handle_file_transfer_session(node_id, send, recv).await
            }
            crate::SessionType::FileBrowser => {
                println!("\r\nStarting file browser session for {node_id}\r");
                Self::handle_file_browser_session(node_id, send, recv).await
            }
            crate::SessionType::TcpRelay => {
                println!("\r\nStarting TCP relay session for {node_id}\r");
                Self::handle_tcp_relay_session(node_id, send, recv).await
            }
            crate::SessionType::Ping => {
                println!("\r\nStarting ping test session for {node_id}\r");
                Self::handle_ping_session(node_id, send, recv).await
            }
        }
    }
}

impl KerrServer {
    async fn handle_shell_session(
        node_id: iroh::PublicKey,
        mut send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
    ) -> Result<(), AcceptError> {
        // Create short session ID for logging (first 8 chars of node_id)
        let session_id = node_id.to_string();
        let session_id = &session_id[..8];

        debug_log::log_session_start(session_id);
        debug_log::log_connection_accepted(session_id, &node_id.to_string());

        // Create a PTY system
        let pty_system = native_pty_system();

        // Create a PTY with initial size
        debug_log::log_pty_creation_start(session_id, 80, 24);
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| {
                debug_log::log_pty_creation_failed(session_id, &e.to_string());
                AcceptError::from_err(PtyError(format!("Failed to open PTY: {}", e)))
            })?;

        // Get PTY file descriptor for logging (if available)
        if let Some(pty_fd) = pair.master.as_raw_fd() {
            debug_log::log_pty_created(session_id, pty_fd);
        } else {
            debug_log::log_debug(session_id, "PTY_CREATED: success (fd unknown)");
        }

        // Spawn bash in the PTY with custom prompt
        // Use 'bash -c' to set PS1 and then exec bash to replace the process
        let username = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
        let prompt_cmd = format!(
            "export PS1='{}@kerr \\w> ' && exec bash --norc --noprofile",
            username
        );

        let mut cmd = CommandBuilder::new("bash");
        cmd.arg("-c");
        cmd.arg(&prompt_cmd);
        cmd.env("TERM", "xterm-256color");

        debug_log::log_bash_spawn_start(session_id);
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| {
                debug_log::log_bash_spawn_failed(session_id, &e.to_string());
                AcceptError::from_err(PtyError(format!("Failed to spawn bash: {}", e)))
            })?;

        // Log bash PID if available
        if let Some(pid) = child.process_id() {
            debug_log::log_bash_spawned(session_id, pid);
        }

        println!("Spawned bash in PTY for {node_id}\r");

        // Get the master PTY for reading/writing
        let mut reader = pair.master.try_clone_reader()
            .map_err(|e| AcceptError::from_err(PtyError(format!("Failed to clone reader: {}", e))))?;
        let mut writer = pair.master.take_writer()
            .map_err(|e| AcceptError::from_err(PtyError(format!("Failed to take writer: {}", e))))?;

        // Keep master for resizing
        let master = Arc::new(std::sync::Mutex::new(pair.master));
        let master_clone = master.clone();

        // Channel to coordinate sending data back to client
        let (send_tx, mut send_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        // Channel to signal when PTY has ended (bash exited)
        let (pty_ended_tx, mut pty_ended_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Spawn task to write messages to send stream
        let session_id_send = session_id.to_string();
        let send_task = tokio::spawn(async move {
            debug_log::log_send_task_started(&session_id_send);
            let mut msg_count = 0;

            while let Some(data) = send_rx.recv().await {
                msg_count += 1;
                debug_log::log_quic_write_start(&session_id_send, data.len());

                match send.write_all(&data).await {
                    Ok(()) => {
                        debug_log::log_quic_write_done(&session_id_send, data.len());
                    }
                    Err(e) => {
                        debug_log::log_quic_write_failed(&session_id_send, data.len(), &e.to_string());
                        break;
                    }
                }
            }

            debug_log::log_send_task_ended(&session_id_send, &format!("channel_closed, sent {} messages", msg_count));
        });

        // Spawn task to read from PTY and send to client
        let send_tx_clone = send_tx.clone();
        let session_id_pty = session_id.to_string();
        let pty_to_client = tokio::spawn(async move {
            debug_log::log_pty_task_started(&session_id_pty);
            let mut buffer = [0u8; 8192];
            let config = bincode::config::standard();
            let mut pty_ended = false;
            let mut total_bytes_read = 0;
            let mut read_count = 0;

            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        // EOF - bash has exited
                        debug_log::log_pty_eof(&session_id_pty);
                        println!("\r\nBash exited, notifying client\r");
                        pty_ended = true;

                        // Send error message to client
                        let msg = ServerMessage::Error {
                            message: "Session ended: bash exited".to_string(),
                        };
                        if let Ok(encoded) = bincode::encode_to_vec(&msg, config) {
                            let len = (encoded.len() as u32).to_be_bytes();
                            let mut full_msg = Vec::new();
                            full_msg.extend_from_slice(&len);
                            full_msg.extend_from_slice(&encoded);

                            debug_log::log_msg_queued(&session_id_pty, "Error", full_msg.len());
                            if send_tx_clone.send(full_msg).is_err() {
                                debug_log::log_queue_send_failed(&session_id_pty, "Error");
                            }
                        }
                        break;
                    }
                    Ok(n) => {
                        read_count += 1;
                        total_bytes_read += n;
                        debug_log::log_pty_read(&session_id_pty, n);

                        let msg = ServerMessage::Output {
                            data: buffer[..n].to_vec(),
                        };
                        match bincode::encode_to_vec(&msg, config) {
                            Ok(encoded) => {
                                let len = (encoded.len() as u32).to_be_bytes();
                                let mut full_msg = Vec::new();
                                full_msg.extend_from_slice(&len);
                                full_msg.extend_from_slice(&encoded);

                                debug_log::log_msg_queued(&session_id_pty, "Output", full_msg.len());
                                if send_tx_clone.send(full_msg).is_err() {
                                    debug_log::log_queue_send_failed(&session_id_pty, "Output");
                                    break;
                                }
                            }
                            Err(e) => {
                                debug_log::log_pty_error(&session_id_pty, &format!("encode failed: {}", e));
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        debug_log::log_pty_error(&session_id_pty, &e.to_string());
                        break;
                    }
                }
            }

            // Signal that PTY has ended
            if pty_ended {
                let _ = pty_ended_tx.send(()).await;
            }

            debug_log::log_pty_task_ended(&session_id_pty, &format!("total_reads={}, total_bytes={}", read_count, total_bytes_read));
        });

        // Main loop: receive from client and write to PTY, or exit if PTY ends
        let config = bincode::config::standard();

        loop {
            tokio::select! {
                // Check if PTY has ended (bash exited)
                _ = pty_ended_rx.recv() => {
                    println!("\r\nPTY ended, closing connection with {node_id}\r");
                    break;
                }

                // Read message from client
                result = async {
                    debug_log::log_quic_read_start(session_id);
                    let mut len_bytes = [0u8; 4];
                    if recv.read_exact(&mut len_bytes).await.is_err() {
                        debug_log::log_quic_read_failed(session_id, "failed to read message length");
                        return None;
                    }
                    let len = u32::from_be_bytes(len_bytes) as usize;

                    let mut msg_bytes = vec![0u8; len];
                    if recv.read_exact(&mut msg_bytes).await.is_err() {
                        debug_log::log_quic_read_failed(session_id, "failed to read message body");
                        return None;
                    }

                    debug_log::log_quic_read_done(session_id, msg_bytes.len());
                    Some(msg_bytes)
                } => {
                    match result {
                        Some(msg_bytes) => {
                            debug_log::log_decode_start(session_id, msg_bytes.len());
                            // Deserialize message
                            let msg: ClientMessage = match bincode::decode_from_slice(&msg_bytes, config) {
                                Ok((m, _)) => {
                                    debug_log::log_decode_done(session_id, "ClientMessage");
                                    m
                                },
                                Err(e) => {
                                    debug_log::log_decode_failed(session_id, &e.to_string());
                                    eprintln!("\r\nFailed to deserialize message: {}\r", e);
                                    continue;
                                }
                            };

                            match msg {
                                ClientMessage::KeyEvent { data } => {
                                    debug_log::log_client_input(session_id, "KeyEvent", data.len());
                                    // Write key event to PTY
                                    if let Err(e) = writer.write_all(&data) {
                                        eprintln!("\r\nFailed to write to PTY: {}\r", e);
                                        break;
                                    }
                                    if let Err(e) = writer.flush() {
                                        eprintln!("\r\nFailed to flush PTY: {}\r", e);
                                        break;
                                    }
                                }
                                ClientMessage::Resize { cols, rows } => {
                                    debug_log::log_client_input(session_id, "Resize", 0);
                                    // Resize the PTY
                                    let new_size = PtySize {
                                        rows,
                                        cols,
                                        pixel_width: 0,
                                        pixel_height: 0,
                                    };
                                    if let Ok(master_guard) = master_clone.lock() {
                                        if let Err(e) = master_guard.resize(new_size) {
                                            eprintln!("\r\nFailed to resize PTY: {}\r", e);
                                        } else {
                                            println!("\r\nResized PTY to {}x{}\r", cols, rows);
                                        }
                                    }
                                }
                                ClientMessage::Disconnect => {
                                    debug_log::log_client_input(session_id, "Disconnect", 0);
                                    println!("\r\nClient requested disconnect\r");
                                    break;
                                }
                                _ => {
                                    eprintln!("\r\nUnexpected message type in shell session\r");
                                }
                            }
                        }
                        None => {
                            // Connection closed
                            break;
                        }
                    }
                }
            }
        }

        // Clean up - ensure all queued messages are sent before closing
        pty_to_client.abort(); // PTY task should already be done, but ensure it's aborted
        drop(send_tx); // Close the send channel so send_task knows to finish
        let _ = send_task.await; // Wait for send_task to finish sending all queued messages

        debug_log::log_session_end(session_id);
        println!("\r\nConnection closed for {node_id}\r");

        Ok(())
    }

    async fn handle_file_transfer_session(
        node_id: iroh::PublicKey,
        mut send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
    ) -> Result<(), AcceptError> {
        let config = bincode::config::standard();

        // Channel to coordinate sending data back to client
        let (send_tx, mut send_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        // Spawn task to write messages to send stream
        let send_task = tokio::spawn(async move {
            while let Some(data) = send_rx.recv().await {
                if send.write_all(&data).await.is_err() {
                    break;
                }
            }
        });

        // File upload state
        let mut upload_file: Option<std::fs::File> = None;
        let mut upload_path: Option<String> = None;

        // Main loop: receive from client and handle file transfer messages
        loop {
            // Read message length
            let mut len_bytes = [0u8; 4];
            if recv.read_exact(&mut len_bytes).await.is_err() {
                break;
            }
            let len = u32::from_be_bytes(len_bytes) as usize;

            // Read message data
            let mut msg_bytes = vec![0u8; len];
            if recv.read_exact(&mut msg_bytes).await.is_err() {
                break;
            }

            // Deserialize message
            let msg: crate::ClientMessage = match bincode::decode_from_slice(&msg_bytes, config) {
                Ok((m, _)) => m,
                Err(e) => {
                    eprintln!("\r\nFailed to deserialize message: {}\r", e);
                    continue;
                }
            };

            match msg {
                crate::ClientMessage::StartUpload { path, size, is_dir, force } => {
                    println!("\r\nReceiving file upload: {} ({} bytes, is_dir: {}, force: {})\r", path, size, is_dir, force);

                    // Check if path is an existing directory - this is an error
                    let file_path = Path::new(&path);

                    // If not force mode and file exists, ask for confirmation
                    if !force && file_path.exists() && !file_path.is_dir() {
                        let prompt_msg = crate::ServerMessage::ConfirmPrompt {
                            message: format!("File '{}' already exists. Overwrite?", path),
                        };
                        if let Ok(encoded) = bincode::encode_to_vec(&prompt_msg, config) {
                            let len = (encoded.len() as u32).to_be_bytes();
                            let mut full_msg = Vec::new();
                            full_msg.extend_from_slice(&len);
                            full_msg.extend_from_slice(&encoded);
                            let _ = send_tx.send(full_msg);
                        }

                        // Wait for confirmation response
                        let mut len_bytes = [0u8; 4];
                        if recv.read_exact(&mut len_bytes).await.is_err() {
                            break;
                        }
                        let len = u32::from_be_bytes(len_bytes) as usize;
                        let mut msg_bytes = vec![0u8; len];
                        if recv.read_exact(&mut msg_bytes).await.is_err() {
                            break;
                        }

                        let confirm_msg: crate::ClientMessage = match bincode::decode_from_slice(&msg_bytes, config) {
                            Ok((m, _)) => m,
                            Err(e) => {
                                eprintln!("\r\nFailed to deserialize confirmation: {}\r", e);
                                continue;
                            }
                        };

                        match confirm_msg {
                            crate::ClientMessage::ConfirmResponse { confirmed } => {
                                if !confirmed {
                                    println!("\r\nUpload cancelled by user\r");
                                    continue;
                                }
                            }
                            _ => {
                                eprintln!("\r\nExpected ConfirmResponse\r");
                                continue;
                            }
                        }
                    }

                    let actual_path = if file_path.is_dir() {
                        let err_msg = crate::ServerMessage::Error {
                            message: format!("Target path is an existing directory: {}. Please specify a filename or use a path with trailing /", path),
                        };
                        eprintln!("\r\nError: Target path is an existing directory: {}\r", path);
                        if let Ok(encoded) = bincode::encode_to_vec(&err_msg, config) {
                            let len = (encoded.len() as u32).to_be_bytes();
                            let mut full_msg = Vec::new();
                            full_msg.extend_from_slice(&len);
                            full_msg.extend_from_slice(&encoded);
                            let _ = send_tx.send(full_msg);
                        }
                        continue;
                    } else {
                        path.clone()
                    };

                    // Create parent directories if needed
                    if let Some(parent) = file_path.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            eprintln!("\r\nFailed to create directories: {}\r", e);
                            let err_msg = crate::ServerMessage::Error {
                                message: format!("Failed to create directories: {}", e),
                            };
                            if let Ok(encoded) = bincode::encode_to_vec(&err_msg, config) {
                                let len = (encoded.len() as u32).to_be_bytes();
                                let mut full_msg = Vec::new();
                                full_msg.extend_from_slice(&len);
                                full_msg.extend_from_slice(&encoded);
                                let _ = send_tx.send(full_msg);
                            }
                            continue;
                        }
                    }

                    // Open file for writing
                    match std::fs::File::create(&actual_path) {
                        Ok(file) => {
                            upload_file = Some(file);
                            upload_path = Some(actual_path.clone());

                            // Send acknowledgment
                            let ack_msg = crate::ServerMessage::UploadAck;
                            if let Ok(encoded) = bincode::encode_to_vec(&ack_msg, config) {
                                let len = (encoded.len() as u32).to_be_bytes();
                                let mut full_msg = Vec::new();
                                full_msg.extend_from_slice(&len);
                                full_msg.extend_from_slice(&encoded);
                                let _ = send_tx.send(full_msg);
                            }
                        }
                        Err(e) => {
                            eprintln!("\r\nFailed to create file: {}\r", e);
                            let err_msg = crate::ServerMessage::Error {
                                message: format!("Failed to create file: {}", e),
                            };
                            if let Ok(encoded) = bincode::encode_to_vec(&err_msg, config) {
                                let len = (encoded.len() as u32).to_be_bytes();
                                let mut full_msg = Vec::new();
                                full_msg.extend_from_slice(&len);
                                full_msg.extend_from_slice(&encoded);
                                let _ = send_tx.send(full_msg);
                            }
                        }
                    }
                }
                crate::ClientMessage::FileChunk { data } => {
                    // Write chunk to file
                    if let Some(ref mut file) = upload_file {
                        if let Err(e) = file.write_all(&data) {
                            eprintln!("\r\nFailed to write to file: {}\r", e);
                            let err_msg = crate::ServerMessage::Error {
                                message: format!("Failed to write to file: {}", e),
                            };
                            if let Ok(encoded) = bincode::encode_to_vec(&err_msg, config) {
                                let len = (encoded.len() as u32).to_be_bytes();
                                let mut full_msg = Vec::new();
                                full_msg.extend_from_slice(&len);
                                full_msg.extend_from_slice(&encoded);
                                let _ = send_tx.send(full_msg);
                            }
                            // Clear upload state
                            upload_file = None;
                            upload_path = None;
                        }
                    } else {
                        eprintln!("\r\nReceived file chunk without StartUpload\r");
                    }
                }
                crate::ClientMessage::EndUpload => {
                    if let Some(path) = &upload_path {
                        println!("\r\nFile upload completed: {}\r", path);
                    }
                    // Close the file and clear state
                    upload_file = None;
                    upload_path = None;
                }
                crate::ClientMessage::RequestDownload { path } => {
                    println!("\r\nClient requested download: {}\r", path);

                    // Check if path exists
                    let file_path = Path::new(&path);
                    if !file_path.exists() {
                        let err_msg = crate::ServerMessage::Error {
                            message: format!("Path does not exist: {}", path),
                        };
                        eprintln!("\r\nError: Path does not exist: {}\r", path);
                        if let Ok(encoded) = bincode::encode_to_vec(&err_msg, config) {
                            let len = (encoded.len() as u32).to_be_bytes();
                            let mut full_msg = Vec::new();
                            full_msg.extend_from_slice(&len);
                            full_msg.extend_from_slice(&encoded);
                            let _ = send_tx.send(full_msg);
                        }
                        continue;
                    }

                    let is_dir = file_path.is_dir();

                    // Calculate total size
                    let total_size = match crate::transfer::calculate_size(file_path) {
                        Ok(size) => size,
                        Err(e) => {
                            let err_msg = crate::ServerMessage::Error {
                                message: format!("Failed to calculate size: {}", e),
                            };
                            eprintln!("\r\nError calculating size: {}\r", e);
                            if let Ok(encoded) = bincode::encode_to_vec(&err_msg, config) {
                                let len = (encoded.len() as u32).to_be_bytes();
                                let mut full_msg = Vec::new();
                                full_msg.extend_from_slice(&len);
                                full_msg.extend_from_slice(&encoded);
                                let _ = send_tx.send(full_msg);
                            }
                            continue;
                        }
                    };

                    println!("\r\nSending file: {} ({} bytes, is_dir: {})\r", path, total_size, is_dir);

                    // Send StartDownload message
                    let start_msg = crate::ServerMessage::StartDownload {
                        size: total_size,
                        is_dir,
                    };
                    if let Ok(encoded) = bincode::encode_to_vec(&start_msg, config) {
                        let len = (encoded.len() as u32).to_be_bytes();
                        let mut full_msg = Vec::new();
                        full_msg.extend_from_slice(&len);
                        full_msg.extend_from_slice(&encoded);
                        let _ = send_tx.send(full_msg);
                    }

                    // Get all files to send
                    let files = match crate::transfer::get_files_recursive(file_path) {
                        Ok(files) => files,
                        Err(e) => {
                            let err_msg = crate::ServerMessage::Error {
                                message: format!("Failed to read files: {}", e),
                            };
                            eprintln!("\r\nError reading files: {}\r", e);
                            if let Ok(encoded) = bincode::encode_to_vec(&err_msg, config) {
                                let len = (encoded.len() as u32).to_be_bytes();
                                let mut full_msg = Vec::new();
                                full_msg.extend_from_slice(&len);
                                full_msg.extend_from_slice(&encoded);
                                let _ = send_tx.send(full_msg);
                            }
                            continue;
                        }
                    };

                    // Send file chunks
                    use std::io::Read;
                    let mut bytes_sent = 0u64;

                    for file in files {
                        let mut f = match std::fs::File::open(&file) {
                            Ok(f) => f,
                            Err(e) => {
                                eprintln!("\r\nFailed to open file {:?}: {}\r", file, e);
                                continue;
                            }
                        };

                        let mut buffer = vec![0u8; crate::transfer::CHUNK_SIZE];

                        loop {
                            let n = match f.read(&mut buffer) {
                                Ok(n) => n,
                                Err(e) => {
                                    eprintln!("\r\nFailed to read file {:?}: {}\r", file, e);
                                    break;
                                }
                            };

                            if n == 0 {
                                break;
                            }

                            let chunk_msg = crate::ServerMessage::FileChunk {
                                data: buffer[..n].to_vec(),
                            };

                            if let Ok(encoded) = bincode::encode_to_vec(&chunk_msg, config) {
                                let len = (encoded.len() as u32).to_be_bytes();
                                let mut full_msg = Vec::new();
                                full_msg.extend_from_slice(&len);
                                full_msg.extend_from_slice(&encoded);
                                let _ = send_tx.send(full_msg);
                            }

                            bytes_sent += n as u64;
                        }
                    }

                    // Send EndDownload message
                    let end_msg = crate::ServerMessage::EndDownload;
                    if let Ok(encoded) = bincode::encode_to_vec(&end_msg, config) {
                        let len = (encoded.len() as u32).to_be_bytes();
                        let mut full_msg = Vec::new();
                        full_msg.extend_from_slice(&len);
                        full_msg.extend_from_slice(&encoded);
                        let _ = send_tx.send(full_msg);
                    }

                    println!("\r\nDownload completed: {} ({} bytes sent)\r", path, bytes_sent);
                }
                crate::ClientMessage::Disconnect => {
                    println!("\r\nClient requested disconnect\r");
                    break;
                }
                _ => {
                    eprintln!("\r\nUnexpected message type in file transfer session\r");
                }
            }
        }

        // Clean up
        send_task.abort();
        println!("\r\nFile transfer session closed for {node_id}\r");

        Ok(())
    }

    async fn handle_file_browser_session(
        node_id: iroh::PublicKey,
        mut send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
    ) -> Result<(), AcceptError> {
        use std::path::Path;

        let config = bincode::config::standard();

        println!("\r\nFile browser session started for {node_id}\r");

        loop {
            // Read message length
            let mut len_bytes = [0u8; 4];
            if let Err(e) = recv.read_exact(&mut len_bytes).await {
                eprintln!("\r\nFailed to read message length: {}\r", e);
                break;
            }
            let msg_len = u32::from_be_bytes(len_bytes) as usize;

            // Read message
            let mut msg_bytes = vec![0u8; msg_len];
            if let Err(e) = recv.read_exact(&mut msg_bytes).await {
                eprintln!("\r\nFailed to read message data: {}\r", e);
                break;
            }

            // Deserialize message
            let (msg, _): (crate::ClientMessage, _) = match bincode::decode_from_slice(&msg_bytes, config) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("\r\nFailed to deserialize message: {}\r", e);
                    continue;
                }
            };

            // Handle filesystem requests
            let response = match msg {
                crate::ClientMessage::FsReadDir { path } => {
                    println!("\r\nFsReadDir request: {}\r", path);

                    match std::fs::read_dir(Path::new(&path)) {
                        Ok(entries) => {
                            let mut file_entries = Vec::new();

                            for entry in entries {
                                if let Ok(entry) = entry {
                                    let path = entry.path();
                                    // Use symlink_metadata to NOT follow symlinks
                                    // This prevents issues when symlinks point to inaccessible locations
                                    let metadata_result = std::fs::symlink_metadata(&path);

                                    if let Ok(metadata) = metadata_result {
                                        let file_name = path.file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("")
                                            .to_string();

                                        // For symlinks, try to determine if they point to a directory
                                        let is_dir = if metadata.is_symlink() {
                                            // Try to follow the symlink to see if it points to a directory
                                            std::fs::metadata(&path)
                                                .map(|m| m.is_dir())
                                                .unwrap_or(false)
                                        } else {
                                            metadata.is_dir()
                                        };

                                        #[cfg(unix)]
                                        let is_hidden = file_name.starts_with('.');

                                        #[cfg(windows)]
                                        let is_hidden = {
                                            use std::os::windows::fs::MetadataExt;
                                            const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
                                            (metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN) != 0
                                        };

                                        #[cfg(not(any(unix, windows)))]
                                        let is_hidden = false;

                                        let name = if is_dir {
                                            format!("{}/", file_name)
                                        } else {
                                            file_name
                                        };

                                        use crate::custom_explorer::file_explorer::FileMetadata;
                                        use crate::custom_explorer::filesystem::FileEntry;

                                        file_entries.push(FileEntry {
                                            name,
                                            path: path.clone(),
                                            is_dir,
                                            is_hidden,
                                            metadata: Some(FileMetadata {
                                                size: metadata.len(),
                                                created: metadata.created().ok(),
                                                modified: metadata.modified().ok(),
                                                is_dir,
                                            }),
                                        });
                                    } else {
                                        // Log but don't fail - skip entries we can't read metadata for
                                        eprintln!("\r\nWarning: Could not read metadata for {:?}: {}\r", path, metadata_result.unwrap_err());
                                    }
                                }
                            }

                            let entries_json = serde_json::to_string(&file_entries).unwrap();
                            crate::ServerMessage::FsDirListing { entries_json }
                        }
                        Err(e) => {
                            eprintln!("\r\nError reading directory {}: {}\r", path, e);
                            crate::ServerMessage::FsError {
                                message: format!("Failed to read directory: {}", e),
                            }
                        }
                    }
                }

                crate::ClientMessage::FsMetadata { path } => {
                    println!("\r\nFsMetadata request: {}\r", path);

                    match std::fs::metadata(Path::new(&path)) {
                        Ok(metadata) => {
                            use crate::custom_explorer::file_explorer::FileMetadata;

                            let file_metadata = FileMetadata {
                                size: metadata.len(),
                                created: metadata.created().ok(),
                                modified: metadata.modified().ok(),
                                is_dir: metadata.is_dir(),
                            };

                            let metadata_json = serde_json::to_string(&file_metadata).unwrap();
                            crate::ServerMessage::FsMetadataResponse { metadata_json }
                        }
                        Err(e) => {
                            crate::ServerMessage::FsError {
                                message: format!("Failed to get metadata: {}", e),
                            }
                        }
                    }
                }

                crate::ClientMessage::FsReadFile { path } => {
                    println!("\r\nFsReadFile request: {}\r", path);

                    match std::fs::read(Path::new(&path)) {
                        Ok(data) => {
                            crate::ServerMessage::FsFileContent { data }
                        }
                        Err(e) => {
                            crate::ServerMessage::FsError {
                                message: format!("Failed to read file: {}", e),
                            }
                        }
                    }
                }

                crate::ClientMessage::FsHashFile { path } => {
                    println!("\r\nFsHashFile request: {}\r", path);

                    match std::fs::read(Path::new(&path)) {
                        Ok(data) => {
                            // Calculate blake3 hash
                            let hash = blake3::hash(&data);
                            let hash_hex = hash.to_hex().to_string();
                            crate::ServerMessage::FsHashResponse { hash: hash_hex }
                        }
                        Err(e) => {
                            crate::ServerMessage::FsError {
                                message: format!("Failed to hash file: {}", e),
                            }
                        }
                    }
                }

                crate::ClientMessage::FsDelete { path } => {
                    println!("\r\nFsDelete request: {}\r", path);

                    let path_obj = Path::new(&path);
                    let result = if path_obj.is_dir() {
                        std::fs::remove_dir_all(path_obj)
                    } else {
                        std::fs::remove_file(path_obj)
                    };

                    match result {
                        Ok(()) => {
                            println!("\r\nSuccessfully deleted: {}\r", path);
                            crate::ServerMessage::FsDeleteResponse { success: true }
                        }
                        Err(e) => {
                            eprintln!("\r\nFailed to delete {}: {}\r", path, e);
                            crate::ServerMessage::FsError {
                                message: format!("Failed to delete: {}", e),
                            }
                        }
                    }
                }

                crate::ClientMessage::Disconnect => {
                    println!("\r\nClient disconnecting\r");
                    break;
                }

                _ => {
                    crate::ServerMessage::Error {
                        message: "Unexpected message type".to_string(),
                    }
                }
            };

            // Send response
            match bincode::encode_to_vec(&response, config) {
                Ok(encoded) => {
                    let len = (encoded.len() as u32).to_be_bytes();
                    if let Err(e) = send.write_all(&len).await {
                        eprintln!("\r\nFailed to write response length: {}\r", e);
                        break;
                    }
                    if let Err(e) = send.write_all(&encoded).await {
                        eprintln!("\r\nFailed to write response data: {}\r", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("\r\nFailed to encode response: {}\r", e);
                    break;
                }
            }
        }

        println!("\r\nFile browser session closed for {node_id}\r");
        Ok(())
    }

    async fn handle_tcp_relay_session(
        node_id: iroh::PublicKey,
        send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
    ) -> Result<(), AcceptError> {
        use tokio::net::TcpStream;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use std::collections::HashMap;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        // Shared state for tracking remote TCP connections
        let tcp_connections: Arc<Mutex<HashMap<u32, tokio::sync::mpsc::Sender<Vec<u8>>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Wrap send stream in Arc<Mutex> for sharing between tasks
        let send = Arc::new(Mutex::new(send));

        let config = bincode::config::standard();

        // Main loop to handle client messages

        loop {
            // Read message length
            let mut len_bytes = [0u8; 4];
            if recv.read_exact(&mut len_bytes).await.is_err() {
                break;
            }
            let msg_len = u32::from_be_bytes(len_bytes) as usize;

            // Read message
            let mut msg_bytes = vec![0u8; msg_len];

            if recv.read_exact(&mut msg_bytes).await.is_err() {
                break;
            }

            // Decode message
            let (msg, _): (crate::ClientMessage, _) = match bincode::decode_from_slice(&msg_bytes, config) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("\r\nFailed to decode message: {}\r", e);
                    break;
                }
            };

            match msg {
                crate::ClientMessage::TcpOpen { stream_id, destination_port } => {
                    println!("\r\nOpening TCP connection {stream_id} to localhost:{destination_port}\r");

                    // Try to connect to remote port
                    match TcpStream::connect(format!("127.0.0.1:{}", destination_port)).await {
                        Ok(tcp_stream) => {
                            // Send success response
                            let response = crate::ServerMessage::TcpOpenResponse {
                                stream_id,
                                success: true,
                                error: None,
                            };
                            let encoded = match bincode::encode_to_vec(&response, config) {
                                Ok(e) => e,
                                Err(e) => {
                                    eprintln!("\r\nFailed to encode TcpOpenResponse: {}\r", e);
                                    continue;
                                }
                            };
                            let len = (encoded.len() as u32).to_be_bytes();

                            {
                                let mut send_locked = send.lock().await;
                                if send_locked.write_all(&len).await.is_err() {
                                    break;
                                }
                                if send_locked.write_all(&encoded).await.is_err() {
                                    break;
                                }
                            }

                            // Create channel for sending data to this TCP connection
                            let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(100);
                            tcp_connections.lock().await.insert(stream_id, tx);

                            let send_for_task = Arc::clone(&send);
                            let tcp_connections_for_task = Arc::clone(&tcp_connections);

                            // Spawn task to handle this TCP connection
                            tokio::spawn(async move {
                                let (mut tcp_read, mut tcp_write) = tcp_stream.into_split();

                                // Task to read from remote TCP and send to client
                                let read_task = {
                                    let send_for_read = Arc::clone(&send_for_task);
                                    tokio::spawn(async move {
                                        let mut buf = vec![0u8; 65536];
                                        loop {
                                            match tcp_read.read(&mut buf).await {
                                                Ok(0) => break, // EOF
                                                Ok(n) => {
                                                    // Send data to client
                                                    let response = crate::ServerMessage::TcpDataResponse {
                                                        stream_id,
                                                        data: buf[..n].to_vec(),
                                                    };
                                                    let config = bincode::config::standard();
                                                    let encoded = match bincode::encode_to_vec(&response, config) {
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

                                // Task to receive from client and write to remote TCP
                                let write_task = tokio::spawn(async move {
                                    while let Some(data) = rx.recv().await {
                                        if tcp_write.write_all(&data).await.is_err() {
                                            break;
                                        }
                                    }
                                });

                                // Wait for either task to complete
                                tokio::select! {
                                    _ = read_task => {}
                                    _ = write_task => {}
                                }

                                // Send close response
                                let close_response = crate::ServerMessage::TcpCloseResponse {
                                    stream_id,
                                    error: None,
                                };
                                let config = bincode::config::standard();
                                if let Ok(encoded) = bincode::encode_to_vec(&close_response, config) {
                                    let len = (encoded.len() as u32).to_be_bytes();
                                    let mut send_locked = send_for_task.lock().await;
                                    let _ = send_locked.write_all(&len).await;
                                    let _ = send_locked.write_all(&encoded).await;
                                }

                                // Remove from connections map
                                tcp_connections_for_task.lock().await.remove(&stream_id);
                                println!("\r\nTCP connection {} closed\r", stream_id);
                            });
                        }
                        Err(e) => {
                            // Send error response
                            eprintln!("\r\nFailed to connect to localhost:{}: {}\r", destination_port, e);
                            let response = crate::ServerMessage::TcpOpenResponse {
                                stream_id,
                                success: false,
                                error: Some(format!("Failed to connect: {}", e)),
                            };
                            let encoded = match bincode::encode_to_vec(&response, config) {
                                Ok(e) => e,
                                Err(e) => {
                                    eprintln!("\r\nFailed to encode error response: {}\r", e);
                                    continue;
                                }
                            };
                            let len = (encoded.len() as u32).to_be_bytes();

                            let mut send_locked = send.lock().await;
                            if send_locked.write_all(&len).await.is_err() {
                                break;
                            }
                            if send_locked.write_all(&encoded).await.is_err() {
                                break;
                            }
                        }
                    }
                }
                crate::ClientMessage::TcpData { stream_id, data } => {
                    // Forward data to the appropriate TCP connection
                    let connections = tcp_connections.lock().await;
                    if let Some(tx) = connections.get(&stream_id) {
                        if tx.send(data).await.is_err() {
                            eprintln!("\r\nFailed to forward data to TCP connection {}\r", stream_id);
                        }
                    }
                }
                crate::ClientMessage::TcpClose { stream_id } => {
                    println!("\r\nClosing TCP connection {stream_id}\r");
                    // Remove connection from map (this will close the channel and terminate the task)
                    tcp_connections.lock().await.remove(&stream_id);
                }
                _ => {
                    eprintln!("\r\nUnexpected message in TCP relay session\r");
                }
            }
        }

        println!("\r\nTCP relay session closed for {node_id}\r");
        Ok(())
    }

    async fn handle_ping_session(
        node_id: iroh::PublicKey,
        mut send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
    ) -> Result<(), AcceptError> {
        let config = bincode::config::standard();

        println!("\r\nPing session started for {node_id}\r");

        loop {
            // Read message length
            let mut len_bytes = [0u8; 4];
            if recv.read_exact(&mut len_bytes).await.is_err() {
                break;
            }
            let len = u32::from_be_bytes(len_bytes) as usize;

            // Read message data
            let mut msg_bytes = vec![0u8; len];
            if recv.read_exact(&mut msg_bytes).await.is_err() {
                break;
            }

            // Deserialize message
            let msg: crate::ClientMessage = match bincode::decode_from_slice(&msg_bytes, config) {
                Ok((m, _)) => m,
                Err(e) => {
                    eprintln!("\r\nFailed to deserialize message: {}\r", e);
                    continue;
                }
            };

            match msg {
                crate::ClientMessage::PingRequest { data } => {
                    // Echo the data back
                    let response = crate::ServerMessage::PingResponse { data };

                    if let Ok(encoded) = bincode::encode_to_vec(&response, config) {
                        let len = (encoded.len() as u32).to_be_bytes();
                        if send.write_all(&len).await.is_err() {
                            break;
                        }
                        if send.write_all(&encoded).await.is_err() {
                            break;
                        }
                    }
                }
                crate::ClientMessage::Disconnect => {
                    println!("\r\nClient requested disconnect\r");
                    break;
                }
                _ => {
                    eprintln!("\r\nUnexpected message type in ping session\r");
                }
            }
        }

        println!("\r\nPing session closed for {node_id}\r");
        Ok(())
    }
}