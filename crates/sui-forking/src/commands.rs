use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[clap(name = "sui-forking")]
#[clap(about = "Minimal CLI for Sui forking with simulacrum", long_about = None)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the forking server
    Start {
        #[clap(long, default_value = "8123")]
        port: u16,

        #[clap(long, default_value = "127.0.0.1")]
        host: String,

        #[clap(long)]
        checkpoint: Option<u64>,

        #[clap(long, default_value = "mainnet")]
        network: String,

        #[clap(long)]
        data_dir: Option<String>,
    },
    /// Advance checkpoint by 1
    AdvanceCheckpoint {
        #[clap(long, default_value = "http://localhost:8123")]
        server_url: String,
    },
    /// Advance clock by specified duration in seconds
    AdvanceClock {
        #[clap(long, default_value = "http://localhost:8123")]
        server_url: String,
        #[clap(long, default_value = "1")]
        seconds: u64,
    },
    /// Advance to next epoch
    AdvanceEpoch {
        #[clap(long, default_value = "http://localhost:8123")]
        server_url: String,
    },
    /// Get current status
    Status {
        #[clap(long, default_value = "http://localhost:8123")]
        server_url: String,
    },
    /// Execute a transaction
    ExecuteTx {
        #[clap(long, default_value = "http://localhost:8123")]
        server_url: String,
        /// Base64 encoded transaction bytes
        #[clap(long)]
        tx_bytes: String,
    },
}

