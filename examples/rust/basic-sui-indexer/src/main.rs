// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod handlers;
mod models;

use handlers::TransactionDigestHandler;

pub mod schema;

use anyhow::{Result, bail};
use clap::Parser;
use diesel_migrations::{EmbeddedMigrations, embed_migrations};
use sui_indexer_alt_framework::{
    cluster::{Args, IndexerCluster},
    pipeline::sequential::SequentialConfig,
    service::Error,
};
use tokio;
use url::Url;

// Embed database migrations into the binary so they run automatically on startup
const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env data
    dotenvy::dotenv().ok();

    // Local database URL created in step 3 above
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in the environment")
        .parse::<Url>()
        .expect("Invalid database URL");

    // Parse command-line arguments (checkpoint range, URLs, performance settings)
    let args = Args::parse();

    // Build and configure the indexer cluster
    let mut cluster = IndexerCluster::builder()
        .with_args(args) // Apply command-line configuration
        .with_database_url(database_url) // Set up database URL
        .with_migrations(&MIGRATIONS) // Enable automatic schema migrations
        .build()
        .await?;

    // Register our custom sequential pipeline with the cluster
    cluster
        .sequential_pipeline(
            TransactionDigestHandler,    // Our processor/handler implementation
            SequentialConfig::default(), // Use default batch sizes and checkpoint lag
        )
        .await?;

    // Start the indexer and wait for completion
    match cluster.run().await?.main().await {
        Ok(()) | Err(Error::Terminated) => Ok(()),
        Err(Error::Aborted) => {
            bail!("Indexer aborted due to an unexpected error")
        }
        Err(Error::Task(e)) => {
            bail!(e)
        }
    }
}
