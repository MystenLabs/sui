// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_indexer_alt_metrics::MetricsArgs;
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_indexer_alt_reader::pg_reader::db::DbArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;
use url::Url;

use crate::RpcArgs;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Run the RPC service.
    Rpc {
        /// The URL of the database to connect to.
        #[clap(
            long,
            default_value = "postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt"
        )]
        database_url: Url,

        /// Bigtable instance ID to make KV store requests to. If this is not provided, KV store
        /// requests will be made to the database.
        #[clap(long)]
        bigtable_instance: Option<String>,

        #[command(flatten)]
        db_args: DbArgs,

        #[command(flatten)]
        bigtable_args: BigtableArgs,

        #[command(flatten)]
        rpc_args: RpcArgs,

        #[command(flatten)]
        system_package_task_args: SystemPackageTaskArgs,

        #[command(flatten)]
        metrics_args: MetricsArgs,

        /// Path to the RPC's configuration TOML file. If one is not provided, the default values for
        /// the configuration will be set.
        #[arg(long)]
        config: Option<PathBuf>,

        /// Path to indexer configuration TOML files (multiple can be supplied). These are used to
        /// identify the pipelines that the RPC will monitor for watermark purposes.
        #[arg(long, action = clap::ArgAction::Append)]
        indexer_config: Vec<PathBuf>,
    },

    /// Output the contents of the default configuration to STDOUT.
    GenerateConfig,
}
