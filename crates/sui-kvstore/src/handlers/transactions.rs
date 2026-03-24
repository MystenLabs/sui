// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::balance_change::derive_balance_changes_2;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::tables;

/// Pipeline that writes transactions to BigTable.
/// Wrap with `BigTableHandler` for the full `concurrent::Handler` implementation.
pub struct TransactionsPipeline;

#[async_trait::async_trait]
impl Processor for TransactionsPipeline {
    const NAME: &'static str = "kvstore_transactions";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.summary.timestamp_ms;
        let checkpoint_number = checkpoint.summary.sequence_number;
        let mut entries = Vec::with_capacity(checkpoint.transactions.len());

        for tx in &checkpoint.transactions {
            let balance_changes = derive_balance_changes_2(&tx.effects, &checkpoint.object_set);
            let digest = tx.transaction.digest();

            let entry = tables::make_entry(
                tables::transactions::encode_key(&digest),
                tables::transactions::encode(
                    &tx.transaction,
                    &tx.signatures,
                    &tx.effects,
                    &tx.events,
                    checkpoint_number,
                    timestamp_ms,
                    &balance_changes,
                    &tx.unchanged_loaded_runtime_objects,
                )?,
                Some(timestamp_ms),
            );

            entries.push(entry);
        }

        Ok(entries)
    }
}

impl BigTableProcessor for TransactionsPipeline {
    const TABLE: &'static str = tables::transactions::NAME;
}
