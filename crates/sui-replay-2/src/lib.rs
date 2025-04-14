// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use similar::{ChangeTag, TextDiff};
use std::str::FromStr;
use sui_types::{effects::TransactionEffects, supported_protocol_versions::Chain};

pub mod data_store;
pub mod environment;
pub mod errors;
pub mod execution;
pub mod gql_queries;
pub mod replay_txn;

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui Replay Tool",
    about = "Replay executed transactions.",
    rename_all = "kebab-case"
)]
pub struct ReplayConfig {
    /// RPC of the fullnode used to replay the transaction.
    #[arg(long, short, default_value = "mainnet")]
    pub node: Node,
    /// Transaction digest to replay.
    #[arg(long, short)]
    pub tx_digest: String,
    /// Show transaction effects.
    #[arg(long, short, default_value = "false")]
    pub show_effects: bool,
    /// Verify transaction execution matches what was executed on chain.
    #[arg(long, short, default_value = "true")]
    pub verify: bool,
    // Enable tracing for tests
    #[arg(long = "trace-execution", default_value = None)]
    pub trace_execution: Option<Option<String>>,
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

/// Utility to diff effects in a human readable format
pub fn diff_effects(
    expected_effect: &TransactionEffects,
    txn_effects: &TransactionEffects,
) -> String {
    let expected = format!("{:#?}", expected_effect);
    let result = format!("{:#?}", txn_effects);
    let mut res = vec![];

    let diff = TextDiff::from_lines(&expected, &result);
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "---",
            ChangeTag::Insert => "+++",
            ChangeTag::Equal => "   ",
        };
        res.push(format!("{}{}", sign, change));
    }

    res.join("")
}
