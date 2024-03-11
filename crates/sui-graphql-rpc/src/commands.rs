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
    GenerateConfig {
        /// Path to output the YAML config, otherwise stdout.
        #[clap(short, long)]
        path: Option<PathBuf>,
    },
    GenerateDocsExamples,
    GenerateSchema {
        /// Path to output GraphQL schema to, in SDL format.
        #[clap(short, long)]
        file: Option<PathBuf>,
    },
    GenerateExamples {
        /// Path to output examples docs.
        #[clap(short, long)]
        file: Option<PathBuf>,
    },
    FromConfig {
        /// Path to TOML file containing configuration for server.
        #[clap(short, long)]
        path: PathBuf,
    },
    StartServer {
        /// The title to display at the top of the page
        #[clap(short, long)]
        ide_title: Option<String>,
        /// DB URL for data fetching
        #[clap(short, long)]
        db_url: Option<String>,
        /// Port to bind the server to
        #[clap(short, long)]
        port: Option<u16>,
        /// Host to bind the server to
        #[clap(long)]
        host: Option<String>,
        /// Port to bind the prom server to
        #[clap(long)]
        prom_port: Option<u16>,
        /// Host to bind the prom server to
        #[clap(long)]
        prom_host: Option<String>,

        /// Path to TOML file containing configuration for service.
        #[clap(short, long)]
        config: Option<PathBuf>,

        /// RPC url to the Node for tx execution
        #[clap(long)]
        node_rpc_url: Option<String>,
    },
}
