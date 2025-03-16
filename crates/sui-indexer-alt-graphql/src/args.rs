// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_metrics::MetricsArgs;

use crate::RpcArgs;

#[derive(clap::Parser, Debug, Clone)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    Rpc {
        #[command(flatten)]
        rpc_args: RpcArgs,

        #[command(flatten)]
        metrics_args: MetricsArgs,
    },
}
