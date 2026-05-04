// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::tables;

use super::handler::BigTableProcessor;

/// Pipeline that writes one row per transaction keyed by tx_sequence_number,
/// mapping each tx_seq to its `(TransactionDigest, event_count)`.
pub struct TxSeqDigestPipeline;

#[async_trait::async_trait]
impl Processor for TxSeqDigestPipeline {
    const NAME: &'static str = "kvstore_tx_seq_digest";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let cp = checkpoint.summary.data();
        // network_total_transactions is cumulative *including* this checkpoint,
        // so tx_lo is the first tx_seq in this checkpoint.
        let tx_lo = cp.network_total_transactions - checkpoint.transactions.len() as u64;
        let timestamp_ms = cp.timestamp_ms;

        let entries = checkpoint
            .transactions
            .iter()
            .enumerate()
            .map(|(i, tx)| {
                let tx_seq = tx_lo + i as u64;
                let digest = tx.transaction.digest();
                let event_count = tx.events.as_ref().map(|e| e.data.len() as u32).unwrap_or(0);
                tables::make_entry(
                    tables::tx_seq_digest::encode_key(tx_seq),
                    tables::tx_seq_digest::encode(&digest, event_count),
                    Some(timestamp_ms),
                )
            })
            .collect();

        Ok(entries)
    }
}

impl BigTableProcessor for TxSeqDigestPipeline {
    const TABLE: &'static str = tables::tx_seq_digest::NAME;
}
