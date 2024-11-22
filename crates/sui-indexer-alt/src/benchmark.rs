// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, time::Instant};

use crate::{
    args::ConsistencyConfig,
    db::{reset_database, DbConfig},
    ingestion::IngestionConfig,
    pipeline::PipelineConfig,
    start_indexer, IndexerConfig,
};
use sui_synthetic_ingestion::synthetic_ingestion::read_ingestion_data;

#[derive(clap::Args, Debug, Clone)]
pub struct BenchmarkConfig {
    /// Path to the local ingestion directory to read checkpoints data from.
    #[arg(long)]
    ingestion_path: PathBuf,

    #[command(flatten)]
    pipeline_config: PipelineConfig,

    /// Only run the following pipelines. If not provided, all pipelines will be run.
    #[arg(long, action = clap::ArgAction::Append)]
    pipeline: Vec<String>,

    #[command(flatten)]
    consistency_config: ConsistencyConfig,
}

pub async fn run_benchmark(
    benchmark_config: BenchmarkConfig,
    db_config: DbConfig,
) -> anyhow::Result<()> {
    let BenchmarkConfig {
        ingestion_path,
        pipeline_config,
        pipeline,
        consistency_config,
    } = benchmark_config;

    let ingestion_data = read_ingestion_data(&ingestion_path).await?;
    let first_checkpoint = *ingestion_data.keys().next().unwrap();
    let last_checkpoint = *ingestion_data.keys().last().unwrap();
    let num_transactions: usize = ingestion_data.values().map(|c| c.transactions.len()).sum();

    reset_database(db_config.clone(), false /* do not skip migrations */).await?;

    let indexer_config = IndexerConfig {
        ingestion_config: IngestionConfig {
            remote_store_url: None,
            local_ingestion_path: Some(ingestion_path),
            checkpoint_buffer_size: IngestionConfig::DEFAULT_CHECKPOINT_BUFFER_SIZE,
            ingest_concurrency: IngestionConfig::DEFAULT_INGEST_CONCURRENCY,
            retry_interval_ms: IngestionConfig::DEFAULT_RETRY_INTERVAL_MS,
        },
        pipeline_config,
        first_checkpoint: Some(first_checkpoint),
        last_checkpoint: Some(last_checkpoint),
        pipeline,
        metrics_address: IndexerConfig::default_metrics_address(),
    };
    let cur_time = Instant::now();
    start_indexer(indexer_config, db_config, consistency_config, false).await?;
    let elapsed = Instant::now().duration_since(cur_time);
    println!("Indexed {} transactions in {:?}", num_transactions, elapsed);
    println!("TPS: {}", num_transactions as f64 / elapsed.as_secs_f64());
    Ok(())
}
