// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

use sui_forking::{
    AdvanceCheckpointRequest, AdvanceClockRequest, ForkingServiceClient, GetStatusRequest,
    GraphQLClient, Node,
};

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

/// Default bind address for the RPC server.
const DEFAULT_RPC_ADDR: &str = "127.0.0.1:9000";

#[derive(Parser)]
#[command(name = "sui-forking", about = "Fork and interact with a Sui network")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start a forked Sui network
    Start {
        /// Network to fork from: mainnet, testnet, devnet, or a custom GraphQL URL
        #[arg(long, default_value = "mainnet")]
        network: Node,

        /// Checkpoint sequence number to fork at (defaults to latest)
        #[arg(long)]
        checkpoint: Option<u64>,

        /// Base directory for on-disk storage (overrides FORKING_DATA_STORE env var)
        #[arg(long)]
        data_dir: Option<PathBuf>,

        /// Address to bind the RPC server to
        #[arg(long, default_value = DEFAULT_RPC_ADDR)]
        rpc_addr: SocketAddr,
    },

    /// Advance the network clock by a given duration
    AdvanceClock {
        /// RPC server address to connect to
        #[arg(long, default_value = DEFAULT_RPC_ADDR)]
        rpc_addr: String,

        /// Duration in milliseconds to advance the clock (defaults to 1)
        #[arg(long)]
        duration_ms: Option<u64>,
    },

    /// Seal pending transactions into a new checkpoint
    AdvanceCheckpoint {
        /// RPC server address to connect to
        #[arg(long, default_value = DEFAULT_RPC_ADDR)]
        rpc_addr: String,
    },

    /// Get the current status of the forked network
    Status {
        /// RPC server address to connect to
        #[arg(long, default_value = DEFAULT_RPC_ADDR)]
        rpc_addr: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    match cli.command {
        Command::Start {
            network,
            checkpoint,
            data_dir,
            rpc_addr,
        } => cmd_start(network, checkpoint, data_dir, rpc_addr).await,
        Command::AdvanceClock {
            rpc_addr,
            duration_ms,
        } => cmd_advance_clock(&rpc_addr, duration_ms).await,
        Command::AdvanceCheckpoint { rpc_addr } => cmd_advance_checkpoint(&rpc_addr).await,
        Command::Status { rpc_addr } => cmd_status(&rpc_addr).await,
    }
}

async fn cmd_start(
    node: Node,
    checkpoint: Option<u64>,
    data_dir: Option<PathBuf>,
    rpc_addr: SocketAddr,
) -> Result<()> {
    let network_name = node.network_name();

    let checkpoint = match checkpoint {
        Some(cp) => cp,
        None => GraphQLClient::new(node.clone(), VERSION)?
            .get_latest_checkpoint_sequence_number()
            .await?
            .ok_or_else(|| anyhow::anyhow!("no checkpoints found for {}", network_name))?,
    };

    let context = sui_forking::startup::initialize(node, checkpoint, VERSION, data_dir).await?;

    println!(
        "Starting forked network from {} at checkpoint {} (rpc on {})",
        network_name, checkpoint, rpc_addr,
    );
    info!(
        "Starting forked network from {} at checkpoint {} (rpc on {})",
        network_name, checkpoint, rpc_addr,
    );

    let handle = tokio::spawn(sui_forking::startup::run(context, rpc_addr, VERSION));
    handle.await??;

    Ok(())
}

async fn cmd_advance_clock(rpc_addr: &str, duration_ms: Option<u64>) -> Result<()> {
    let mut client = ForkingServiceClient::connect(rpc_url(rpc_addr)).await?;
    let resp = client
        .advance_clock(AdvanceClockRequest { duration_ms })
        .await?
        .into_inner();

    println!("Clock advanced");
    println!(
        "  timestamp: {} ({})",
        resp.timestamp_ms,
        format_timestamp(resp.timestamp_ms)
    );
    println!("  tx digest: {}", resp.tx_digest);
    Ok(())
}

async fn cmd_advance_checkpoint(rpc_addr: &str) -> Result<()> {
    let mut client = ForkingServiceClient::connect(rpc_url(rpc_addr)).await?;
    let resp = client
        .advance_checkpoint(AdvanceCheckpointRequest {})
        .await?
        .into_inner();

    println!("Checkpoint sealed");
    println!("  sequence number: {}", resp.checkpoint_sequence_number);
    println!(
        "  timestamp:       {} ({})",
        resp.timestamp_ms,
        format_timestamp(resp.timestamp_ms)
    );
    Ok(())
}

async fn cmd_status(rpc_addr: &str) -> Result<()> {
    let mut client = ForkingServiceClient::connect(rpc_url(rpc_addr)).await?;
    let resp = client.get_status(GetStatusRequest {}).await?.into_inner();

    println!("Forked network status");
    println!("  epoch:                {}", resp.epoch);
    println!(
        "  checkpoint:           {}",
        resp.checkpoint_sequence_number
    );
    println!(
        "  timestamp:            {} ({})",
        resp.timestamp_ms,
        format_timestamp(resp.timestamp_ms)
    );
    println!("  forked at checkpoint: {}", resp.forked_at_checkpoint);
    Ok(())
}

fn rpc_url(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    }
}

fn format_timestamp(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| format!("{ms}ms"))
}
