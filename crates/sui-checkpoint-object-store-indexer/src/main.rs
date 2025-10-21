// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use sui_checkpoint_object_store_indexer::{CheckpointBlobPipeline, EpochsPipeline};
use sui_config::object_storage_config::ObjectStoreConfig;
use sui_indexer_alt_framework::{Indexer, IndexerArgs};
use sui_indexer_alt_object_store::ObjectStore;

#[derive(Debug, clap::Parser)]
#[command(name = "sui-checkpoint-object-store-indexer")]
#[command(about = "Indexer that writes checkpoints as compressed proto blobs to object storage")]
struct Args {
    /// Number of concurrent checkpoint uploads
    #[arg(long, default_value = "10")]
    write_concurrency: usize,

    /// Interval between watermark updates
    #[arg(long, default_value = "1m", value_parser = humantime::parse_duration)]
    watermark_interval: std::time::Duration,

    #[command(flatten)]
    object_store_config: ObjectStoreConfig,

    /// Full node URL to fetch checkpoints from
    #[arg(long)]
    rpc_api_url: url::Url,

    /// Optional username for gRPC authentication
    #[arg(long)]
    rpc_username: Option<String>,

    /// Optional password for gRPC authentication
    #[arg(long)]
    rpc_password: Option<String>,

    /// Optional Zstd compression level. If not provided, data will be stored uncompressed
    #[arg(long)]
    compression_level: Option<i32>,

    #[command(flatten)]
    indexer_args: IndexerArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::Parser;
    use sui_indexer_alt_framework::{
        ingestion::{ClientArgs, IngestionConfig},
        pipeline::{CommitterConfig, concurrent::ConcurrentConfig},
    };

    let args = Args::parse();

    tracing_subscriber::fmt::init();

    tracing::info!("Starting checkpoint object store indexer");
    tracing::info!("Args: {:#?}", args);

    let object_store = args
        .object_store_config
        .make()
        .context("Failed to create object store")?;

    let store = ObjectStore::new(object_store);

    let client_args = ClientArgs {
        rpc_api_url: Some(args.rpc_api_url),
        rpc_username: args.rpc_username,
        rpc_password: args.rpc_password,
        remote_store_url: None,
        local_ingestion_path: None,
    };

    let registry = prometheus::Registry::new();
    let cancel = tokio_util::sync::CancellationToken::new();

    let config = ConcurrentConfig {
        committer: CommitterConfig {
            write_concurrency: args.write_concurrency,
            watermark_interval_ms: args.watermark_interval.as_millis() as u64,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut indexer = Indexer::new(
        store.clone(),
        args.indexer_args,
        client_args,
        IngestionConfig::default(),
        Some("checkpoint_indexer"),
        &registry,
        cancel.clone(),
    )
    .await?;

    indexer
        .concurrent_pipeline(
            CheckpointBlobPipeline {
                compression_level: args.compression_level,
            },
            config.clone(),
        )
        .await?;

    indexer
        .concurrent_pipeline(EpochsPipeline, config.clone())
        .await?;

    let mut h_indexer = indexer.run().await?;

    tokio::select! {
        res = &mut h_indexer => {
            tracing::info!("Indexer completed successfully");
            res?;
            return Ok(());
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT, shutting down...");
        }
        _ = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await
        } => {
            tracing::info!("Received SIGTERM, shutting down...");
        }
    }

    cancel.cancel();
    tracing::info!("Waiting for indexer to shut down gracefully...");
    h_indexer.await?;

    Ok(())
}
