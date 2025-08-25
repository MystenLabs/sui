// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    artifacts::{Artifact, ArtifactManager},
    build::BuildCmdConfig,
    data_stores::{
        data_store::DataStore, file_system_store::FileSystemStore, in_memory_store::InMemoryStore,
        read_through_store::ReadThroughStore,
    },
    displays::Pretty,
    replay_interface::{ReadDataStore, SetupStore, StoreSummary},
    replay_txn::replay_transaction,
};
use anyhow::{anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use similar::{ChangeTag, TextDiff};
use std::{
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};
use sui_json_rpc_types::SuiTransactionBlockEffects;
use sui_types::{effects::TransactionEffects, supported_protocol_versions::Chain};

pub mod artifacts;
pub mod build;
#[path = "data-stores/mod.rs"]
pub mod data_stores;
pub mod displays;
pub mod execution;
pub mod replay_interface;
pub mod replay_txn;
pub mod tracing;

const DEFAULT_OUTPUT_DIR: &str = ".replay";
const MAINNET_GQL_URL: &str = "https://public-rpc.sui-mainnet.mystenlabs.com/graphql";
const TESTNET_GQL_URL: &str = "https://public-rpc.sui-testnet.mystenlabs.com/graphql";

// Arguments to the replay tool.
// It allows to replay a single transaction by digest or
// a file containing multiple digests, one per line.
// This may evolve to something very different in time and
// it's not meant to be stable.
// The options available are very convenient for the current
// development cycle.
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

/// Arguments for replay
#[derive(Parser, Clone, Debug)]
pub struct ReplayConfig {
    /// Transaction digest to replay.
    #[arg(long, short)]
    pub digest: Option<String>,
    /// File containing a list of digests, one per line.
    #[arg(long)]
    pub digests_path: Option<PathBuf>,
    /// RPC of the fullnode used to replay the transaction.
    #[arg(long, short, default_value = "mainnet")]
    pub node: Node,
    /// Whether to trace the transaction execution. Generated traces will be saved in the output
    /// directory (or `<cur_dir>/.replay/<digest>` if none provided).
    #[arg(long = "trace", default_value = "false")]
    pub trace: bool,
    /// The output directory for the replay artifacts. Defaults `<cur_dir>/.replay/<digest>`.
    #[arg(long, short)]
    pub output_dir: Option<PathBuf>,
    /// Terminate a batch replay early if an error occurs when replaying one of the transactions.
    #[arg(long, default_value = "false")]
    pub terminate_early: bool,
    /// Show transaction effects.
    #[arg(long, short, default_value = "false")]
    pub show_effects: bool,
    /// Whether existing artifacts that were generated from a previous replay of the transaction
    /// should be overwritten or an error raised if they already exist.
    #[arg(long, default_value = "false")]
    pub overwrite_existing: bool,
    /// Print a summary of data store usage after the replay completes.
    #[arg(long, short = 'v', default_value = "false")]
    pub verbose: bool,
    /// Select which data store mode to use.
    /// Options:
    /// - gql-only: remote GraphQL only
    /// - fs-then-gql: FileSystem primary with GraphQL fallback
    /// - fs-only: FileSystem only
    /// - inmem-fs-gql: InMemory -> FileSystem -> GraphQL (default)
    #[arg(long = "store-mode", value_enum, default_value_t = StoreMode::GqlOnly)]
    pub store_mode: StoreMode,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum StoreMode {
    #[value(name = "gql-only")]
    GqlOnly,
    #[value(name = "fs-then-gql")]
    FsThenGql,
    #[value(name = "fs-only")]
    FsOnly,
    #[value(name = "inmem-fs-gql")]
    InmemFsGql,
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

    pub fn rpc_url(&self) -> &str {
        match self {
            Node::Mainnet => MAINNET_GQL_URL,
            Node::Testnet => TESTNET_GQL_URL,
            // Node::Devnet => "",
            Node::Custom(url) => url.as_str(),
        }
    }

    pub fn node_dir(&self) -> &str {
        match self {
            Node::Mainnet => "mainnet",
            Node::Testnet => "testnet",
            // Node::Devnet => "devnet",
            // TODO: custom provides a URL which has to be translated to a valid directory name
            Node::Custom(_) => "custom",
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
        verbose,
        store_mode,
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

    // Build the selected data store and run replay
    match store_mode {
        StoreMode::GqlOnly => {
            let gql_store = DataStore::new(node.clone(), version)
                .map_err(|e| anyhow!("Failed to create data store: {:?}", e))?;
            run_replay(
                &gql_store,
                &output_root_dir,
                &digests,
                *overwrite_existing,
                *trace,
                *verbose,
                terminate_early,
            )
            .await?;
        }
        StoreMode::FsThenGql => {
            let fs_store = FileSystemStore::new(node.clone())
                .map_err(|e| anyhow!("Failed to create file system store: {:?}", e))?;
            let gql_store = DataStore::new(node.clone(), version)
                .map_err(|e| anyhow!("Failed to create data store: {:?}", e))?;
            let store = ReadThroughStore::new(fs_store, gql_store);
            run_replay(
                &store,
                &output_root_dir,
                &digests,
                *overwrite_existing,
                *trace,
                *verbose,
                terminate_early,
            )
            .await?;
        }
        StoreMode::FsOnly => {
            let fs_store = FileSystemStore::new(node.clone())
                .map_err(|e| anyhow!("Failed to create file system store: {:?}", e))?;
            run_replay(
                &fs_store,
                &output_root_dir,
                &digests,
                *overwrite_existing,
                *trace,
                *verbose,
                terminate_early,
            )
            .await?;
        }
        StoreMode::InmemFsGql => {
            let fs_store = FileSystemStore::new(node.clone())
                .map_err(|e| anyhow!("Failed to create file system store: {:?}", e))?;
            let gql_store = DataStore::new(node.clone(), version)
                .map_err(|e| anyhow!("Failed to create data store: {:?}", e))?;
            let secondary_store = ReadThroughStore::new(fs_store, gql_store);
            let in_memory_store = InMemoryStore::new(node.clone());
            let store = ReadThroughStore::new(in_memory_store, secondary_store);
            run_replay(
                &store,
                &output_root_dir,
                &digests,
                *overwrite_existing,
                *trace,
                *verbose,
                terminate_early,
            )
            .await?;
        }
    }

    Ok(output_root_dir)
}

async fn run_replay<S>(
    data_store: &S,
    output_root_dir: &Path,
    digests: &[String],
    overwrite_existing: bool,
    trace: bool,
    verbose: bool,
    terminate_early: bool,
) -> anyhow::Result<()>
where
    S: ReadDataStore + StoreSummary + SetupStore,
{
    data_store.setup(None)?;
    for tx_digest in digests {
        let tx_dir = output_root_dir.join(tx_digest);
        let artifact_manager = ArtifactManager::new(&tx_dir, overwrite_existing)?;
        match replay_transaction(&artifact_manager, tx_digest, data_store, trace).await {
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

    if verbose {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "\nData store summary:");
        if let Err(e) = data_store.summary(&mut out) {
            ::tracing::warn!("Failed to write data store summary: {:?}", e);
        }
    }
    Ok(())
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
