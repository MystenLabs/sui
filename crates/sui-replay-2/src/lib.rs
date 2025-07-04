// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::artifacts::ArtifactManager;
use crate::build::BuildCmdConfig;
use crate::data_store::DataStore;
use crate::replay_txn::replay_transaction;
use anyhow::{anyhow, bail};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::str::FromStr;
use sui_types::supported_protocol_versions::Chain;

pub mod artifacts;
pub mod build;
pub mod data_store;
pub mod displays;
pub mod execution;
pub mod gql_queries;
pub mod replay_interface;
pub mod replay_txn;
pub mod tracing;

const DEFAULT_OUTPUT_DIR: &str = ".replay";

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
pub struct Config {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub replay: ReplayConfig,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Commands {
    /// Build and prepare replay data
    #[clap(alias = "b")]
    Build(BuildCmdConfig),
}

/// Arguments for the (implicit) replay command.
#[derive(Parser, Clone, Debug)]
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
    /// Provide a directory to collect tracing. Or defaults to `<cur_dir>/.replay/<digest>`
    #[arg(long = "trace", default_value = "false")]
    pub trace: bool,
    /// Terminate a batch replay early if an error occurs when replaying one of the transactions.
    #[arg(long, default_value = "false")]
    pub terminate_early: bool,
    /// The output directory for the replay artifacts. Defaults `<cur_dir>/.replay/<digest>`.
    #[arg(long, short)]
    pub output_dir: Option<PathBuf>,
    /// Show transaction effects.
    #[arg(long, short, default_value = "false")]
    pub show_effects: bool,
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

pub fn handle_replay_config(config: ReplayConfig, version: &str) -> anyhow::Result<PathBuf> {
    let ReplayConfig {
        node,
        digest,
        digests_path,
        trace,
        mut terminate_early,
        output_dir,
        show_effects: _,
    } = config;

    let output_root_dir = if let Some(dir) = output_dir {
        dir
    } else {
        // Default output directory is `<cur_dir>/.replay/<digest>`
        let current_dir =
            std::env::current_dir().map_err(|e| anyhow!("Failed to get current directory: {e}"))?;
        current_dir.join(DEFAULT_OUTPUT_DIR)
    };

    // If a file is specified it is read and the digest ignored.
    // Once we decide on the options we want this is likely to change.
    let digests = if let Some(digests_path) = digests_path {
        // read digests from file
        std::fs::read_to_string(digests_path.clone())
            .map_err(|e| {
                anyhow!(
                    "Failed to read digests file {}: {e}",
                    digests_path.display(),
                )
            })?
            .lines()
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>()
    } else if let Some(tx_digest) = digest {
        // terminate early if a single digest is provided this way we get proper error messages from
        terminate_early = true;
        // single digest provided
        vec![tx_digest]
    } else {
        bail!("either --digest or --digests-path must be provided");
    };

    ::tracing::debug!("Binary version: {version}");

    // `DataStore` implements `TransactionStore`, `EpochStore` and `ObjectStore`
    let data_store = DataStore::new(node, version)
        .map_err(|e| anyhow!("Failed to create data store: {:?}", e))?;

    // load and replay transactions
    for tx_digest in digests {
        let tx_dir = output_root_dir.join(&tx_digest);
        let artifact_manager = ArtifactManager::new(&tx_dir, true /* overrides_allowed */)?;
        match replay_transaction(&artifact_manager, &tx_digest, &data_store, trace) {
            Err(e) if terminate_early => {
                ::tracing::error!("Error while replaying transaction {}: {:?}", tx_digest, e);
                bail!("Replay terminated due to error: {}", e);
            }
            Err(e) => {
                ::tracing::error!("Failed to replay transaction {}: {:?}", tx_digest, e);
            }
            Ok(_) => {
                ::tracing::info!("Successfully replayed transaction {}", tx_digest);
            }
        }
    }

    Ok(output_root_dir)
}
