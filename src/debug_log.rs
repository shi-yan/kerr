//! Debug logging module for tracking shell session data flow
//!
//! Logs to ~/.kerr_debug.log to avoid interfering with shell display

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref LOG_FILE: Mutex<Option<std::fs::File>> = {
        // Try to open log file in user's home directory
        let log_path = match std::env::var("HOME") {
            Ok(home) => format!("{}/.kerr_debug.log", home),
            Err(_) => "/tmp/kerr_debug.log".to_string(),
        };

        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(file) => {
                eprintln!("\r\n[DEBUG] Logging to: {}\r", log_path);
                Mutex::new(Some(file))
            }
            Err(e) => {
                eprintln!("\r\n[DEBUG] Failed to open log file {}: {}\r", log_path, e);
                Mutex::new(None)
            }
        }
    };
}

/// Log a debug message with timestamp
pub fn log_debug(session_id: &str, message: &str) {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let log_line = format!("[{}] [{}] {}\n", timestamp, session_id, message);

    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut file) = *guard {
            let _ = file.write_all(log_line.as_bytes());
            let _ = file.flush();
        }
    }
}

/// Log PTY read event
pub fn log_pty_read(session_id: &str, bytes_read: usize) {
    log_debug(session_id, &format!("PTY_READ: {} bytes from bash", bytes_read));
}

/// Log PTY EOF (bash exited)
pub fn log_pty_eof(session_id: &str) {
    log_debug(session_id, "PTY_EOF: bash has exited");
}

/// Log PTY read error
pub fn log_pty_error(session_id: &str, error: &str) {
    log_debug(session_id, &format!("PTY_ERROR: {}", error));
}

/// Log message queued for sending
pub fn log_msg_queued(session_id: &str, msg_type: &str, encoded_size: usize) {
    log_debug(
        session_id,
        &format!("MSG_QUEUED: type={}, encoded_size={} bytes", msg_type, encoded_size)
    );
}

/// Log message queue send failure
pub fn log_queue_send_failed(session_id: &str, msg_type: &str) {
    log_debug(session_id, &format!("QUEUE_FAILED: type={}, channel closed", msg_type));
}

/// Log QUIC write start
pub fn log_quic_write_start(session_id: &str, size: usize) {
    log_debug(session_id, &format!("QUIC_WRITE_START: {} bytes", size));
}

/// Log QUIC write success
pub fn log_quic_write_done(session_id: &str, size: usize) {
    log_debug(session_id, &format!("QUIC_WRITE_DONE: {} bytes sent", size));
}

/// Log QUIC write failure
pub fn log_quic_write_failed(session_id: &str, size: usize, error: &str) {
    log_debug(
        session_id,
        &format!("QUIC_WRITE_FAILED: {} bytes, error={}", size, error)
    );
}

/// Log send task started
pub fn log_send_task_started(session_id: &str) {
    log_debug(session_id, "SEND_TASK: started");
}

/// Log send task ended
pub fn log_send_task_ended(session_id: &str, reason: &str) {
    log_debug(session_id, &format!("SEND_TASK: ended, reason={}", reason));
}

/// Log PTY task started
pub fn log_pty_task_started(session_id: &str) {
    log_debug(session_id, "PTY_TASK: started");
}

/// Log PTY task ended
pub fn log_pty_task_ended(session_id: &str, reason: &str) {
    log_debug(session_id, &format!("PTY_TASK: ended, reason={}", reason));
}

/// Log session started
pub fn log_session_start(session_id: &str) {
    log_debug(session_id, "SESSION_START: shell session beginning");
}

/// Log session ended
pub fn log_session_end(session_id: &str) {
    log_debug(session_id, "SESSION_END: connection closed");
}

/// Log client input received
pub fn log_client_input(session_id: &str, input_type: &str, size: usize) {
    log_debug(
        session_id,
        &format!("CLIENT_INPUT: type={}, size={} bytes", input_type, size)
    );
}
