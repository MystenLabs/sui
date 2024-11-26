// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, time::Instant};

use crate::{
    db::{reset_database, DbArgs},
    ingestion::IngestionArgs,
    start_indexer, IndexerArgs, IndexerConfig,
};
use sui_synthetic_ingestion::synthetic_ingestion::read_ingestion_data;

#[derive(clap::Args, Debug, Clone)]
pub struct BenchmarkArgs {
    /// Path to the local ingestion directory to read checkpoints data from.
    #[arg(long)]
    ingestion_path: PathBuf,
}

pub async fn run_benchmark(
    db_args: DbArgs,
    benchmark_args: BenchmarkArgs,
    indexer_config: IndexerConfig,
) -> anyhow::Result<()> {
    let BenchmarkArgs { ingestion_path } = benchmark_args;

    let ingestion_data = read_ingestion_data(&ingestion_path).await?;
    let first_checkpoint = *ingestion_data.keys().next().unwrap();
    let last_checkpoint = *ingestion_data.keys().last().unwrap();
    let num_transactions: usize = ingestion_data.values().map(|c| c.transactions.len()).sum();

    reset_database(db_args.clone(), false /* do not skip migrations */).await?;

    let indexer_args = IndexerArgs {
        first_checkpoint: Some(first_checkpoint),
        last_checkpoint: Some(last_checkpoint),
        ..Default::default()
    };

    let ingestion_args = IngestionArgs {
        remote_store_url: None,
        local_ingestion_path: Some(ingestion_path.clone()),
    };

    let cur_time = Instant::now();

    start_indexer(
        db_args,
        indexer_args,
        ingestion_args,
        indexer_config,
        false, /* with_genesis */
    )
    .await?;

    let elapsed = Instant::now().duration_since(cur_time);
    println!("Indexed {} transactions in {:?}", num_transactions, elapsed);
    println!("TPS: {}", num_transactions as f64 / elapsed.as_secs_f64());
    Ok(())
}
