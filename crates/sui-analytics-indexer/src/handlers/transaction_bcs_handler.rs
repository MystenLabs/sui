// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use fastcrypto::encoding::{Base64, Encoding};
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::EpochId;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::parquet::ParquetBatch;
use crate::tables::TransactionBCSEntry;
use crate::{AnalyticsBatch, AnalyticsHandler, CheckpointMetadata, FileType};

pub struct TransactionBCSBatch {
    pub inner: ParquetBatch<TransactionBCSEntry>,
}

pub struct TransactionBCSProcessor;

pub type TransactionBCSHandler = AnalyticsHandler<TransactionBCSProcessor, TransactionBCSBatch>;

impl Default for TransactionBCSBatch {
    fn default() -> Self {
        Self {
            inner: ParquetBatch::new(FileType::TransactionBCS, 0)
                .expect("Failed to create ParquetBatch"),
        }
    }
}

impl CheckpointMetadata for TransactionBCSEntry {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint_sequence_number(&self) -> u64 {
        self.checkpoint
    }
}

impl AnalyticsBatch for TransactionBCSBatch {
    type Entry = TransactionBCSEntry;

    fn inner_mut(&mut self) -> &mut ParquetBatch<Self::Entry> {
        &mut self.inner
    }

    fn inner(&self) -> &ParquetBatch<Self::Entry> {
        &self.inner
    }
}

#[async_trait]
impl Processor for TransactionBCSProcessor {
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
