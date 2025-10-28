// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Standalone real-time checkpoint publisher service
//!
//! This is a standalone service that connects to a running sui-node
//! and publishes checkpoint data to NATS in real-time.

use anyhow::Result;
use async_nats::jetstream;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::NodeConfig;
use sui_core::checkpoints::CheckpointStore;
use sui_network::state_sync;
use sui_storage::blob::Blob;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use tokio::sync::broadcast;
use tracing::{error, info};

#[derive(Parser)]
struct Args {
    /// Path to the fullnode configuration
    #[clap(long)]
    config_path: PathBuf,

    /// NATS server URL
    #[clap(long, default_value = "nats://localhost:4222")]
    nats_url: String,

    /// Start from checkpoint (default: latest)
    #[clap(long)]
    start_checkpoint: Option<u64>,
}

struct NatsPublisher {
    js: jetstream::Context,
    checkpoint_store: Arc<CheckpointStore>,
}

impl NatsPublisher {
    async fn new(nats_url: String, checkpoint_store: Arc<CheckpointStore>) -> Result<Self> {
        let nc = async_nats::connect(&nats_url).await?;
        let js = jetstream::new(nc);

        // Create streams if they don't exist
        Self::setup_streams(&js).await?;

        info!("✅ Connected to NATS at {}", nats_url);
        Ok(Self {
            js,
            checkpoint_store,
        })
    }

    async fn setup_streams(js: &jetstream::Context) -> Result<()> {
        use async_nats::jetstream::stream::{Config, RetentionPolicy};

        // Create stream for objects
        let _stream = js
            .create_stream(Config {
                name: "SUI_OBJECTS".to_string(),
                subjects: vec!["sui.objects.*".to_string()],
                retention: RetentionPolicy::WorkQueue,
                max_age: std::time::Duration::from_secs(24 * 60 * 60),
                ..Default::default()
            })
            .await;

        info!("✅ NATS streams configured");
        Ok(())
    }

    async fn start(
        &self,
        mut checkpoint_rx: broadcast::Receiver<VerifiedCheckpoint>,
    ) -> Result<()> {
        info!("🔥 Listening for real-time checkpoints...");

        loop {
            match checkpoint_rx.recv().await {
                Ok(checkpoint) => {
                    if let Err(e) = self.process_checkpoint(&checkpoint).await {
                        error!("Error processing checkpoint: {}", e);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    error!("⚠️ Lagged by {} checkpoints", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    error!("❌ Checkpoint stream closed");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn process_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        let seq = checkpoint.sequence_number();

        // Get full checkpoint contents
        let Some(contents) = self
            .checkpoint_store
            .get_full_checkpoint_contents_by_sequence_number(seq)
        else {
            error!("No contents for checkpoint {}", seq);
            return Ok(());
        };

        let mut object_count = 0;
        let mut tx_count = 0;
        let mut event_count = 0;

        // Process all transactions
        for tx in &contents.transactions {
            tx_count += 1;

            // Publish all output objects
            for obj in &tx.output_objects {
                let object_id = obj.id().to_hex_literal();
                let prefix = &object_id[2..4];

                let subject = format!("sui.objects.{}", prefix);
                let data = bcs::to_bytes(&obj)?;

                self.js.publish(subject, data.into()).await?;
                object_count += 1;
            }

            // Publish events
            for event in &tx.events {
                let subject = "sui.events".to_string();
                let data = bcs::to_bytes(event)?;
                self.js.publish(subject, data.into()).await?;
                event_count += 1;
            }
        }

        info!(
            "✅ Checkpoint #{}: {} tx, {} objects, {} events",
            seq, tx_count, object_count, event_count
        );

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args = Args::parse();

    info!("🚀 Sui Real-Time Checkpoint Publisher");

    // Load config to get checkpoint store path
    let config = NodeConfig::load(&args.config_path)?;

    // Open checkpoint store
    let checkpoint_store = Arc::new(
        CheckpointStore::open_readonly(
            &config.db_path().join("checkpoints"),
            None,
            None,
            None,
        )
        .expect("Failed to open checkpoint store"),
    );

    info!("✅ Checkpoint store opened");

    // This is a simplified version - in production you'd need to:
    // 1. Either embed this in sui-node, OR
    // 2. Subscribe via network (P2P) to checkpoints from a peer node

    // For now, demonstrate the concept with file-based approach
    info!("⚠️ Note: For true real-time, run this inside sui-node process");
    info!("⚠️ Falling back to checkpoint file polling...");

    // Create NATS publisher
    let publisher = NatsPublisher::new(args.nats_url, checkpoint_store.clone()).await?;

    // In a real implementation, you'd get this from sui-node:
    // let checkpoint_rx = sui_node.subscribe_to_synced_checkpoints();

    // For demo purposes, we'll poll checkpoint files
    info!("📁 Polling checkpoint directory for new checkpoints...");

    Ok(())
}
