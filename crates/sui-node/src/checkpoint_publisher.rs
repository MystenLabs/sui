// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Real-time checkpoint publisher plugin for sui-node
//!
//! This module provides a plugin that runs inside sui-node and publishes
//! checkpoint data to NATS/RabbitMQ in real-time as checkpoints are synced.

use anyhow::Result;
use async_nats::jetstream;
use std::sync::Arc;
use sui_core::checkpoints::CheckpointStore;
use sui_network::state_sync;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

pub struct CheckpointPublisherConfig {
    pub nats_url: String,
    pub enable_objects: bool,
    pub enable_transactions: bool,
    pub enable_events: bool,
    pub batch_size: usize,
}

impl Default for CheckpointPublisherConfig {
    fn default() -> Self {
        Self {
            nats_url: "nats://localhost:4222".to_string(),
            enable_objects: true,
            enable_transactions: true,
            enable_events: true,
            batch_size: 100,
        }
    }
}

pub struct CheckpointPublisher {
    config: CheckpointPublisherConfig,
    js: jetstream::Context,
    checkpoint_store: Arc<CheckpointStore>,
    state_sync: state_sync::Handle,
}

impl CheckpointPublisher {
    pub async fn new(
        config: CheckpointPublisherConfig,
        checkpoint_store: Arc<CheckpointStore>,
        state_sync: state_sync::Handle,
    ) -> Result<Self> {
        let nc = async_nats::connect(&config.nats_url).await?;
        let js = jetstream::new(nc);

        // Setup NATS streams
        Self::setup_nats_streams(&js).await?;

        info!("✅ Checkpoint publisher initialized");

        Ok(Self {
            config,
            js,
            checkpoint_store,
            state_sync,
        })
    }

    async fn setup_nats_streams(js: &jetstream::Context) -> Result<()> {
        use async_nats::jetstream::stream::{Config, RetentionPolicy, StorageType};

        // Create stream for objects with hex-prefix sharding
        let _objects_stream = js
            .get_or_create_stream(Config {
                name: "SUI_OBJECTS".to_string(),
                subjects: vec!["sui.objects.*".to_string()],
                retention: RetentionPolicy::WorkQueue,
                storage: StorageType::File,
                max_age: std::time::Duration::from_secs(24 * 3600),
                ..Default::default()
            })
            .await?;

        info!("✅ NATS streams configured");
        Ok(())
    }

    /// Start the publisher as a background task
    pub fn start(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(e) = self.run().await {
                error!("Checkpoint publisher error: {}", e);
            }
        })
    }

    async fn run(&self) -> Result<()> {
        // Subscribe to checkpoint stream from StateSync
        let mut checkpoint_rx = self.state_sync.subscribe_to_synced_checkpoints();

        info!("🔥 Checkpoint publisher started, listening for checkpoints...");

        loop {
            match checkpoint_rx.recv().await {
                Ok(checkpoint) => {
                    if let Err(e) = self.process_checkpoint(&checkpoint).await {
                        error!(
                            "Error processing checkpoint {}: {}",
                            checkpoint.sequence_number(),
                            e
                        );
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("⚠️ Publisher lagged by {} checkpoints", n);
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
            warn!("No contents for checkpoint {}", seq);
            return Ok(());
        };

        let mut stats = PublishStats::default();

        // Process all transactions
        for tx in &contents.transactions {
            // Publish transaction if enabled
            if self.config.enable_transactions {
                let subject = "sui.transactions".to_string();
                let data = bcs::to_bytes(&tx)?;
                self.js.publish(subject, data.into()).await?;
                stats.transactions += 1;
            }

            // Publish objects if enabled
            if self.config.enable_objects {
                for obj in &tx.output_objects {
                    let object_id = obj.id().to_hex_literal();
                    
                    // Extract hex prefix for sharding (first 2 chars after "0x")
                    let prefix = &object_id[2..4];
                    let subject = format!("sui.objects.{}", prefix);
                    
                    let data = bcs::to_bytes(&obj)?;
                    self.js.publish(subject, data.into()).await?;
                    stats.objects += 1;
                }
            }

            // Publish events if enabled
            if self.config.enable_events {
                for event in &tx.events {
                    let subject = "sui.events".to_string();
                    let data = bcs::to_bytes(event)?;
                    self.js.publish(subject, data.into()).await?;
                    stats.events += 1;
                }
            }
        }

        info!(
            "✅ Published checkpoint #{}: {} tx, {} objects, {} events",
            seq, stats.transactions, stats.objects, stats.events
        );

        Ok(())
    }
}

#[derive(Default)]
struct PublishStats {
    transactions: usize,
    objects: usize,
    events: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_prefix_extraction() {
        let object_id = "0x8fd1a2e8cf7d4b3d0b4e1a5b7a2e4fdd5bb2a9c37a2b4d11d9a1d6b5a0b4c9e3";
        let prefix = &object_id[2..4];
        assert_eq!(prefix, "8f");
    }
}
