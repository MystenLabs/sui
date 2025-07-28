// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::artifacts::{Artifact, ArtifactManager};
use crate::build::BuildCmdConfig;
use crate::data_store::DataStore;
use crate::displays::Pretty;
use crate::replay_txn::replay_transaction;
use anyhow::{anyhow, bail};
use clap::{Parser, Subcommand};
use similar::{ChangeTag, TextDiff};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use sui_json_rpc_types::SuiTransactionBlockEffects;
use sui_types::effects::TransactionEffects;
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
    /// Whether to trace the transaction execution. Generated traces will be saved in the output
    /// directory (or `<cur_dir>/.replay/<digest>` if none provided).
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
    /// Whether existing artifacts that were generated from a previous replay of the transaction
    /// should be overwritten or an error raised if they already exist.
    #[arg(long, default_value = "false")]
    pub overwrite_existing: bool,
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

pub async fn handle_replay_config(config: &ReplayConfig, version: &str) -> anyhow::Result<PathBuf> {
    let ReplayConfig {
        node,
        digest,
        digests_path,
        trace,
        mut terminate_early,
        output_dir,
        show_effects: _,
        overwrite_existing,
    } = config;

    let output_root_dir = if let Some(dir) = output_dir {
        dir.to_path_buf()
    } else {
        // Default output directory is `<cur_dir>/.replay/<digest>`
        let current_dir =
            std::env::current_dir().map_err(|e| anyhow!("Failed to get current directory: {e}"))?;
        current_dir.join(DEFAULT_OUTPUT_DIR)
    };

    // If trying to trace but the binary was not built with the tracing feature flag raise an error.
    #[cfg(not(feature = "tracing"))]
    if *trace {
        bail!(
            "Tracing is not enabled in this build. Please rebuild with the \
            `tracing` feature (`--features tracing`) to use tracing in replay"
        );
    }

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
        vec![tx_digest.clone()]
    } else {
        bail!("either --digest or --digests-path must be provided");
    };

    ::tracing::debug!("Binary version: {version}");

    // `DataStore` implements `TransactionStore`, `EpochStore` and `ObjectStore`
    let data_store = DataStore::new(node.clone(), version)
        .map_err(|e| anyhow!("Failed to create data store: {:?}", e))?;

    // load and replay transactions
    for tx_digest in digests {
        let tx_dir = output_root_dir.join(&tx_digest);
        let artifact_manager =
            ArtifactManager::new(&tx_dir, *overwrite_existing /* overrides_allowed */)?;
        match replay_transaction(&artifact_manager, &tx_digest, &data_store, *trace).await {
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

pub fn print_effects_or_fork<W: Write>(
    digest: &str,
    output_root: &Path,
    show_effects: bool,
    w: &mut W,
) -> anyhow::Result<()> {
    let output_dir = output_root.join(digest);
    let manager = ArtifactManager::new(&output_dir, false)?;
    if manager.member(Artifact::ForkedTransactionEffects).exists() {
        writeln!(w, "Transaction {digest} forked")?;
        let forked_effects = manager
            .member(Artifact::ForkedTransactionEffects)
            .try_get_transaction_effects()
            .transpose()?
            .unwrap();
        let expected_effects = manager
            .member(Artifact::TransactionEffects)
            .try_get_transaction_effects()
            .transpose()?
            .unwrap();
        writeln!(
            w,
            "Forked Transaction Effects for {digest}\n{}",
            diff_effects(&expected_effects, &forked_effects)
        )?;
    } else if show_effects {
        let tx_effects = manager
            .member(Artifact::TransactionEffects)
            .try_get_transaction_effects()
            .transpose()?
            .unwrap();
        writeln!(
            w,
            "{}",
            SuiTransactionBlockEffects::try_from(tx_effects.clone())
                .map_err(|e| anyhow::anyhow!("Failed to convert effects: {e}"))?
        )?;
        manager
            .member(Artifact::TransactionGasReport)
            .try_get_gas_report()
            .transpose()?
            .map(|report| {
                writeln!(
                    w,
                    "Transaction Gas Report for {digest}\n{}",
                    Pretty(&report)
                )
                .unwrap()
            })
            .unwrap_or_else(|| {
                writeln!(w, "No gas report available for transaction {digest}").unwrap();
            });
    }
    Ok(())
}

/// Utility to diff `TransactionEffect` in a human readable format
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
