// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, time::Instant};

use crate::{BootstrapGenesis, config::IndexerConfig, setup_indexer};
use prometheus::Registry;
use sui_indexer_alt_framework::{
    IndexerArgs,
    ingestion::{ClientArgs, ingestion_client::IngestionClientArgs},
    postgres::{DbArgs, reset_database},
};
use sui_indexer_alt_schema::MIGRATIONS;
use sui_indexer_alt_schema::checkpoints::StoredGenesis;
use sui_indexer_alt_schema::epochs::StoredEpochStart;
use sui_synthetic_ingestion::synthetic_ingestion::read_ingestion_data;
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(clap::Args, Debug, Clone)]
pub struct BenchmarkArgs {
    /// Path to the local ingestion directory to read checkpoints data from.
    #[arg(long)]
    ingestion_path: PathBuf,

    /// Only run the following pipelines. If not provided, all pipelines found in the
    /// configuration file will be run.
    #[arg(long, action = clap::ArgAction::Append)]
    pipeline: Vec<String>,
}

pub async fn run_benchmark(
    database_url: Url,
    db_args: DbArgs,
    benchmark_args: BenchmarkArgs,
    indexer_config: IndexerConfig,
) -> anyhow::Result<()> {
    let BenchmarkArgs {
        ingestion_path,
        pipeline,
    } = benchmark_args;

    let ingestion_data = read_ingestion_data(&ingestion_path).await?;
    let first_checkpoint = *ingestion_data.keys().next().unwrap();
    let last_checkpoint = *ingestion_data.keys().last().unwrap();
    let num_transactions: usize = ingestion_data.values().map(|c| c.transactions.len()).sum();

    reset_database(database_url.clone(), db_args.clone(), Some(&MIGRATIONS)).await?;

    let indexer_args = IndexerArgs {
        first_checkpoint: Some(first_checkpoint),
        last_checkpoint: Some(last_checkpoint),
        pipeline,
        ..Default::default()
    };

    let client_args = ClientArgs {
        ingestion: IngestionClientArgs {
            local_ingestion_path: Some(ingestion_path.clone()),
            ..Default::default()
        },
        ..Default::default()
    };

    let cur_time = Instant::now();

    setup_indexer(
        database_url,
        db_args,
        indexer_args,
        client_args,
        indexer_config,
        Some(BootstrapGenesis {
            stored_genesis: StoredGenesis {
                genesis_digest: [0u8; 32].to_vec(),
                initial_protocol_version: 0,
            },
            stored_epoch_start: StoredEpochStart {
                epoch: 0,
                protocol_version: 0,
                cp_lo: 0,
                start_timestamp_ms: 0,
                reference_gas_price: 0,
                system_state: vec![],
            },
        }),
        &Registry::new(),
        CancellationToken::new(),
    )
    .await?
    .run()
    .await?
    .await?;

    let elapsed = Instant::now().duration_since(cur_time);
    println!("Indexed {} transactions in {:?}", num_transactions, elapsed);
    println!("TPS: {}", num_transactions as f64 / elapsed.as_secs_f64());
    Ok(())
}
