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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { register } => {
            kerr::server::run_server(register).await?;
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
                    println!();
                }
                None => {
                    println!("No connection selected.");
                }
            }
        }
    }

    Ok(())
}
