// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::path::PathBuf;

#[derive(Parser)]
#[clap(
    name = "sui-graphql-rpc",
    about = "Sui GraphQL RPC",
    rename_all = "kebab-case",
    author,
    version
)]
pub enum Command {
    GenerateSchema {
        /// Path to output GraphQL schema to, in SDL format.
        #[clap(short, long)]
        file: Option<PathBuf>,
    },
    StartServer {
        /// URL of the RPC server for data fetching
        #[clap(short, long)]
        rpc_url: Option<String>,
        /// Port to bind the server to
        #[clap(short, long)]
        port: Option<u16>,
        /// Host to bind the server to
        #[clap(long)]
        host: Option<String>,
        /// Path to TOML file containing configuration for service.
        #[clap(short, long)]
        config: Option<PathBuf>,
    },
}
