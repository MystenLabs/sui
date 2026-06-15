// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::tx_seq_by_digest`](crate::schema::tx_seq_by_digest)
//! CF: one `TransactionDigest → tx_seq` row per executed
//! transaction.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::tx_seq_at;
use crate::schema::keys::U64Varint;
use crate::schema::tx_seq_by_digest;

/// Pipeline marker for `tx_seq_by_digest`.
pub struct TxSeqByDigest;

pub struct Row {
    pub digest: TransactionDigest,
    pub tx_seq: u64,
}

#[async_trait]
impl Processor for TxSeqByDigest {
    const NAME: &'static str = "tx_seq_by_digest";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::with_capacity(checkpoint.transactions.len());
        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            rows.push(Row {
                digest: *tx.effects.transaction_digest(),
                tx_seq: tx_seq_at(checkpoint, i),
            });
        }
        Ok(rows)
    }
}

#[async_trait]
impl sequential::Handler for TxSeqByDigest {
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
        let cf = &conn.store.schema().tx_seq_by_digest;
        for row in batch {
            conn.batch.put(
                cf,
                &tx_seq_by_digest::Key(row.digest),
                &U64Varint(row.tx_seq),
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
    async fn process_emits_one_row_per_transaction() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(5).build_checkpoint());
        let rows = TxSeqByDigest.process(&checkpoint).await.unwrap();
        assert_eq!(rows.len(), checkpoint.transactions.len());
    }
}
