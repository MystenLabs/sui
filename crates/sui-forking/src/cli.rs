// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// use crate::seeds::InitialSeeds;
use clap::{Parser, Subcommand};
use sui_types::base_types::SuiAddress;

const PORT: &str = "3001";
const IP: &str = "127.0.0.1";
const DEFAULT_ADDRESS: &str = "http://127.0.0.1:3001";

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
        /// Port to bind the server. Default is 3001
        #[clap(long, default_value = PORT)]
        port: u16,

        /// Host IP address to bind the server. Default is localhost.
        #[clap(long, default_value = IP)]
        host: String,

        /// Checkpoint to fork from. If not provided, forks from the latest checkpoint.
        #[clap(long)]
        checkpoint: Option<u64>,

        /// Network to fork from (e.g., mainnet, testnet, devnet, or a custom one).
        #[clap(long, default_value = "mainnet")]
        network: String,

        /// Optional data directory for storing forked data
        #[clap(long)]
        data_dir: Option<String>,
        // /// Initial accounts to restore with their owned objects
        // #[clap(flatten)]
        // accounts: InitialSeeds,
    },
    /// Advance checkpoint by 1
    AdvanceCheckpoint {
        #[clap(long, default_value = DEFAULT_ADDRESS)]
        server_url: String,
    },
    /// Advance clock by specified duration in seconds
    AdvanceClock {
        #[clap(long, default_value = DEFAULT_ADDRESS)]
        server_url: String,
        #[clap(long, default_value = "1")]
        seconds: u64,
    },
    /// Advance to next epoch
    AdvanceEpoch {
        #[clap(long, default_value = DEFAULT_ADDRESS)]
        server_url: String,
    },
    /// Get current status
    Status {
        #[clap(long, default_value = DEFAULT_ADDRESS)]
        server_url: String,
    },
    /// Execute a transaction
    ExecuteTx {
        #[clap(long, default_value = DEFAULT_ADDRESS)]
        server_url: String,
        /// Base64 encoded transaction bytes
        #[clap(long)]
        tx_bytes: String,
    },
    Faucet {
        #[clap(long, default_value = DEFAULT_ADDRESS)]
        server_url: String,
        #[clap(long)]
        address: SuiAddress,
        #[clap(long)]
        amount: u64,
    },
}
