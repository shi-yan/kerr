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

pub async fn run_server() -> Result<()> {
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;

    // Build our protocol handler and add our protocol, identified by its ALPN, and spawn the node.
    let router = Router::builder(endpoint).accept(ALPN, KerrServer).spawn();

    // Wait for the node to be online
    router.endpoint().online().await;

    let addr = router.endpoint().node_addr();

    // Encode the address as a compressed connection string (JSON -> gzip -> base64)
    let connection_string = crate::encode_connection_string(&addr);

    // Build the connection commands
    let connect_command = format!("kerr connect {}", connection_string);
    let send_command = format!("kerr send {}", connection_string);
    let pull_command = format!("kerr pull {}", connection_string);
    let browse_command = format!("kerr browse {}", connection_string);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    Kerr Server Online                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");
    println!("Commands:");
    println!("  Connect: {}", connect_command);
    println!("  Send:    {} <local> <remote>", send_command);
    println!("  Pull:    {} <remote> <local>", pull_command);
    println!("  Browse:  {}", browse_command);
    println!("\n─────────────────────────────────────────────────────────────────");
    println!("Keys: [c]onnect | [s]end | [p]ull | [b]rowse | Ctrl+C to stop");
    println!("─────────────────────────────────────────────────────────────────\n");

    // Enable raw mode for keyboard event handling
    enable_raw_mode().unwrap_or_else(|err| eprintln!("Failed to enable raw mode: {err}"));

    // Spawn task to handle keyboard events
    let connect_clone = connect_command.clone();
    let send_clone = send_command.clone();
    let pull_clone = pull_command.clone();

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
        let (send, mut recv) = connection.accept_bi().await?;

        // Read the Hello message to determine session type
        let config = bincode::config::standard();
        let mut len_bytes = [0u8; 4];
        if recv.read_exact(&mut len_bytes).await.is_err() {
            eprintln!("\r\nFailed to read Hello message length\r");
            return Ok(());
        }
        let len = u32::from_be_bytes(len_bytes) as usize;
        let mut msg_bytes = vec![0u8; len];
        if recv.read_exact(&mut msg_bytes).await.is_err() {
            eprintln!("\r\nFailed to read Hello message\r");
            return Ok(());
        }

        let hello_msg: crate::ClientMessage = match bincode::decode_from_slice(&msg_bytes, config) {
            Ok((m, _)) => m,
            Err(e) => {
                eprintln!("\r\nFailed to deserialize Hello message: {}\r", e);
                return Ok(());
            }
        };

        let session_type = match hello_msg {
            crate::ClientMessage::Hello { session_type } => session_type,
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
        }
    }
}

impl KerrServer {
    async fn handle_shell_session(
        node_id: iroh::PublicKey,
        mut send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
    ) -> Result<(), AcceptError> {
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

        // Clean up
        pty_to_client.abort();
        send_task.abort();
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
                    // TODO: Implement file download
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
            if let Err(_) = recv.read_exact(&mut len_bytes).await {
                break;
            }
            let msg_len = u32::from_be_bytes(len_bytes) as usize;

            // Read message
            let mut msg_bytes = vec![0u8; msg_len];
            if let Err(_) = recv.read_exact(&mut msg_bytes).await {
                break;
            }

            // Deserialize message
            let (msg, _): (crate::ClientMessage, _) = match bincode::decode_from_slice(&msg_bytes, config) {
                Ok(m) => m,
                Err(_) => continue,
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
                                    let metadata = std::fs::metadata(&path);

                                    if let Ok(metadata) = metadata {
                                        let file_name = path.file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("")
                                            .to_string();

                                        let is_dir = metadata.is_dir();

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
                                    }
                                }
                            }

                            let entries_json = serde_json::to_string(&file_entries).unwrap();
                            crate::ServerMessage::FsDirListing { entries_json }
                        }
                        Err(e) => {
                            crate::ServerMessage::Error {
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
                            crate::ServerMessage::Error {
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
                            crate::ServerMessage::Error {
                                message: format!("Failed to read file: {}", e),
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
                    if let Err(_) = send.write_all(&len).await {
                        break;
                    }
                    if let Err(_) = send.write_all(&encoded).await {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        println!("\r\nFile browser session closed for {node_id}\r");
        Ok(())
    }
}
