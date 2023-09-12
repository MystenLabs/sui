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
        #[clap(long)]
        host: Option<String>,

        /// Maximum depth of query
        #[clap(long)]
        max_query_depth: Option<usize>,
    },
}
