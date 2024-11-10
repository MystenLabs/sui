// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use sui_types::supported_protocol_versions::Chain;
use std::str::FromStr;

pub mod data_store;
pub mod environment;
pub mod epoch_store;
pub mod errors;
pub mod execution;
pub mod replay_txn_data;

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui Replay Tool",
    about = "Replay executed transactions.",
    rename_all = "kebab-case"
)]
pub enum ReplayCommand {
    /// Replay transaction
    #[command(name = "tx")]
    ReplayTransaction {
        /// RPC of the fullnode used to replay the transaction.
        #[arg(long, short, default_value = "mainnet")]
        node: Node,
        /// Transaction digest to replay.
        #[arg(long, short)]
        tx_digest: String,
        /// Show transaction effects.
        #[arg(long, short, default_value = "false")]
        show_effects: bool,
        /// Verify transaction execution matches what was executed on chain.
        #[arg(long, short, default_value = "false")]
        verify: bool,
        /// Required config objects and versions of the config objects to use if replaying a
        /// transaction that utilizes the config object for regulated coin types and that has been
        /// denied.
        #[arg(long, short, num_args = 2..)]
        config_objects: Option<Vec<String>>,
    },
}

#[derive(Clone, Debug)]
pub enum Node {
    Mainnet,
    Testnet,
    Devnet,
    Custom(String),
}

impl Node {
    pub fn chain(&self) -> Chain {
        match self {
            Node::Mainnet => Chain::Mainnet,
            Node::Testnet => Chain::Testnet,
            Node::Devnet => Chain::Unknown,
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
            "devnet" => Ok(Node::Devnet),
            _ => Ok(Node::Custom(s.to_string())),
        }
    }
    
}