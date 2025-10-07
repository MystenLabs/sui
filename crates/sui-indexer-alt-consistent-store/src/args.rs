// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::ArgAction;
use sui_indexer_alt_framework::{ingestion::ClientArgs, IndexerArgs};
use sui_indexer_alt_metrics::MetricsArgs;

use crate::restore::{
    formal_snapshot::FormalSnapshotArgs, storage::StorageConnectionArgs, RestoreArgs,
};
pub use crate::rpc::{RpcArgs, TlsArgs};

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Run the Indexer and RPC.
    Run {
        /// The path where the RocksDB database will be stored. The database will be created if it
        /// does not exist.
        #[arg(long)]
        database_path: PathBuf,

        #[clap(flatten)]
        indexer_args: IndexerArgs,

        #[clap(flatten)]
        client_args: ClientArgs,

        #[clap(flatten)]
        rpc_args: RpcArgs,

        #[clap(flatten)]
        metrics_args: MetricsArgs,

        /// Path to the RPC's configuration TOML file. If one is not provided, the default values for
        /// the configuration will be set.
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Restore the database from a formal snapshot.
    Restore {
        /// The path where the RocksDB database will be stored. The database will be created if it
        /// does not exist.
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

        /// The pipelines to restore.
        #[arg(long, action = ArgAction::Append)]
        pipeline: Vec<String>,

        /// Path to the RPC's configuration TOML file. If one is not provided, the default values for
        /// the configuration will be set. Only the `rocksdb` section is read for restoration.
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Output the contents of the default configuration to STDOUT.
    GenerateConfig,
}
