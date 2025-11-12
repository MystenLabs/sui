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

use crate::FileType;
use crate::package_store::PackageCache;
use crate::tables::EventEntry;
use crate::writers::AnalyticsWriter;

pub struct EventHandler {
    package_cache: Arc<PackageCache>,
}

impl EventHandler {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self { package_cache }
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
    type Batch = Vec<EventEntry>;

    const MIN_EAGER_ROWS: usize = 100_000;
    const MAX_PENDING_ROWS: usize = 500_000;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        batch.extend(values);

        if batch.len() >= Self::MIN_EAGER_ROWS {
            BatchStatus::Ready
        } else {
            BatchStatus::Pending
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        if batch.is_empty() {
            return Ok(0);
        }

        // Get the checkpoint range from the batch
        let first_checkpoint = batch.first().unwrap().checkpoint;
        let last_checkpoint = batch.last().unwrap().checkpoint;
        let epoch = batch.first().unwrap().epoch;

        // Create a temporary Parquet file
        use crate::parquet::ParquetWriter;
        use tempfile::TempDir;

        let temp_dir = TempDir::new()?;
        let mut writer: ParquetWriter =
            ParquetWriter::new(temp_dir.path(), FileType::Event, first_checkpoint)?;

        // Collect into a vec to satisfy 'static lifetime requirement
        let rows: Vec<EventEntry> = batch.to_vec();
        AnalyticsWriter::<EventEntry>::write(&mut writer, Box::new(rows.into_iter()))?;
        AnalyticsWriter::<EventEntry>::flush(&mut writer, last_checkpoint + 1)?;

        // Build the object store path
        let file_path = FileType::Event.file_path(
            crate::FileFormat::PARQUET,
            epoch,
            first_checkpoint..(last_checkpoint + 1),
        );

        // Read the file and upload
        let local_file = temp_dir
            .path()
            .join(FileType::Event.dir_prefix().as_ref())
            .join(format!("epoch_{}", epoch))
            .join(format!(
                "{}_{}.parquet",
                first_checkpoint,
                last_checkpoint + 1
            ));

        let file_bytes = tokio::fs::read(&local_file).await?;

        conn.object_store()
            .put(&file_path, file_bytes.into())
            .await?;

        Ok(batch.len())
    }
}
