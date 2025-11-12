// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use move_core_types::annotated_value::MoveValue;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_json_rpc_types::type_and_fields_from_move_event_data;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::event::Event;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::PipelineConfig;
use crate::package_store::PackageCache;
use crate::parquet::ParquetBatch;
use crate::tables::EventEntry;

pub struct EventHandler {
    package_cache: Arc<PackageCache>,
    config: PipelineConfig,
}

impl EventHandler {
    pub fn new(package_cache: Arc<PackageCache>, config: PipelineConfig) -> Self {
        Self {
            package_cache,
            config,
        }
    }
}

#[async_trait]
impl Processor for EventHandler {
    const NAME: &'static str = "event";
    const FANOUT: usize = 10;
    type Value = EventEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut entries = Vec::new();

        for executed_tx in &checkpoint.transactions {
            let digest = executed_tx.effects.transaction_digest();

            if let Some(events) = &executed_tx.events {
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
            }
        }

        Ok(entries)
    }
}

#[async_trait]
impl Handler for EventHandler {
    type Store = ObjectStore;
    type Batch = ParquetBatch<EventEntry>;

    const MIN_EAGER_ROWS: usize = usize::MAX;
    const MAX_PENDING_ROWS: usize = usize::MAX;

    fn min_eager_rows(&self) -> usize {
        self.config.max_row_count
    }

    fn max_pending_rows(&self) -> usize {
        self.config.max_row_count * 5
    }

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        // Get first value to extract epoch and checkpoint
        let Some(first) = values.next() else {
            return BatchStatus::Pending;
        };

        batch.set_epoch(first.epoch);
        batch.update_last_checkpoint(first.checkpoint);

        // Write first value and remaining values
        if let Err(e) = batch.write_rows(std::iter::once(first).chain(values.by_ref()), crate::FileType::Event) {
            tracing::error!("Failed to write rows to ParquetBatch: {}", e);
            return BatchStatus::Pending;
        }

        // Let framework decide when to flush based on min_eager_rows()
        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        let Some(file_path) = batch.current_file_path() else {
            return Ok(0);
        };

        let row_count = batch.row_count()?;
        let file_bytes = tokio::fs::read(file_path).await?;
        let object_path = batch.object_store_path();

        conn.object_store()
            .put(&object_path, file_bytes.into())
            .await?;

        Ok(row_count)
    }
}
