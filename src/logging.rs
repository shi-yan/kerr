//! Logging infrastructure for kerr server
//!
//! Provides structured logging with file output, timestamps, and user context

use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize logging for the kerr server
///
/// Sets up tracing to write to the specified log file with:
/// - Timestamps for each log entry
/// - User information (username)
/// - Append mode (doesn't overwrite existing logs)
/// - Structured log format
///
/// Returns a guard that must be kept alive for the duration of the program
pub fn init_server_logging<P: AsRef<Path>>(log_file: P) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let log_path = log_file.as_ref();

    // Get current user information
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());

    let is_root = username == "root" || std::env::var("USER").map(|u| u == "root").unwrap_or(false);
    let user_info = if is_root {
        format!("USER=root (privileged)")
    } else {
        format!("USER={}", username)
    };

    // Write initial log header to file (append mode)
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let separator = "=".repeat(80);
        writeln!(file, "\n{}", separator)?;
        writeln!(file, "Kerr Server Started - {}", timestamp)?;
        writeln!(file, "{}", user_info)?;
        writeln!(file, "Log file: {}", log_path.display())?;
        writeln!(file, "{}\n", separator)?;
        file.flush()?;
    }

    // Create file appender for tracing
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;

    // IMPORTANT: Keep the guard alive! When it's dropped, logging stops.
    let (file_writer, guard) = tracing_appender::non_blocking(file);

    // Set up the tracing subscriber with file output
    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false) // No ANSI colors in log file
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(false);

    // Also output to stderr for interactive use
    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false);

    // Set up env filter (can be controlled via RUST_LOG env var)
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(stderr_layer)
        .init();

    tracing::info!(
        user = %username,
        is_root = is_root,
        log_file = %log_path.display(),
        "Logging initialized"
    );

    // Return the guard - caller MUST keep it alive!
    Ok(guard)
}

/// Initialize console-only logging (for commands other than serve)
pub fn init_console_logging() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
}
