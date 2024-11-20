// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod direct;

use clap::{Parser, Subcommand};
use anyhow::Result;

#[derive(Parser)]
#[clap(
    name = "sui-rpc-benchmark",
    about = "Benchmark tool for comparing Sui RPC access methods"
)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Benchmark direct database queries
    #[clap(name = "direct")]
    DirectQuery {
        #[clap(long, default_value = "100")]
        num_queries: u64,
        #[clap(long, default_value = "1")]
        num_threads: usize,
    },

    /// Benchmark JSON RPC endpoints
    #[clap(name = "jsonrpc")]
    JsonRpc {
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        endpoint: String,
        #[clap(long, default_value = "100")] 
        num_queries: u64,
        #[clap(long, default_value = "1")]
        num_threads: usize,
    },

    /// Benchmark GraphQL queries
    #[clap(name = "graphql")]
    GraphQL {
        #[clap(long, default_value = "http://127.0.0.1:9000/graphql")]
        endpoint: String,
        #[clap(long, default_value = "100")]
        num_queries: u64,
        #[clap(long, default_value = "1")]
        num_threads: usize,
    },
}

pub fn run_benchmarks() -> Result<()> {
    let opts: Opts = Opts::parse();

    match opts.command {
        Command::DirectQuery { num_queries, num_threads } => {
            println!("Running direct query benchmark with {} queries and {} threads", num_queries, num_threads);
            Ok(())
        }
        Command::JsonRpc { endpoint, num_queries, num_threads } => {
            println!("Running JSON RPC benchmark against {} with {} queries and {} threads", endpoint, num_queries, num_threads);
            // TODO: Implement JSON RPC benchmark
            Ok(())
        }
        Command::GraphQL { endpoint, num_queries, num_threads } => {
            println!("Running GraphQL benchmark against {} with {} queries and {} threads", endpoint, num_queries, num_threads);
            // TODO: Implement GraphQL benchmark
            Ok(())
        }
    }
}

