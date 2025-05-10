// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::path::PathBuf;

use crate::config::{ConnectionConfig, Ide, TxExecFullNodeConfig};

#[derive(Parser)]
#[command(
    name = "sui-graphql-rpc",
    about = "Sui GraphQL RPC",
    rename_all = "kebab-case",
    author,
    version
)]
pub enum Command {
    /// Output a TOML config (suitable for passing into the --config parameter of the start-server
    /// command) with all values set to their defaults.
    GenerateConfig {
        /// Optional path to an output file. Prints to `stdout` if not provided.
        output: Option<PathBuf>,
    },

    StartServer {
        #[command(flatten)]
        ide: Ide,

        #[command(flatten)]
        connection: ConnectionConfig,

        /// Path to TOML file containing configuration for service.
        #[arg(short, long)]
        config: Option<PathBuf>,

        #[command(flatten)]
        tx_exec_full_node: TxExecFullNodeConfig,
    },
}
