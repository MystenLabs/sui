// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use move_core_types::annotated_value::MoveValue;

use crate::handlers::{process_transactions, AnalyticsHandler, TransactionProcessor};
use crate::package_store::PackageCache;
use crate::tables::EventEntry;
use crate::FileType;
use sui_json_rpc_types::type_and_fields_from_move_event_data;
use sui_types::event::Event;
use sui_types::full_checkpoint_content::CheckpointData;

use super::wait_for_cache;

#[derive(Clone)]
pub struct EventHandler {
    package_cache: Arc<PackageCache>,
}

impl EventHandler {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self { package_cache }
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<EventEntry> for EventHandler {
    async fn process_checkpoint(
        &self,
        checkpoint_data: Arc<CheckpointData>,
    ) -> Result<Vec<EventEntry>> {
        wait_for_cache(&checkpoint_data, &self.package_cache).await;
        Ok(process_transactions(checkpoint_data.clone(), Arc::new(self.clone())).await?)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Event)
    }

    fn name(&self) -> &'static str {
        "event"
    }
}

#[async_trait::async_trait]
impl TransactionProcessor<EventEntry> for EventHandler {
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Vec<EventEntry>> {
        let transaction = &checkpoint.transactions[tx_idx];
        if let Some(events) = &transaction.events {
            let epoch = checkpoint.checkpoint_summary.epoch;
            let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number;
            let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;
            let digest = transaction.transaction.digest();

            let mut entries = Vec::new();
            for (idx, event) in events.data.iter().enumerate() {
                let Event {
                    package_id,
                    transaction_module,
                    sender,
                    type_,
                    contents,
                } = event;
                let layout = self
                    .package_cache
                    .resolver_for_epoch(epoch)
                    .type_layout(move_core_types::language_storage::TypeTag::Struct(
                        Box::new(type_.clone()),
                    ))
                    .await?;
                let move_value = MoveValue::simple_deserialize(contents, &layout)?;
                let (_, event_json) = type_and_fields_from_move_event_data(move_value)?;
                let entry = EventEntry {
                    transaction_digest: digest.base58_encode(),
                    event_index: idx as u64,
                    checkpoint: checkpoint_seq,
                    epoch,
                    timestamp_ms,
                    sender: sender.to_string(),
                    package: package_id.to_string(),
                    module: transaction_module.to_string(),
                    event_type: type_.to_string(),
                    bcs: "".to_string(),
                    bcs_length: contents.len() as u64,
                    event_json: event_json.to_string(),
                };

                entries.push(entry);
            }
            Ok(entries)
        } else {
            Ok(Vec::new())
        }
    }
}
