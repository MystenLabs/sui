// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod store;
mod handlers;

use anyhow::Result;
use clap::Parser;
use sui_indexer_alt_framework::{
    pipeline::{sequential::SequentialConfig, CommitterConfig},
    ingestion::{IngestionConfig, ClientArgs},
    Indexer, IndexerArgs,
};
use tokio;
use url::Url;

use store::ClickHouseStore;
use handlers::TransactionDigestHandler;

#[derive(Parser)]
#[clap(
    name = "clickhouse-sui-indexer",
    about = "A Sui indexer that writes transaction digests to ClickHouse",
    version = "0.1.0"
)]
struct Args {
    #[clap(flatten)]
    pub indexer_args: IndexerArgs,
    
    #[clap(flatten)]
    pub client_args: ClientArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize crypto provider for rustls
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install crypto provider");
    
    // Initialize logging and load .env file if present
    dotenvy::dotenv().ok();
    
    // Parse command-line arguments
    let args = Args::parse();
    
    // Get ClickHouse URL from environment, with a sensible default for local development
    let clickhouse_url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123".to_string())
        .parse::<Url>()?;
    
    println!("Connecting to ClickHouse at: {}", clickhouse_url);
    
    // Create our custom ClickHouse store
    let store = ClickHouseStore::new(clickhouse_url);
    
    // Ensure the database tables are created before starting the indexer
    println!("Creating ClickHouse tables if they don't exist...");
    store.create_tables_if_not_exists().await?;
    println!("✓ ClickHouse tables ready");
    
    // Manually build the indexer with our custom ClickHouse store
    // This is the key difference from basic-sui-indexer which uses IndexerCluster::builder()
    let mut indexer = Indexer::new(
        store.clone(),
        args.indexer_args,
        args.client_args,
        IngestionConfig::default(),
        None, // No metrics prefix
        &Default::default(), // Empty prometheus registry
        tokio_util::sync::CancellationToken::new(),
    ).await?;
    
    println!("Registering transaction digest handler...");
    
    // Register our sequential pipeline handler
    // This processes checkpoints in order and extracts transaction digests
    indexer.sequential_pipeline(
        TransactionDigestHandler,
        SequentialConfig {
            committer: CommitterConfig::default(),
            checkpoint_lag: 0, // Process checkpoints as soon as they're available
        },
    ).await?;
    
    println!("✓ Pipeline registered");
    println!("Starting ClickHouse Sui indexer...");
    println!("Press Ctrl+C to stop the indexer");
    
    // Start the indexer and wait for it to complete
    println!("Calling indexer.run()...");
    let handle = indexer.run().await?;
    println!("indexer.run() returned, waiting for handle...");
    
    // This will run until the indexer is stopped (e.g., by Ctrl+C)
    handle.await?;
    
    println!("Indexer stopped");
    Ok(())
}