// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_consistent_store::restore::StorageConnectionArgs;
use sui_consistent_store::restore::formal_snapshot::FormalSnapshotArgs;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_metrics::MetricsArgs;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Run the rpc-store indexer and HTTP RPC service. Every
    /// pipeline configured in the [`crate::config::ServiceConfig`]
    /// is enabled (raw chain data and derived indexes). The indexer
    /// resumes from each pipeline's persisted `__watermark`; on a
    /// fresh database with no prior restore that floor is genesis
    /// (checkpoint 0).
    Run {
        /// The path where the RocksDB database lives. The database
        /// is created if it does not yet exist.
        #[arg(long)]
        database_path: PathBuf,

        #[clap(flatten)]
        indexer_args: IndexerArgs,

        #[clap(flatten)]
        client_args: ClientArgs,

        #[clap(flatten)]
        metrics_args: MetricsArgs,

        /// Path to the service's TOML configuration file. If
        /// omitted, the defaults from
        /// [`crate::config::ServiceConfig::default`] are used.
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Restore the rpc-store from a Sui formal snapshot. The
    /// derived-index pipelines (and the raw `objects` CF) are
    /// rebuilt from the snapshot's live-object set. Any pipeline
    /// that can't be sourced from the snapshot (raw chain data
    /// CFs, bitmap CFs) has its `__watermark` floored to the
    /// snapshot's anchor checkpoint so tip indexing resumes from
    /// `target_checkpoint + 1` instead of replaying from genesis.
    Restore {
        /// The path where the RocksDB database lives. The database
        /// is created if it does not yet exist.
        #[arg(long)]
        database_path: PathBuf,

        #[clap(flatten)]
        formal_snapshot_args: FormalSnapshotArgs,

        #[clap(flatten)]
        storage_connection_args: StorageConnectionArgs,

        #[clap(flatten)]
        restore_args: RestoreArgs,

        #[clap(flatten)]
        metrics_args: MetricsArgs,

        /// Path to the service's TOML configuration file. If
        /// omitted, the defaults from
        /// [`crate::config::ServiceConfig::default`] are used.
        /// Only the `db` and `restore` sections are consulted during
        /// a restore.
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Serve the gRPC / HTTP RPC over an existing rpc-store database
    /// without running the indexer. No ingestion source is required:
    /// the database is opened and queried exactly as it is on disk,
    /// and nothing advances the watermarks while the server runs.
    /// Useful for inspecting a freshly restored or previously
    /// indexed database.
    Serve {
        /// The path where the RocksDB database lives. Unlike `run`,
        /// the database must already exist.
        #[arg(long)]
        database_path: PathBuf,

        #[clap(flatten)]
        metrics_args: MetricsArgs,

        /// Path to the service's TOML configuration file. If
        /// omitted, the defaults from
        /// [`crate::config::ServiceConfig::default`] are used. Only
        /// the `db`, `consistency`, and `rpc` sections are consulted
        /// when serving.
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Print the default service configuration to STDOUT in TOML
    /// form.
    GenerateConfig,
}

/// Knobs for the restore driver itself. Distinct from
/// [`FormalSnapshotArgs`] (which says where to fetch from) and
/// [`StorageConnectionArgs`] (which says how to connect).
#[derive(clap::Args, Clone, Debug, Default)]
pub struct RestoreArgs {
    /// Number of snapshot partitions fetched concurrently during a
    /// restore. Overrides `restore.shard_concurrency` from the
    /// config file; when omitted, the config value (default 8) is
    /// used.
    #[arg(long)]
    pub shard_concurrency: Option<usize>,
}
