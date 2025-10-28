// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_nats::jetstream;
use std::sync::Arc;
use sui_types::full_checkpoint_content::CheckpointData;
use tracing::debug;

use crate::stats::PublisherStats;

pub struct CheckpointPublisher {
    js: jetstream::Context,
    publish_objects: bool,
    publish_transactions: bool,
    publish_events: bool,
}

impl CheckpointPublisher {
    pub fn new(
        js: jetstream::Context,
        publish_objects: bool,
        publish_transactions: bool,
        publish_events: bool,
    ) -> Self {
        Self {
            js,
            publish_objects,
            publish_transactions,
            publish_events,
        }
    }

    pub async fn publish_checkpoint(
        &self,
        checkpoint: &CheckpointData,
        stats: &PublisherStats,
    ) -> Result<()> {
        let seq = checkpoint.checkpoint_summary.sequence_number;

        debug!("Processing checkpoint #{}", seq);

        // Process all transactions in checkpoint
        for tx in &checkpoint.transactions {
            // Publish transaction
            if self.publish_transactions {
                let subject = "sui.transactions".to_string();
                let data = bcs::to_bytes(&tx)?;
                self.js.publish(subject, data.into()).await?;
                stats.transaction_published();
            }

            // Publish objects with hex-prefix sharding
            if self.publish_objects {
                for obj in &tx.output_objects {
                    let object_id = obj.id().to_hex_literal();
                    
                    // Extract hex prefix (first 2 chars after "0x")
                    // Example: "0x8fd1a2..." -> "8f"
                    let prefix = &object_id[2..4];
                    
                    let subject = format!("sui.objects.{}", prefix);
                    let data = bcs::to_bytes(&obj)?;
                    
                    self.js.publish(subject, data.into()).await?;
                    stats.object_published();
                }
            }

            // Publish events
            if self.publish_events {
                for event in &tx.events {
                    let subject = "sui.events".to_string();
                    let data = bcs::to_bytes(event)?;
                    self.js.publish(subject, data.into()).await?;
                    stats.event_published();
                }
            }
        }

        Ok(())
    }
}
