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
    }

    Ok(())
}
