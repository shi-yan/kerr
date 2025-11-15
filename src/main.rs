//! Kerr - A peer-to-peer remote shell application
//!
//! Named after the Kerr black hole solution, representing wormhole-like connections
//! between peers in a network.

use clap::{Parser, Subcommand};
use n0_snafu::Result;

#[derive(Parser)]
#[command(name = "kerr")]
#[command(about = "Peer-to-peer remote shell - like SSH through a wormhole", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Kerr server and accept incoming connections
    Serve {
        /// Register this connection with the backend server using the provided alias
        #[arg(long)]
        register: Option<String>,
        /// Path to session file (optional, defaults to standard config directory)
        #[arg(long)]
        session: Option<String>,
        /// Path to log file (logs will be appended with timestamps)
        #[arg(long)]
        log: Option<String>,
        /// Automatically update without prompting if update is available
        #[arg(long)]
        update: bool,
    },
    /// Connect to a Kerr server
    Connect {
        /// Connection string from the server
        connection_string: String,
    },
    /// Send a file or directory to the server
    Send {
        /// Connection string from the server
        connection_string: String,
        /// Local file or directory path
        local_path: String,
        /// Remote destination path
        remote_path: String,
        /// Force overwrite without confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Pull a file or directory from the server
    Pull {
        /// Connection string from the server
        connection_string: String,
        /// Remote file or directory path
        remote_path: String,
        /// Local destination path
        local_path: String,
    },
    /// Browse the filesystem with an interactive TUI
    Browse {
        /// Optional connection string to browse remote filesystem
        connection_string: Option<String>,
    },
    /// Create a TCP relay proxy to forward local port to remote port
    Relay {
        /// Connection string from the server
        connection_string: String,
        /// Local port to listen on
        local_port: u16,
        /// Remote port to forward to
        remote_port: u16,
    },
    /// Test network performance with increasing payload sizes
    Ping {
        /// Connection string from the server
        connection_string: String,
    },
    /// Login with Google OAuth2
    Login,
    /// Logout and invalidate the current session
    Logout,
    /// List all registered connections
    Ls,
    /// Start a web-based UI for remote file browsing and editing
    Ui {
        /// Optional connection string from the server (if not provided, will show connection selector)
        connection_string: Option<String>,
        /// Port to run the web server on (default: 3000)
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { register, session, log, update } => {
            // Initialize logging if log file is specified
            // IMPORTANT: Keep _guard alive for the entire server lifetime
            let _guard = if let Some(log_file) = &log {
                match kerr::logging::init_server_logging(log_file) {
                    Ok(guard) => Some(guard),
                    Err(e) => {
                        eprintln!("Failed to initialize logging: {}", e);
                        eprintln!("Continuing without file logging...");
                        kerr::logging::init_console_logging();
                        None
                    }
                }
            } else {
                kerr::logging::init_console_logging();
                None
            };

            // Acquire instance lock to ensure single instance
            let _instance_lock = match kerr::lock::InstanceLock::try_acquire() {
                Ok(lock) => {
                    tracing::info!("Instance lock acquired successfully");
                    Some(lock)
                }
                Err(e) => {
                    eprintln!("\n{}", e);
                    eprintln!("\nIf you believe this is an error, you can manually remove the lock file:");
                    if let Ok(lock_path) = kerr::lock::InstanceLock::get_path() {
                        eprintln!("  {}", lock_path.display());
                    }
                    std::process::exit(1);
                }
            };

            // Check for updates on startup (unless in debug mode)
            if !kerr::update::is_debug_mode() {
                if let Err(e) = check_and_prompt_for_update(update).await {
                    tracing::warn!("Update check failed: {}", e);
                }
            }

            kerr::server::run_server(register, session).await?;
        }
        Commands::Connect { connection_string } => {
            kerr::client::run_client(connection_string).await?;
        }
        Commands::Send { connection_string, local_path, remote_path, force } => {
            kerr::client::send_file(connection_string, local_path, remote_path, force).await?;
        }
        Commands::Pull { connection_string, remote_path, local_path } => {
            kerr::client::pull_file(connection_string, remote_path, local_path).await?;
        }
        Commands::Browse { connection_string } => {
            if let Some(conn_str) = connection_string {
                // Browse remote filesystem
                kerr::client::browse_remote(conn_str).await?;
            } else {
                // Browse local filesystem
                kerr::browser::run_browser()
                    .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Browser error: {}", e)))?;
            }
        }
        Commands::Relay { connection_string, local_port, remote_port } => {
            kerr::client::run_tcp_relay(&connection_string, local_port, remote_port).await?;
        }
        Commands::Ping { connection_string } => {
            kerr::client::ping_test(connection_string).await?;
        }
        Commands::Login => {
            kerr::auth::login().await?;
        }
        Commands::Logout => {
            kerr::auth::logout().await?;
        }
        Commands::Ls => {
            // Fetch connections from backend
            let connections_response = kerr::auth::fetch_connections().await?;

            // Show the interactive list
            match kerr::connections_list::run_connections_list(connections_response.connections)? {
                Some(selected_connection) => {
                    // User selected a connection, print the commands
                    println!("\n╔══════════════════════════════════════════════════════════════╗");
                    println!("║             Connection Commands                              ║");
                    println!("╚══════════════════════════════════════════════════════════════╝\n");

                    let alias = selected_connection.alias.as_deref().unwrap_or("(no alias)");
                    let host = &selected_connection.host_name;
                    println!("Selected: {} @ {}\n", alias, host);

                    let conn_str = &selected_connection.connection_string;
                    println!("Commands:");
                    println!("  Connect: kerr connect {}", conn_str);
                    println!("  Send:    kerr send {} <local> <remote>", conn_str);
                    println!("  Pull:    kerr pull {} <remote> <local>", conn_str);
                    println!("  Browse:  kerr browse {}", conn_str);
                    println!("  Ping:    kerr ping {}", conn_str);
                    println!("  Web UI:  kerr ui {}", conn_str);
                    println!();
                }
                None => {
                    println!("No connection selected.");
                }
            }
        }
        Commands::Ui { connection_string, port } => {
            kerr::web_ui::run_web_ui(connection_string, port).await
                .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Web UI error: {}", e)))?;
        }
    }

    Ok(())
}

/// Check for updates and prompt user (unless auto-update is enabled)
async fn check_and_prompt_for_update(auto_update: bool) -> anyhow::Result<()> {
    use std::io::{self, Write};

    // Load config
    let config = match kerr::config::ServerConfig::load() {
        Ok(c) => c,
        Err(_) => {
            // No config found, skip update check
            tracing::debug!("No server config found, skipping update check");
            return Ok(());
        }
    };

    // Check for updates
    tracing::info!("Checking for updates...");
    let version_info = match kerr::update::check_for_updates(&config).await? {
        Some(info) => info,
        None => {
            tracing::info!("No updates available (current version: {})", kerr::VERSION);
            return Ok(());
        }
    };

    let current_version = kerr::VERSION;
    let new_version = &version_info.version;

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                  Update Available                            ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("  Current version: {}", current_version);
    println!("  New version:     {}", new_version);
    println!();

    // If auto-update is enabled, proceed immediately
    if auto_update {
        println!("Auto-update enabled. Starting update...\n");
        tracing::info!("Auto-update enabled, proceeding with update from {} to {}", current_version, new_version);

        // Perform the update (this will exit the process if successful)
        if let Err(e) = kerr::update::perform_update(&config).await {
            eprintln!("Update failed: {}", e);
            tracing::error!("Update failed: {}", e);
            return Err(e);
        }

        return Ok(());
    }

    // Prompt user for confirmation
    print!("Do you want to update now? [Y/n]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input.is_empty() || input == "y" || input == "yes" {
        println!("\nStarting update...");
        tracing::info!("User confirmed update from {} to {}", current_version, new_version);

        // Perform the update (this will exit the process if successful)
        if let Err(e) = kerr::update::perform_update(&config).await {
            eprintln!("Update failed: {}", e);
            tracing::error!("Update failed: {}", e);
            return Err(e);
        }
    } else {
        println!("\nUpdate skipped. You can update later by restarting with --update flag.");
        tracing::info!("User declined update");
    }

    Ok(())
}
