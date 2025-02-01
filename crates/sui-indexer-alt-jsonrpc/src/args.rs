// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_indexer_alt_metrics::MetricsArgs;
use sui_pg_db::DbArgs;

use crate::{data::system_package_task::SystemPackageTaskArgs, RpcArgs};

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(flatten)]
    pub db_args: DbArgs,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Run the RPC service.
    Rpc {
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
    },

    /// Output the contents of the default configuration to STDOUT.
    GenerateConfig,
}
