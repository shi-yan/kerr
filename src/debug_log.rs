//! Debug logging module for tracking shell session data flow
//!
//! Uses tracing framework - logs go to file specified by --log or to stderr

/// Log a debug message with session_id
pub fn log_debug(session_id: &str, message: &str) {
    tracing::debug!(session_id = session_id, "{}", message);
}

/// Log a new session starting with clear separator
pub fn log_new_session_separator(session_id: &str, session_type: &str) {
    tracing::info!("========================================");
    tracing::info!("NEW SESSION: {} [{}]", session_type, session_id);
    tracing::info!("========================================");
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

/// Log connection acceptance
pub fn log_connection_accepted(session_id: &str, remote_addr: &str) {
    log_debug(
        session_id,
        &format!("CONNECTION_ACCEPTED: remote_addr={}", remote_addr)
    );
}

/// Log bidirectional stream accepted
pub fn log_bi_stream_accepted(session_id: &str) {
    log_debug(session_id, "BI_STREAM_ACCEPTED: bidirectional stream ready");
}

/// Log Hello message received
pub fn log_hello_received(session_id: &str, session_type: &str) {
    log_debug(
        session_id,
        &format!("HELLO_RECEIVED: session_type={}", session_type)
    );
}

/// Log PTY creation start
pub fn log_pty_creation_start(session_id: &str, cols: u16, rows: u16) {
    log_debug(
        session_id,
        &format!("PTY_CREATE_START: cols={}, rows={}", cols, rows)
    );
}

/// Log PTY creation success
pub fn log_pty_created(session_id: &str, pty_fd: i32) {
    log_debug(
        session_id,
        &format!("PTY_CREATED: success, fd={}", pty_fd)
    );
}

/// Log PTY creation failure
pub fn log_pty_creation_failed(session_id: &str, error: &str) {
    log_debug(
        session_id,
        &format!("PTY_CREATE_FAILED: {}", error)
    );
}

/// Log bash spawn start
pub fn log_bash_spawn_start(session_id: &str) {
    log_debug(session_id, "BASH_SPAWN_START: spawning bash process");
}

/// Log bash spawn success
pub fn log_bash_spawned(session_id: &str, pid: u32) {
    log_debug(
        session_id,
        &format!("BASH_SPAWNED: success, pid={}", pid)
    );
}

/// Log bash spawn failure
pub fn log_bash_spawn_failed(session_id: &str, error: &str) {
    log_debug(
        session_id,
        &format!("BASH_SPAWN_FAILED: {}", error)
    );
}

/// Log QUIC stream read start
pub fn log_quic_read_start(session_id: &str) {
    log_debug(session_id, "QUIC_READ_START: waiting for data from client");
}

/// Log QUIC stream read success
pub fn log_quic_read_done(session_id: &str, bytes_read: usize) {
    log_debug(
        session_id,
        &format!("QUIC_READ_DONE: {} bytes received", bytes_read)
    );
}

/// Log QUIC stream read failure
pub fn log_quic_read_failed(session_id: &str, error: &str) {
    log_debug(
        session_id,
        &format!("QUIC_READ_FAILED: {}", error)
    );
}

/// Log message decode start
pub fn log_decode_start(session_id: &str, buffer_size: usize) {
    log_debug(
        session_id,
        &format!("DECODE_START: buffer_size={} bytes", buffer_size)
    );
}

/// Log message decode success
pub fn log_decode_done(session_id: &str, msg_type: &str) {
    log_debug(
        session_id,
        &format!("DECODE_DONE: msg_type={}", msg_type)
    );
}

/// Log message decode failure
pub fn log_decode_failed(session_id: &str, error: &str) {
    log_debug(
        session_id,
        &format!("DECODE_FAILED: {}", error)
    );
}

/// Log WebSocket connection started
pub fn log_ws_connection_start(session_id: &str) {
    log_debug(session_id, "WS_CONNECTION_START: WebSocket shell connection initiated");
}

/// Log WebSocket to QUIC task started
pub fn log_ws_to_quic_task_started(session_id: &str) {
    log_debug(session_id, "WS_TO_QUIC_TASK: started");
}

/// Log WebSocket to QUIC task ended
pub fn log_ws_to_quic_task_ended(session_id: &str, reason: &str) {
    log_debug(
        session_id,
        &format!("WS_TO_QUIC_TASK: ended, reason={}", reason)
    );
}

/// Log QUIC to WebSocket task started
pub fn log_quic_to_ws_task_started(session_id: &str) {
    log_debug(session_id, "QUIC_TO_WS_TASK: started");
}

/// Log QUIC to WebSocket task ended
pub fn log_quic_to_ws_task_ended(session_id: &str, reason: &str) {
    log_debug(
        session_id,
        &format!("QUIC_TO_WS_TASK: ended, reason={}", reason)
    );
}

/// Log WebSocket message received
pub fn log_ws_msg_received(session_id: &str, size: usize) {
    log_debug(
        session_id,
        &format!("WS_MSG_RECEIVED: {} bytes from browser", size)
    );
}

/// Log WebSocket message sent
pub fn log_ws_msg_sent(session_id: &str, size: usize) {
    log_debug(
        session_id,
        &format!("WS_MSG_SENT: {} bytes to browser", size)
    );
}
