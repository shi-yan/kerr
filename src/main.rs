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
    Serve,
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
    /// Browse the local filesystem with an interactive TUI
    Browse,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve => {
            kerr::server::run_server().await?;
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
        Commands::Browse => {
            kerr::browser::run_browser()
                .map_err(|e| n0_snafu::Error::anyhow(anyhow::anyhow!("Browser error: {}", e)))?;
        }
    }

    Ok(())
}
