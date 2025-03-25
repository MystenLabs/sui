// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod config;
pub mod direct;
pub mod json_rpc;

use std::collections::HashSet;
use std::time::Duration;

use anyhow::Result;
use clap::{value_parser, Parser, Subcommand};
use tracing::info;
use url::Url;

use crate::config::BenchmarkConfig;
use crate::direct::query_enricher::QueryEnricher;
use crate::direct::query_executor::QueryExecutor;
use crate::direct::query_template_generator::QueryTemplateGenerator;

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
            default_value = "postgres://postgres:postgres@localhost:5432/sui",
            value_parser = value_parser!(Url)
        )]
        db_url: Url,
        #[clap(long, default_value = "50")]
        concurrency: usize,
        #[clap(long, default_value = "30")]
        duration_secs: u64,
    },
    /// Benchmark JSON RPC endpoints
    #[clap(name = "jsonrpc")]
    JsonRpc {
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        endpoint: String,
        #[clap(long, default_value = "50")]
        concurrency: usize,
        #[clap(long)]
        duration_secs: Option<u64>,
        #[clap(long, default_value = "requests.jsonl")]
        requests_file: String,
        #[clap(long, value_delimiter = ',')]
        methods_to_skip: Vec<String>,
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
            concurrency,
            duration_secs,
        } => {
            info!("Running direct query benchmark against DB {}", db_url);

            let template_generator = QueryTemplateGenerator::new(db_url.clone());
            let query_templates = template_generator.generate_query_templates().await?;
            info!("Generated {} query templates", query_templates.len());

            let query_enricher = QueryEnricher::new(&db_url).await?;
            let enriched_queries = query_enricher.enrich_queries(query_templates).await?;
            info!(
                "Enriched {} queries with sample data",
                enriched_queries.len()
            );

            let config = BenchmarkConfig {
                concurrency,
                duration: Some(Duration::from_secs(duration_secs)),
                json_rpc_file_path: None,
                json_rpc_methods_to_skip: HashSet::new(),
            };
            let query_executor = QueryExecutor::new(&db_url, enriched_queries, config).await?;
            let result = query_executor.run().await?;

            info!("Total queries: {}", result.total_queries);
            info!("Total errors: {}", result.total_errors);
            info!("Average latency: {:.2}ms", result.avg_latency_ms);
            info!("Per-table statistics:");
            for stat in &result.table_stats {
                info!(
                    "  {:<30} queries: {:<8} errors: {:<8} avg latency: {:.2}ms",
                    stat.table_name, stat.queries, stat.errors, stat.avg_latency_ms
                );
            }
            Ok(())
        }
        Command::JsonRpc {
            endpoint,
            concurrency,
            duration_secs,
            requests_file,
            methods_to_skip,
        } => {
            info!("Running JSON RPC benchmark against {endpoint} with concurrency={concurrency} duration_secs={:?} requests_file={requests_file}", 
                duration_secs);
            json_rpc::run_benchmark(
                &endpoint,
                &requests_file,
                concurrency,
                duration_secs,
                methods_to_skip.into_iter().collect(),
            )
            .await?;
            Ok(())
        }
        Command::GraphQL { endpoint } => {
            info!("Running GraphQL benchmark against {}", endpoint);
            todo!()
        }
    }
}
