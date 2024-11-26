// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod direct;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::direct::query_executor::QueryExecutor;
use crate::direct::query_generator::QueryGenerator;

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
        #[clap(
            long,
            default_value = "postgres://postgres:postgres@localhost:5432/sui"
        )]
        db_url: String,
        #[clap(
            long,
            default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../sui-indexer-alt/migrations")
        )]
        migration_path: PathBuf,
    },
    /// Benchmark JSON RPC endpoints
    #[clap(name = "jsonrpc")]
    JsonRpc {
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        endpoint: String,
    },
    /// Benchmark GraphQL queries
    #[clap(name = "graphql")]
    GraphQL {
        #[clap(long, default_value = "http://127.0.0.1:9000/graphql")]
        endpoint: String,
    },
}

pub async fn run_benchmarks() -> Result<(), anyhow::Error> {
    let opts: Opts = Opts::parse();

    match opts.command {
        Command::DirectQuery {
            db_url,
            migration_path,
        } => {
            println!(
                "Running direct query benchmark against DB {} and migrations {}",
                db_url,
                migration_path.display()
            );
            let query_generator = QueryGenerator { migration_path };
            let benchmark_queries = query_generator.generate_benchmark_queries()?;
            println!("Generated {} benchmark queries", benchmark_queries.len());
            let query_executor = QueryExecutor::new(db_url.as_str(), benchmark_queries).await?;
            query_executor.run().await?;
            Ok(())
        }
        Command::JsonRpc { endpoint } => {
            println!("Running JSON RPC benchmark against {}", endpoint);
            todo!()
        }
        Command::GraphQL { endpoint } => {
            println!("Running GraphQL benchmark against {}", endpoint);
            todo!()
        }
    }
}
