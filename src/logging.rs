//! Logging infrastructure for kerr server
//!
//! Provides structured logging with file output, timestamps, and user context

use anyhow::Result;
use std::fs::{self, OpenOptions};
use std::io::{IsTerminal, Write};
use std::path::Path;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// A writer that converts \n to \r\n for proper terminal display in raw mode
struct CrLfWriter<W: Write> {
    inner: W,
}

impl<W: Write> CrLfWriter<W> {
    fn new(inner: W) -> Self {
        Self { inner }
    }
}

impl<W: Write> Write for CrLfWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut output = Vec::new();
        for &byte in buf {
            if byte == b'\n' {
                output.push(b'\r');
                output.push(b'\n');
            } else {
                output.push(byte);
            }
        }
        self.inner.write_all(&output)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Initialize logging for the kerr server.
///
/// Logs to the given file (append mode) and also to stderr.
/// When stderr is a TTY (interactive terminal), `\n` is converted to `\r\n`
/// so output is correct under crossterm raw mode. When stderr is not a TTY
/// (e.g. a systemd service piping to journald), plain line endings are used
/// so journald does not record spurious blank lines.
///
/// Returns a guard that must be kept alive for the duration of the program.
pub fn init_server_logging<P: AsRef<Path>>(log_file: P) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let log_path = log_file.as_ref();

    // Ensure the log directory exists before we try to open/create the file.
    if let Some(parent) = log_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    // Get current user information
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    let is_root = username == "root";
    let user_info = if is_root {
        "USER=root (privileged)".to_string()
    } else {
        format!("USER={}", username)
    };

    // Write session header (append mode — never overwrites prior runs)
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let separator = "=".repeat(80);
        writeln!(file, "\n{}", separator)?;
        writeln!(file, "Kerr Server Started - {}", timestamp)?;
        writeln!(file, "{}", user_info)?;
        writeln!(file, "Log file: {}", log_path.display())?;
        writeln!(file, "{}\n", separator)?;
        file.flush()?;
    }

    let file = OpenOptions::new().create(true).append(true).open(log_path)?;
    let (file_writer, guard) = tracing_appender::non_blocking(file);

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(false);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // When stderr is a TTY (interactive), raw mode is active so \n alone does
    // not move the cursor to column 0 — we need \r\n.  When stderr goes to
    // journald or a pipe, plain \n is correct; CrLfWriter would produce blank
    // lines between every tracing record in `journalctl`.
    if std::io::stderr().is_terminal() {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .with(
                fmt::layer()
                    .with_writer(|| CrLfWriter::new(std::io::stderr()))
                    .with_ansi(true)
                    .with_target(false)
                    .with_thread_ids(false)
                    .with_line_number(false),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .with(
                fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_ansi(false)
                    .with_target(false)
                    .with_thread_ids(false)
                    .with_line_number(false),
            )
            .init();
    }

    tracing::info!(
        user = %username,
        is_root = is_root,
        log_file = %log_path.display(),
        "Logging initialized"
    );

    Ok(guard)
}

/// Initialize console-only logging (for commands other than serve).
pub fn init_console_logging() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::fmt()
        .with_writer(|| CrLfWriter::new(std::io::stderr()))
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
}
