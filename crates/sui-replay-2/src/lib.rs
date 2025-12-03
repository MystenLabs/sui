// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    artifacts::{Artifact, ArtifactManager},
    displays::Pretty,
    replay_txn::replay_transaction,
    summary_metrics::TotalMetrics,
};
use anyhow::{Result, anyhow, bail};
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use similar::{ChangeTag, TextDiff};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use sui_config::sui_config_dir;
use sui_data_store::{
    Node, ReadDataStore, SetupStore, StoreSummary,
    stores::{DataStore, FileSystemStore, InMemoryStore, ReadThroughStore},
};
use sui_json_rpc_types::SuiTransactionBlockEffects;
use sui_types::effects::TransactionEffects;
// Disambiguate external tracing crate from local `crate::tracing` module using absolute path.
use ::tracing::{Instrument, debug, error, info, info_span, warn};

pub mod artifacts;
pub mod displays;
pub mod execution;
pub mod package_tools;
pub mod replay_txn;
pub mod summary_metrics;
pub mod tracing;

const DEFAULT_OUTPUT_DIR: &str = ".replay";
const CONFIG_FILE_NAME: &str = "replay.toml";

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
    pub command: Option<Command>,
    #[command(flatten)]
    pub replay_stable: ReplayConfigStable,
    #[command(flatten)]
    pub replay_experimental: ReplayConfigExperimental,
}

/// Subcommands for the replay tool
#[derive(Parser, Clone, Debug)]
pub enum Command {
    /// Rebuild a package from cache and source
    RebuildPackage {
        /// Package ID to rebuild
        #[arg(long = "pkg-id")]
        package_id: String,

        /// Path to package source directory
        #[arg(long = "pkg-src")]
        package_source: PathBuf,

        /// Output path for rebuilt package binary. If not specified, replaces the package in cache
        #[arg(short = 'o', long = "output")]
        output_path: Option<PathBuf>,

        /// RPC of the fullnode used to fetch the package
        #[arg(short = 'n', long = "node", default_value = "mainnet")]
        node: Node,
    },

    /// Extract a package from cache to a file
    ExtractPackage {
        /// Package ID to extract
        #[arg(long = "pkg-id")]
        package_id: String,

        /// Output path for extracted package binary
        #[arg(short = 'o', long = "output")]
        output_path: PathBuf,

        /// RPC of the fullnode cache to extract from
        #[arg(short = 'n', long = "node", default_value = "mainnet")]
        node: Node,
    },

    /// Overwrite a package in cache with a provided package file
    OverwritePackage {
        /// Package ID to overwrite
        #[arg(long = "pkg-id")]
        package_id: String,

        /// Path to the package file to write
        #[arg(long = "pkg-path")]
        package_path: PathBuf,

        /// RPC of the fullnode cache to write to
        #[arg(short = 'n', long = "node", default_value = "mainnet")]
        node: Node,
    },
}

/// Arguments for replay (used for both CLI and config file)
#[derive(Parser, Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ReplayConfigStable {
    /// Transaction digest to replay.
    #[arg(long = "digest", short)]
    pub digest: Option<String>,

    /// File containing a list of digests, one per line.
    #[arg(long = "digests-path")]
    pub digests_path: Option<PathBuf>,

    /// Terminate a batch replay early if an error occurs when replaying one of the transactions.
    #[arg(long = "terminate-early", num_args = 0, default_missing_value = "true")]
    pub terminate_early: Option<bool>,

    /// Whether to trace the transaction execution. Generated traces will be saved in the output
    /// directory (or `<cur_dir>/.replay/<digest>` if none provided).
    #[arg(long = "trace", num_args = 0, default_missing_value = "true")]
    pub trace: Option<bool>,

    /// The output directory for the replay artifacts. Defaults `<cur_dir>/.replay/<digest>`.
    #[arg(long = "output-dir", short)]
    pub output_dir: Option<PathBuf>,

    /// Show transaction effects.
    #[arg(short = 'e', long = "show-effects", num_args = 1)]
    pub show_effects: Option<bool>,

    /// Whether existing artifacts that were generated from a previous replay of the transaction
    /// should be overwritten or an error raised if they already exist.
    #[arg(long = "overwrite", num_args = 0, default_missing_value = "true")]
    pub overwrite: Option<bool>,
}

/// Arguments for replay used for internal processing
/// (same as ReplayConfigStable but with default values set)
#[derive(Parser, Clone, Debug)]
pub struct ReplayConfigStableInternal {
    pub digest: Option<String>,
    pub digests_path: Option<PathBuf>,
    pub terminate_early: bool,
    pub trace: bool,
    pub output_dir: Option<PathBuf>,
    pub show_effects: bool,
    pub overwrite: bool,
}

impl Default for ReplayConfigStableInternal {
    /// Theser represent default values for the flags specified on the command line.
    /// They need to be specified explicitly as we can't provide them in as Clap
    /// annotations - if we did, we'd loose the ability to detect which command line
    /// arguments are missing.
    fn default() -> Self {
        Self {
            digest: None,
            digests_path: None,
            terminate_early: false,
            trace: false,
            output_dir: None,
            show_effects: true,
            overwrite: false,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct TOMLConfig {
    flags: Option<ReplayConfigStable>,
}

#[derive(Parser, Clone, Debug)]
pub struct ReplayConfigExperimental {
    /// RPC of the fullnode used to replay the transaction.
    #[arg(long, short, default_value = "mainnet")]
    pub node: Node,

    /// Print a summary of data store usage after the replay completes.
    #[arg(long, short = 'v', default_value = "false")]
    pub verbose: bool,

    /// Select which data store mode to use.
    /// Options:
    /// - gql-only: remote GraphQL only
    /// - fs-then-gql: FileSystem primary with GraphQL fallback
    /// - fs-only: FileSystem only
    /// - inmem-fs: InMemory -> FileSystem
    /// - inmem-fs-gql: InMemory -> FileSystem -> GraphQL (default)
    #[arg(long = "store-mode", value_enum, default_value_t = StoreMode::GqlOnly)]
    pub store_mode: StoreMode,

    /// Include execution and total time in transaction output.
    #[arg(long = "track-time", default_value = "false")]
    pub track_time: bool,

    /// Cache executors across transactions within the same epoch.
    #[arg(long = "cache-executor", default_value = "false")]
    pub cache_executor: bool,
}

impl Default for ReplayConfigExperimental {
    fn default() -> Self {
        Self {
            node: Node::Mainnet,
            verbose: false,
            store_mode: StoreMode::GqlOnly,
            track_time: false,
            cache_executor: false,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum StoreMode {
    #[value(name = "gql-only")]
    GqlOnly,
    #[value(name = "fs-then-gql")]
    FsThenGql,
    #[value(name = "fs-only")]
    FsOnly,
    #[value(name = "inmem-fs")]
    InmemFs,
    #[value(name = "inmem-fs-gql")]
    InmemFsGql,
}

/// Load replay configuration from ~/.sui/sui_config/replay.toml file.
/// Returns default config (all fields set to None) if file cannot be found or read.
pub fn load_config_file() -> Result<ReplayConfigStable> {
    let config_dir = match sui_config_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("Cannot locate replay config file: {e}");
            return Ok(ReplayConfigStable::default());
        }
    };
    let config_file_path = config_dir.join(CONFIG_FILE_NAME);

    if !config_file_path.exists() {
        return Ok(ReplayConfigStable::default());
    }

    let content = fs::read_to_string(&config_file_path).map_err(|e| {
        anyhow!(
            "Failed to read replay config file '{:?}': {}",
            config_file_path,
            e
        )
    })?;

    let config: TOMLConfig = toml::from_str(&content).map_err(|e| {
        anyhow!(
            "Failed to parse replay config file '{:?}': {}",
            config_file_path,
            e
        )
    })?;

    config.flags.ok_or_else(|| {
        anyhow!(
            "No flags section found in replay config file '{:?}'",
            config_file_path
        )
    })
}

/// Merge CLI flags and config file flags into a single config. CLI flags take
/// precedence over config file flags, which take precedence over defaults.
pub fn merge_configs(
    cli_config: ReplayConfigStable,
    file_config: ReplayConfigStable,
) -> ReplayConfigStableInternal {
    let default_config = ReplayConfigStableInternal::default();
    ReplayConfigStableInternal {
        digest: cli_config.digest.or(file_config.digest),

        digests_path: cli_config.digests_path.or(file_config.digests_path),

        terminate_early: cli_config
            .terminate_early
            .or(file_config.terminate_early)
            .unwrap_or(default_config.terminate_early),

        trace: cli_config
            .trace
            .or(file_config.trace)
            .unwrap_or(default_config.trace),

        output_dir: cli_config.output_dir.or(file_config.output_dir),

        show_effects: cli_config
            .show_effects
            .or(file_config.show_effects)
            .unwrap_or(default_config.show_effects),

        overwrite: cli_config
            .overwrite
            .or(file_config.overwrite)
            .unwrap_or(default_config.overwrite),
    }
}

pub async fn handle_replay_config(
    stable_config: &ReplayConfigStableInternal,
    experimental_config: &ReplayConfigExperimental,
    version: &str,
) -> Result<PathBuf> {
    let ReplayConfigStableInternal {
        digest,
        digests_path,
        terminate_early,
        trace,
        output_dir,
        show_effects: _, // used in the caller
        overwrite: overwrite_existing,
    } = &stable_config;
    let mut terminate_early = *terminate_early;

    let ReplayConfigExperimental {
        node,
        verbose,
        store_mode,
        track_time,
        cache_executor,
    } = experimental_config;

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

    debug!("Binary version: {version}");

    // Build the selected data store and run replay
    match store_mode {
        StoreMode::GqlOnly => {
            let gql_store = DataStore::new(node.clone(), version)
                .map_err(|e| anyhow!("Failed to create data store: {:?}", e))?;
            run_replay(
                &gql_store,
                &output_root_dir,
                &digests,
                node,
                *overwrite_existing,
                *trace,
                *verbose,
                terminate_early,
                *track_time,
                *cache_executor,
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
                node,
                *overwrite_existing,
                *trace,
                *verbose,
                terminate_early,
                *track_time,
                *cache_executor,
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
                node,
                *overwrite_existing,
                *trace,
                *verbose,
                terminate_early,
                *track_time,
                *cache_executor,
            )
            .await?;
        }
        StoreMode::InmemFs => {
            let fs_store = FileSystemStore::new(node.clone())
                .map_err(|e| anyhow!("Failed to create file system store: {:?}", e))?;
            let in_memory_store = InMemoryStore::new(node.clone());
            let store = ReadThroughStore::new(in_memory_store, fs_store);
            run_replay(
                &store,
                &output_root_dir,
                &digests,
                node,
                *overwrite_existing,
                *trace,
                *verbose,
                terminate_early,
                *track_time,
                *cache_executor,
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
                node,
                *overwrite_existing,
                *trace,
                *verbose,
                terminate_early,
                *track_time,
                *cache_executor,
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
    node: &Node,
    overwrite_existing: bool,
    trace: bool,
    verbose: bool,
    terminate_early: bool,
    track_time: bool,
    cache_executor: bool,
) -> Result<()>
where
    S: ReadDataStore + StoreSummary + SetupStore,
{
    use crate::replay_txn::ExecutorProvider;
    use std::time::Instant;

    data_store.setup(None)?;
    let mut total_metrics = TotalMetrics::new();
    let mut executor_provider = ExecutorProvider::new(cache_executor);

    for tx_digest in digests {
        let tx_dir = output_root_dir.join(tx_digest);
        let artifact_manager = ArtifactManager::new(&tx_dir, overwrite_existing)?;
        let span = info_span!("replay", tx_digest = %tx_digest);

        let tx_start = Instant::now();
        let result = replay_transaction(
            &artifact_manager,
            tx_digest,
            data_store,
            node.network_name(),
            trace,
            &mut executor_provider,
        )
        .instrument(span)
        .await;
        let tx_total_ms = tx_start.elapsed().as_millis();

        let success = result.is_ok();
        let exec_ms = result.as_ref().ok().copied().unwrap_or(0);

        total_metrics.add_transaction(success, tx_total_ms, exec_ms);

        // Print per-transaction result
        let status = if success { "OK" } else { "FAILED" };
        if track_time {
            println!(
                "> Replayed txn {} ({}): exec_ms={}, total_ms={}",
                tx_digest, status, exec_ms, tx_total_ms
            );
        } else {
            println!("> Replayed txn {} ({})", tx_digest, status);
        }

        match result {
            Err(e) if terminate_early => {
                error!(tx_digest = %tx_digest, error = ?e, "Replay error; terminating early");
                bail!("Replay terminated due to error: {}", e);
            }
            Err(e) => {
                error!(tx_digest = %tx_digest, error = ?e, "Replay failed");
            }
            Ok(_) => {
                info!(tx_digest = %tx_digest, "Replay succeeded");
            }
        }
    }

    if verbose {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "\nData store summary:");
        if let Err(e) = data_store.summary(&mut out) {
            warn!("Failed to write data store summary: {:?}", e);
        }
    }

    if digests.len() > 1 {
        println!(
            "Replay run: tx_count={} success={} failure={} - exec_ms={}, total_ms={}",
            total_metrics.tx_count,
            total_metrics.success_count,
            total_metrics.failure_count,
            total_metrics.exec_ms,
            total_metrics.total_ms
        );
    }

    Ok(())
}

pub fn print_effects_or_fork<W: Write>(
    digest: &str,
    output_root: &Path,
    show_effects: bool,
    w: &mut W,
) -> Result<()> {
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
                .map_err(|e| anyhow!("Failed to convert effects: {e}"))?
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
