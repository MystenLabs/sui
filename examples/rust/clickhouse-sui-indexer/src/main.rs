// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod handlers;
mod store;

use anyhow::Result;
use clap::Parser;
use sui_indexer_alt_framework::{
    ingestion::{ClientArgs, IngestionConfig},
    pipeline::concurrent::ConcurrentConfig,
    Indexer, IndexerArgs,
};
use url::Url;

use handlers::TxDigests;
use store::ClickHouseStore;

#[derive(clap::Parser, Debug, Clone)]
struct Args {
    #[clap(flatten)]
    pub indexer_args: IndexerArgs,

    #[clap(flatten)]
    pub client_args: ClientArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize crypto provider for HTTPS connections (needed for remote checkpoint fetching)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install crypto provider");

    // Parse command-line arguments
    let args = Args::parse();

    // ClickHouse connection (uses 'dev' user by default for local development)
    let clickhouse_url = "http://localhost:8123".parse::<Url>()?;

    println!("Connecting to ClickHouse at: {}", clickhouse_url);

    // Create our custom ClickHouse store
    let store = ClickHouseStore::new(clickhouse_url);

    // Ensure the database tables are created before starting the indexer
    store.create_tables_if_not_exists().await?;

    // Manually build the indexer with our custom ClickHouse store
    // This is the key difference from basic-sui-indexer which uses IndexerCluster::builder()
    let mut indexer = Indexer::new(
        store.clone(),
        args.indexer_args,
        args.client_args,
        IngestionConfig::default(),
        None,                // No metrics prefix
        &Default::default(), // Empty prometheus registry
        tokio_util::sync::CancellationToken::new(),
    )
    .await?;

    // Register our concurrent pipeline handler (better for testing pruning)
    // This processes checkpoints with separate reader and pruner components
    indexer
        .concurrent_pipeline(
            TxDigests,
            // ConcurrentConfig default comes with no pruning.
            ConcurrentConfig::default(),
        )
        .await?;

    println!("Starting ClickHouse Sui indexer...");

    // Start the indexer and wait for it to complete
    let handle = indexer.run().await?;

    // This will run until the indexer is stopped (e.g., by Ctrl+C)
    handle.await?;

    println!("Indexer stopped");
    Ok(())
}
