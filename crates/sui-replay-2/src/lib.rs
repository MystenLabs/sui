// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::path::PathBuf;
use std::str::FromStr;
use sui_types::supported_protocol_versions::Chain;

pub mod data_store;
pub mod execution;
pub mod gql_queries;
pub mod replay_interface;
pub mod replay_txn;
pub mod tracing;

/// Arguments to the replay tool.
/// It allows to replay a single transaction by digest or
/// a file containing multiple digests, one per line.
/// This may evolve to something very different in time and
/// it's not meant to be stable.
/// The options available are very convenient for the current
/// development cycle.
#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui Replay Tool",
    about = "Replay executed transactions.",
    rename_all = "kebab-case"
)]
pub struct ReplayConfig {
    /// Transaction digest to replay.
    #[arg(long, short)]
    pub digest: Option<String>,
    /// File containing a list of digest, one per line.
    #[arg(long)]
    pub digests_path: Option<PathBuf>,
    /// RPC of the fullnode used to replay the transaction.
    #[arg(long, short, default_value = "mainnet")]
    pub node: Node,
    /// Show transaction effects.
    #[arg(long, short, default_value = "false")]
    pub show_effects: bool,
    /// Verify transaction execution matches what was executed on chain.
    #[arg(long, short, default_value = "false")]
    pub verify: bool,
    /// Provide a directory to collect tracing. Or defaults to `<cur_dir>/.replay/<digest>`
    #[arg(long = "trace", default_value = None)]
    pub trace: Option<Option<PathBuf>>,
}

/// Enum around rpc gql endpoints.
#[derive(Clone, Debug)]
pub enum Node {
    Mainnet,
    Testnet,
    // TODO: define once we have stable end points.
    //       Use `Custom` for now.
    // Devnet,
    Custom(String),
}

impl Node {
    pub fn chain(&self) -> Chain {
        match self {
            Node::Mainnet => Chain::Mainnet,
            Node::Testnet => Chain::Testnet,
            // Node::Devnet => Chain::Unknown,
            Node::Custom(_) => Chain::Unknown,
        }
    }
}

impl FromStr for Node {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(Node::Mainnet),
            "testnet" => Ok(Node::Testnet),
            // "devnet" => Ok(Node::Devnet),
            _ => Ok(Node::Custom(s.to_string())),
        }
    }
}
