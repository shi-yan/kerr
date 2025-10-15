//! Kerr server - accepts incoming connections, creates PTY, and spawns bash

use iroh::{
    Endpoint,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler, Router},
};
use n0_snafu::{Result, ResultExt};
use std::sync::Arc;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use crate::{ClientMessage, ServerMessage, ALPN};
use arboard::Clipboard;
use base64::Engine;
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

pub async fn run_server() -> Result<()> {
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    // Build our protocol handler and add our protocol, identified by its ALPN, and spawn the node.
    let router = Router::builder(endpoint).accept(ALPN, KerrServer).spawn();

    // Wait for the node to be online
    router.endpoint().online().await;

    let addr = router.endpoint().node_addr();

    // Encode the address as a connection string (JSON -> base64)
    let addr_json = serde_json::to_string(&addr).unwrap();
    let connection_string = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(addr_json.as_bytes());

    // Build the full connection command
    let connection_command = format!("kerr connect {}", connection_string);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    Kerr Server Online                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    println!("Connection command:");
    println!("\n  {}\n", connection_command);
    println!("─────────────────────────────────────────────────────────────────");
    println!("Press 'c' to copy to clipboard | Ctrl+C to stop server");
    println!("─────────────────────────────────────────────────────────────────\n");

    // Enable raw mode for keyboard event handling
    enable_raw_mode().unwrap_or_else(|err| eprintln!("Failed to enable raw mode: {err}"));

    // Spawn task to handle keyboard events
    let connection_command_clone = connection_command.clone();
    let keyboard_task = tokio::task::spawn(async move {
        let mut event_stream = EventStream::new();

        loop {
            if let Some(event_result) = event_stream.next().await {
                match event_result {
                    Ok(Event::Key(key_event)) => {
                        match (key_event.code, key_event.modifiers, key_event.kind) {
                            // Handle 'c' key press to copy to clipboard
                            (KeyCode::Char('c'), KeyModifiers::NONE, KeyEventKind::Press) => {
                                match Clipboard::new() {
                                    Ok(mut clipboard) => {
                                        if clipboard.set_text(&connection_command_clone).is_ok() {
                                            println!("\r\n✓ Connection command copied to clipboard!\r\n");
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
        let node_id = connection.remote_node_id()?;
        println!("\r\nAccepted connection from {node_id}\r");

        // Accept a bi-directional stream
        let (mut send, mut recv) = connection.accept_bi().await?;

        // Create a PTY system
        let pty_system = native_pty_system();

        // Create a PTY with initial size
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AcceptError::from_err(PtyError(format!("Failed to open PTY: {}", e))))?;

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

        let _child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| AcceptError::from_err(PtyError(format!("Failed to spawn bash: {}", e))))?;

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
        let send_task = tokio::spawn(async move {
            while let Some(data) = send_rx.recv().await {
                if send.write_all(&data).await.is_err() {
                    break;
                }
            }
        });

        // Spawn task to read from PTY and send to client
        let send_tx_clone = send_tx.clone();
        let pty_to_client = tokio::spawn(async move {
            let mut buffer = [0u8; 8192];
            let config = bincode::config::standard();
            let mut pty_ended = false;

            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        // EOF - bash has exited
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
                            let _ = send_tx_clone.send(full_msg);
                        }
                        break;
                    }
                    Ok(n) => {
                        let msg = ServerMessage::Output {
                            data: buffer[..n].to_vec(),
                        };
                        match bincode::encode_to_vec(&msg, config) {
                            Ok(encoded) => {
                                let len = (encoded.len() as u32).to_be_bytes();
                                let mut full_msg = Vec::new();
                                full_msg.extend_from_slice(&len);
                                full_msg.extend_from_slice(&encoded);

                                if send_tx_clone.send(full_msg).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    Err(_) => break,
                }
            }

            // Signal that PTY has ended
            if pty_ended {
                let _ = pty_ended_tx.send(()).await;
            }
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
                    let mut len_bytes = [0u8; 4];
                    if recv.read_exact(&mut len_bytes).await.is_err() {
                        return None;
                    }
                    let len = u32::from_be_bytes(len_bytes) as usize;

                    let mut msg_bytes = vec![0u8; len];
                    if recv.read_exact(&mut msg_bytes).await.is_err() {
                        return None;
                    }

                    Some(msg_bytes)
                } => {
                    match result {
                        Some(msg_bytes) => {
                            // Deserialize message
                            let msg: ClientMessage = match bincode::decode_from_slice(&msg_bytes, config) {
                                Ok((m, _)) => m,
                                Err(e) => {
                                    eprintln!("\r\nFailed to deserialize message: {}\r", e);
                                    continue;
                                }
                            };

                            match msg {
                                ClientMessage::KeyEvent { data } => {
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
                                    println!("\r\nClient requested disconnect\r");
                                    break;
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

        // Clean up
        pty_to_client.abort();
        send_task.abort();
        println!("\r\nConnection closed for {node_id}\r");

        // Wait until the remote closes the connection
        connection.closed().await;

        Ok(())
    }
}
