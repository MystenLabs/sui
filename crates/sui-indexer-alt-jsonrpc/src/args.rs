// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_indexer_alt_metrics::MetricsArgs;
use sui_pg_db::DbArgs;
use url::Url;

use crate::{NodeArgs, RpcArgs};

pub use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;

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
        rpc_args: RpcArgs,

        #[command(flatten)]
        system_package_task_args: SystemPackageTaskArgs,

        #[command(flatten)]
        metrics_args: MetricsArgs,

        #[command(flatten)]
        node_args: NodeArgs,

        /// Path to the RPC's configuration TOML file. If one is not provided, the default values for
        /// the configuration will be set.
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Output the contents of the default configuration to STDOUT.
    GenerateConfig,
}
