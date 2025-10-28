// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Example: Real-time Checkpoint Publisher to NATS
//!
//! This example demonstrates how to subscribe to Sui's real-time checkpoint
//! stream and publish to NATS JetStream, similar to Firedancer's RabbitMQ approach.
//!
//! Usage:
//!   cargo run --example checkpoint_publisher -- --config-path /path/to/fullnode.yaml

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::NodeConfig;
use sui_node::SuiNode;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use tokio::sync::broadcast;
use tracing::{error, info};

#[derive(Parser)]
#[clap(name = "checkpoint-publisher")]
struct Args {
    #[clap(long)]
    config_path: PathBuf,

    #[clap(long, default_value = "nats://localhost:4222")]
    nats_url: String,
}

/// Example publisher that consumes checkpoint stream
struct CheckpointPublisher {
    nats_client: async_nats::Client,
}

impl CheckpointPublisher {
    async fn new(nats_url: String) -> Result<Self> {
        let nats_client = async_nats::connect(&nats_url).await?;
        info!("Connected to NATS at {}", nats_url);
        Ok(Self { nats_client })
    }

    async fn start(
        &self,
        mut checkpoint_rx: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<sui_core::checkpoints::CheckpointStore>,
    ) -> Result<()> {
        info!("🔥 Starting real-time checkpoint publisher...");

        loop {
            match checkpoint_rx.recv().await {
                Ok(checkpoint) => {
                    let seq = checkpoint.sequence_number();
                    info!("📦 Received checkpoint #{}", seq);

                    // Get full checkpoint contents
                    if let Some(contents) = checkpoint_store
                        .get_full_checkpoint_contents_by_sequence_number(seq)
                    {
                        self.process_checkpoint(&checkpoint, &contents).await?;
                    } else {
                        error!("Failed to get contents for checkpoint {}", seq);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    error!("⚠️ Lagged by {} checkpoints, catching up...", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    error!("❌ Checkpoint stream closed");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn process_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: &sui_types::messages_checkpoint::FullCheckpointContents,
    ) -> Result<()> {
        let seq = checkpoint.sequence_number();

        // Process all transactions in this checkpoint
        for tx in &contents.transactions {
            // Publish transaction
            let tx_subject = "sui.transactions";
            let tx_data = bcs::to_bytes(&tx)?;
            self.nats_client
                .publish(tx_subject.to_string(), tx_data.into())
                .await?;

            // Process all output objects
            for obj in &tx.output_objects {
                let object_id = obj.id().to_hex_literal();
                let prefix = &object_id[2..4]; // Extract first 2 hex chars

                // Route to shard based on prefix (hex-prefix sharding)
                let subject = format!("sui.objects.{}", prefix);
                let obj_data = bcs::to_bytes(&obj)?;

                self.nats_client
                    .publish(subject.clone(), obj_data.into())
                    .await?;

                info!("  ✅ Published {} to {}", object_id, subject);
            }

            // Publish events
            for event in &tx.events {
                let event_subject = "sui.events";
                let event_data = bcs::to_bytes(event)?;
                self.nats_client
                    .publish(event_subject.to_string(), event_data.into())
                    .await?;
            }
        }

        info!(
            "✅ Published checkpoint #{} ({} transactions)",
            seq,
            contents.transactions.len()
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

    info!("🚀 Starting Sui Checkpoint Publisher");
    info!("Config: {:?}", args.config_path);

    // Load node configuration
    let config = NodeConfig::load(&args.config_path)?;

    // Start SuiNode
    let registry_service = mysten_metrics::start_prometheus_server(config.metrics_address);
    let node = SuiNode::start(config, registry_service.0).await?;

    info!("✅ SuiNode started successfully");

    // Create NATS publisher
    let publisher = CheckpointPublisher::new(args.nats_url).await?;

    // Subscribe to checkpoint stream (NEW API!)
    let checkpoint_rx = node.subscribe_to_synced_checkpoints();
    let checkpoint_store = node.checkpoint_store();

    info!("🎧 Subscribed to checkpoint stream");

    // Start publishing (blocks forever)
    publisher.start(checkpoint_rx, checkpoint_store).await?;

    Ok(())
}
