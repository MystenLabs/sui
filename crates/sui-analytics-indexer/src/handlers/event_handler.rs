// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};

use sui_indexer::framework::Handler;
use sui_rest_api::CheckpointData;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEvents;
use sui_types::event::Event;

use crate::handlers::AnalyticsHandler;
use crate::tables::EventEntry;
use crate::FileType;

pub struct EventHandler {
    events: Vec<EventEntry>,
}

#[async_trait::async_trait]
impl Handler for EventHandler {
    fn name(&self) -> &str {
        "event"
    }
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;
        for checkpoint_transaction in checkpoint_transactions {
            if let Some(events) = &checkpoint_transaction.events {
                self.process_events(
                    checkpoint_summary.epoch,
                    checkpoint_summary.sequence_number,
                    checkpoint_transaction.transaction.digest(),
                    checkpoint_summary.timestamp_ms,
                    events,
                );
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<EventEntry> for EventHandler {
    fn read(&mut self) -> Result<Vec<EventEntry>> {
        let cloned = self.events.clone();
        self.events.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Event)
    }
}

impl EventHandler {
    pub fn new() -> Self {
        EventHandler { events: vec![] }
    }
    fn process_events(
        &mut self,
        epoch: u64,
        checkpoint: u64,
        digest: &TransactionDigest,
        timestamp_ms: u64,
        events: &TransactionEvents,
    ) {
        for (idx, event) in events.data.iter().enumerate() {
            let Event {
                package_id,
                transaction_module,
                sender,
                type_,
                contents,
            } = event;
            let entry = EventEntry {
                transaction_digest: digest.base58_encode(),
                event_index: idx as u64,
                checkpoint,
                epoch,
                timestamp_ms,
                sender: sender.to_string(),
                package: package_id.to_string(),
                module: transaction_module.to_string(),
                event_type: type_.to_string(),
                bcs: Base64::encode(contents.clone()),
            };
            self.events.push(entry);
        }
    }
}
