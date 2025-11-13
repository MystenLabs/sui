// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::{BatchStatus, Handler};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI;

use crate::parquet::ParquetBatch;
use crate::tables::MoveCallEntry;
use crate::{FileType, PipelineConfig};

pub struct MoveCallBatch {
    pub inner: ParquetBatch<MoveCallEntry>,
}

impl Default for MoveCallBatch {
    fn default() -> Self {
        Self {
            inner: ParquetBatch::new(FileType::MoveCall, 0).expect("Failed to create ParquetBatch"),
        }
    }
}

pub struct MoveCallHandler {
    config: PipelineConfig,
}

impl MoveCallHandler {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Processor for MoveCallHandler {
    const NAME: &'static str = "move_call";
    const FANOUT: usize = 10;
    type Value = MoveCallEntry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut entries = Vec::new();

        for executed_tx in &checkpoint.transactions {
            let move_calls = executed_tx.transaction.move_calls();
            let transaction_digest = executed_tx.effects.transaction_digest().base58_encode();

            for (package, module, function) in move_calls.iter() {
                let entry = MoveCallEntry {
                    transaction_digest: transaction_digest.clone(),
                    checkpoint: checkpoint_seq,
                    epoch,
                    timestamp_ms,
                    package: package.to_string(),
                    module: module.to_string(),
                    function: function.to_string(),
                };
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

crate::impl_analytics_handler!(MoveCallHandler, MoveCallBatch, checkpoint);
