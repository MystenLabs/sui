// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::{Parser, Subcommand};

use sui_types::base_types::SuiAddress;

use crate::seeds::InitialAccounts;

const RPC_PORT: &str = "9000";
const SERVER_PORT: &str = "9001";
const IP: &str = "127.0.0.1";
const DEFAULT_ADDRESS: &str = "http://127.0.0.1:9001";

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
        /// Port to bind the gRPC RPC service. Default is 9000.
        #[clap(long, default_value = RPC_PORT)]
        rpc_port: u16,

        /// Port to bind the HTTP forking server. Default is 9001.
        #[clap(long, default_value = SERVER_PORT)]
        server_port: u16,

        /// Host IP address to bind the server. Default is localhost.
        #[clap(long, default_value = IP)]
        host: String,

        /// Checkpoint to fork from.
        /// If a local fork cache exists for this checkpoint, startup resumes from the latest
        /// locally cached checkpoint in that fork directory.
        /// For older checkpoints, use `--objects` instead of `--accounts` to seed startup data.
        /// If not provided, forks from the remote latest checkpoint.
        #[clap(long)]
        checkpoint: Option<u64>,

        /// Network to fork from: `mainnet`, `testnet`, `devnet`, or a full GraphQL URL.
        /// Any non-keyword value must be a valid `http(s)` GraphQL endpoint URL.
        #[clap(long, default_value = "mainnet")]
        network: String,

        /// Optional fullnode RPC URL.
        /// Required when `--network` is a custom GraphQL URL.
        #[clap(long)]
        fullnode_url: Option<String>,

        /// Optional data directory for storing forked data
        #[clap(long)]
        data_dir: Option<String>,
        /// Startup seed inputs.
        /// Use either `--accounts` or `--objects` (mutually exclusive), or neither.
        #[clap(flatten)]
        accounts: InitialAccounts,
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
