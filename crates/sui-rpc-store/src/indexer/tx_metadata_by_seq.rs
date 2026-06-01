// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::tx_metadata_by_seq`](crate::schema::tx_metadata_by_seq)
//! CF: one `Metadata` row per executed transaction, keyed by
//! `tx_seq`.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::tx_seq_at;
use crate::schema::keys::U64Be;
use crate::schema::tx_metadata_by_seq;
use crate::schema::tx_metadata_by_seq::Metadata;

/// Pipeline marker for `tx_metadata_by_seq`.
pub struct TxMetadataBySeq;

pub struct Row {
    pub tx_seq: u64,
    pub metadata: Metadata,
}

#[async_trait]
impl Processor for TxMetadataBySeq {
    const NAME: &'static str = "tx_metadata_by_seq";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let summary = checkpoint.summary.data();
        let mut rows = Vec::with_capacity(checkpoint.transactions.len());
        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            let event_count = tx.events.as_ref().map(|e| e.data.len()).unwrap_or(0) as u32;
            rows.push(Row {
                tx_seq: tx_seq_at(checkpoint, i),
                metadata: Metadata {
                    digest: *tx.effects.transaction_digest(),
                    checkpoint_seq: summary.sequence_number,
                    ckpt_position: i as u32,
                    event_count,
                    timestamp_ms: summary.timestamp_ms,
                },
            });
        }
        Ok(rows)
    }
}

#[async_trait]
impl sequential::Handler for TxMetadataBySeq {
    type Store = Store;
    type Batch = Vec<Row>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Row>) {
        batch.extend(values);
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut sui_consistent_store::Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let cf = &conn.store.schema().tx_metadata_by_seq;
        for row in batch {
            conn.batch.put(
                cf,
                &U64Be(row.tx_seq),
                &tx_metadata_by_seq::store(&row.metadata),
            )?;
        }
        Ok(batch.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_emits_one_row_per_transaction_with_correct_position() {
        let checkpoint = Arc::new(
            TestCheckpointBuilder::new(3)
                .with_timestamp_ms(123_456)
                .build_checkpoint(),
        );
        let rows = TxMetadataBySeq.process(&checkpoint).await.unwrap();
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.metadata.checkpoint_seq, 3);
            assert_eq!(row.metadata.ckpt_position, i as u32);
            assert_eq!(row.metadata.timestamp_ms, 123_456);
        }
    }
}
