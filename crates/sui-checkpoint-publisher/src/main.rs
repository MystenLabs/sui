// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sui Checkpoint Publisher (StateSync Broadcast Version)
//!
//! Runs as part of sui-node and subscribes to real-time StateSync broadcast channels
//! to publish checkpoint data to NATS with minimal latency (~100-500ms).
//!
//! This is the FASTEST ingestion layer for the Sui RPC Shard Architecture.

use anyhow::{Context, Result};
use async_nats::jetstream::{self, stream};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::NodeConfig;
use sui_node::SuiNode;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

mod publisher;
mod stats;

use publisher::CheckpointPublisher;
use stats::PublisherStats;

#[derive(Parser, Debug)]
#[clap(name = "sui-checkpoint-publisher")]
#[clap(about = "Real-time checkpoint publisher using StateSync broadcast (FASTEST)")]
struct Args {
    /// Path to the fullnode configuration file
    #[clap(long, env = "SUI_CONFIG_PATH", default_value = "fullnode.yaml")]
    config_path: PathBuf,

    /// NATS server URL
    #[clap(long, env = "NATS_URL", default_value = "nats://localhost:4222")]
    nats_url: String,

    /// NATS stream name
    #[clap(long, default_value = "SUI_OBJECTS")]
    stream_name: String,

    /// Enable object publishing
    #[clap(long, default_value = "true")]
    publish_objects: bool,

    /// Enable transaction publishing
    #[clap(long, default_value = "true")]
    publish_transactions: bool,

    /// Enable event publishing
    #[clap(long, default_value = "true")]
    publish_events: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    info!("🚀 Sui Checkpoint Publisher (StateSync Broadcast) starting...");
    info!("   Config path: {}", args.config_path.display());
    info!("   NATS URL: {}", args.nats_url);
    info!("   Mode: Real-time broadcast (LOWEST LATENCY)");

    // Load node configuration
    let config = NodeConfig::load(&args.config_path)
        .context("Failed to load node configuration")?;

    // Start Prometheus metrics server
    let registry_service = mysten_metrics::start_prometheus_server(config.metrics_address);
    let registry = registry_service.default_registry();

    info!("📡 Starting Sui node...");
    
    // Start the Sui node
    let node = SuiNode::start(&config, registry)
        .await
        .context("Failed to start Sui node")?;

    info!("✅ Sui node started");

    // Connect to NATS
    let nc = async_nats::connect(&args.nats_url)
        .await
        .context("Failed to connect to NATS")?;
    info!("✅ Connected to NATS");

    let js = jetstream::new(nc);

    // Setup NATS streams
    setup_nats_streams(&js, &args.stream_name).await?;
    info!("✅ NATS streams configured");

    // Create publisher
    let publisher = Arc::new(CheckpointPublisher::new(
        js,
        args.publish_objects,
        args.publish_transactions,
        args.publish_events,
    ));

    let stats = Arc::new(PublisherStats::new());

    // Start stats reporter
    let stats_clone = stats.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            stats_clone.report();
        }
    });

    // Subscribe to StateSync broadcast channel
    info!("⚡ Subscribing to StateSync broadcast (real-time)...");
    
    let checkpoint_store = node.checkpoint_store();
    let mut checkpoint_rx = node.subscribe_to_synced_checkpoints();

    info!("🎯 Listening for checkpoints (latency: ~100-500ms)...");

    // Process checkpoints in real-time
    while let Ok(checkpoint) = checkpoint_rx.recv().await {
        let seq = checkpoint.sequence_number();
        info!("⚡ Real-time checkpoint: {}", seq);

        // Spawn task to process checkpoint
        let publisher = publisher.clone();
        let stats = stats.clone();
        let checkpoint_store = checkpoint_store.clone();

        tokio::spawn(async move {
            if let Err(e) = process_checkpoint_realtime(
                checkpoint,
                &checkpoint_store,
                &publisher,
                &stats,
            )
            .await
            {
                error!("Failed to process checkpoint {}: {}", seq, e);
                stats.error();
            } else {
                stats.checkpoint_processed();
            }
        });
    }

    warn!("StateSync broadcast channel closed");
    Ok(())
}

async fn setup_nats_streams(js: &jetstream::Context, stream_name: &str) -> Result<()> {
    // Create stream for objects (hex-prefix sharded)
    let config = stream::Config {
        name: stream_name.to_string(),
        subjects: vec![
            "sui.objects.*".to_string(),
            "sui.transactions".to_string(),
            "sui.events".to_string(),
        ],
        retention: stream::RetentionPolicy::WorkQueue,
        storage: stream::StorageType::File,
        max_age: Duration::from_secs(24 * 3600),
        ..Default::default()
    };

    match js.get_or_create_stream(config).await {
        Ok(_) => Ok(()),
        Err(e) => {
            warn!("Stream might already exist: {}", e);
            Ok(())
        }
    }
}

async fn process_checkpoint_realtime(
    checkpoint: VerifiedCheckpoint,
    checkpoint_store: &Arc<sui_core::checkpoints::CheckpointStore>,
    publisher: &CheckpointPublisher,
    stats: &PublisherStats,
) -> Result<()> {
    let seq = checkpoint.sequence_number();

    // Get full checkpoint contents from store
    let contents = checkpoint_store
        .get_full_checkpoint_contents_by_sequence_number(seq)
        .context("Failed to get checkpoint contents")?
        .context("Checkpoint contents not found")?;

    // Convert to CheckpointData for publishing
    let checkpoint_data = Arc::new(sui_types::full_checkpoint_content::CheckpointData {
        checkpoint_summary: checkpoint.into_inner(),
        checkpoint_contents: contents.into_inner().into_inner(),
        transactions: checkpoint_store
            .get_checkpoint_data(seq, contents.iter())?
            .into_iter()
            .map(|data| data.transaction)
            .collect(),
    });

    // Publish checkpoint
    publisher.publish_checkpoint(&checkpoint_data, stats).await?;

    Ok(())
}
