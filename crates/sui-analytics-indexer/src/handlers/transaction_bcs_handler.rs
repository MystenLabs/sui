// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::encoding::{Base64, Encoding};
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::parquet::ParquetBatch;
use crate::tables::TransactionBCSEntry;
use crate::{FileType, PipelineConfig};

pub struct TransactionBCSBatch {
    pub inner: ParquetBatch<TransactionBCSEntry>,
}

impl Default for TransactionBCSBatch {
    fn default() -> Self {
        Self {
            inner: ParquetBatch::new(FileType::TransactionBCS, 0)
                .expect("Failed to create ParquetBatch"),
        }
    }
}

pub struct TransactionBCSHandler {
    config: PipelineConfig,
}

impl TransactionBCSHandler {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Processor for TransactionBCSHandler {
    const NAME: &'static str = "transaction_bcs";
    const FANOUT: usize = 10;
    type Value = TransactionBCSEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut entries = Vec::with_capacity(checkpoint.transactions.len());

        for checkpoint_transaction in &checkpoint.transactions {
            let txn = &checkpoint_transaction.transaction;
            let transaction_digest = checkpoint_transaction
                .effects
                .transaction_digest()
                .base58_encode();

            entries.push(TransactionBCSEntry {
                transaction_digest,
                checkpoint: checkpoint_seq,
                epoch,
                timestamp_ms,
                bcs: Base64::encode(bcs::to_bytes(txn)?),
            });
        }

        Ok(entries)
    }
}

#[async_trait]
impl Handler for TransactionBCSHandler {
    type Store = ObjectStore;
    type Batch = TransactionBCSBatch;


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

        batch.inner.set_epoch(first.epoch);
        batch.inner.update_last_checkpoint(first.checkpoint);

        // Write first value and remaining values
        if let Err(e) = batch
            .inner
            .write_rows(std::iter::once(first).chain(values.by_ref()))
        {
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
        let Some(file_path) = batch.inner.current_file_path() else {
            return Ok(0);
        };

        let row_count = batch.inner.row_count()?;
        let file_bytes = tokio::fs::read(file_path).await?;
        let object_path = batch.inner.object_store_path();

        conn.object_store()
            .put(&object_path, file_bytes.into())
            .await?;

        Ok(row_count)
    }
}
