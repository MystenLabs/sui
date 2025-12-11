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

use crate::Row;
use crate::tables::TransactionBCSRow;

pub struct TransactionBCSProcessor;

impl Row for TransactionBCSRow {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

#[async_trait]
impl Processor for TransactionBCSProcessor {
    const NAME: &'static str = "transaction_bcs";
    const FANOUT: usize = 10;
    type Value = TransactionBCSRow;

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

            entries.push(TransactionBCSRow {
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
