// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::BatchStatus;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework_store_traits::Store;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::storage::EpochInfo;

use crate::KeyValueStoreReader as _;
use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::bigtable::store::BigTableStore;
use crate::tables;

/// Pipeline that writes epoch data in the legacy format to BigTable.
/// This maintains backwards compatibility during migration.
pub struct EpochLegacyPipeline;

/// Epoch info with optional previous epoch update info.
pub struct EpochLegacyBatch {
    pub epoch_info: EpochInfo,
    pub prev_epoch_update: Option<PrevEpochUpdate>,
}

pub struct PrevEpochUpdate {
    pub epoch_id: u64,
    pub end_checkpoint: Option<u64>,
    pub end_timestamp_ms: Option<u64>,
}

#[async_trait::async_trait]
impl Processor for EpochLegacyPipeline {
    const NAME: &'static str = "kvstore_epochs_legacy";
    type Value = EpochLegacyBatch;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let checkpoint_data: CheckpointData = checkpoint.as_ref().clone().into();

        match checkpoint_data.epoch_info()? {
            Some(epoch_info) => {
                let prev_epoch_update = if epoch_info.epoch > 0 {
                    Some(PrevEpochUpdate {
                        epoch_id: epoch_info.epoch - 1,
                        end_checkpoint: epoch_info.start_checkpoint.map(|sq| sq - 1),
                        end_timestamp_ms: epoch_info.start_timestamp_ms,
                    })
                } else {
                    None
                };
                Ok(vec![EpochLegacyBatch {
                    epoch_info,
                    prev_epoch_update,
                }])
            }
            None => Ok(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl Handler for EpochLegacyPipeline {
    type Store = BigTableStore;
    type Batch = Option<EpochLegacyBatch>;

    const MIN_EAGER_ROWS: usize = 1;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        if let Some(value) = values.next() {
            *batch = Some(value);
            BatchStatus::Ready
        } else {
            BatchStatus::Pending
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        let Some(epoch_batch) = batch else {
            return Ok(0);
        };

        let mut entries = Vec::with_capacity(2);

        // Update previous epoch with end info if needed (read-modify-write)
        if let Some(prev_update) = &epoch_batch.prev_epoch_update {
            let prev_data = conn
                .client()
                .get_epoch(prev_update.epoch_id)
                .await?
                .with_context(|| {
                    format!(
                        "previous epoch {} not found when processing epoch {}",
                        prev_update.epoch_id, epoch_batch.epoch_info.epoch
                    )
                })?;
            let prev = EpochInfo {
                epoch: prev_data.epoch.context("missing epoch")?,
                protocol_version: prev_data.protocol_version,
                start_timestamp_ms: prev_data.start_timestamp_ms,
                start_checkpoint: prev_data.start_checkpoint,
                reference_gas_price: prev_data.reference_gas_price,
                system_state: prev_data.system_state,
                end_checkpoint: prev_update.end_checkpoint,
                end_timestamp_ms: prev_update.end_timestamp_ms,
            };
            entries.push(epoch_to_entry(&prev)?);
        }

        // Write the new epoch info
        entries.push(epoch_to_entry(&epoch_batch.epoch_info)?);

        conn.client()
            .write_entries(tables::epochs::NAME, entries)
            .await?;
        Ok(1)
    }
}

fn epoch_to_entry(epoch: &EpochInfo) -> anyhow::Result<Entry> {
    Ok(tables::make_entry(
        tables::epochs::encode_key(epoch.epoch),
        tables::epochs::encode(epoch)?,
        epoch.end_timestamp_ms.or(epoch.start_timestamp_ms),
    ))
}
