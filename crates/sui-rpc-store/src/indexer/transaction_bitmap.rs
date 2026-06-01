// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::transaction_bitmap`](crate::schema::transaction_bitmap)
//! CF.
//!
//! This first cut indexes a single dimension — transaction sender
//! — encoded as `[DIM_SENDER][sender_address_bytes]`. New
//! dimensions (transaction kind, called function, etc.) extend
//! the pipeline by emitting additional rows from `process` with
//! their own discriminator byte; the schema's merge operator
//! handles the bucket union without further work.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::tx_seq_at;
use crate::schema::transaction_bitmap;

/// Discriminator byte for the "transaction sender" dimension.
/// Distinct dimensions share the CF, so a tag byte keeps their
/// key prefixes from colliding.
pub const DIM_SENDER: u8 = 0x01;

/// Pipeline marker for `transaction_bitmap`.
pub struct TransactionBitmap;

pub struct Row {
    pub dimension_key: Vec<u8>,
    pub tx_seq: u64,
}

#[async_trait]
impl Processor for TransactionBitmap {
    const NAME: &'static str = "transaction_bitmap";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::with_capacity(checkpoint.transactions.len());
        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            let tx_seq = tx_seq_at(checkpoint, i);
            let sender = tx.transaction.sender();
            let mut dim = Vec::with_capacity(1 + sender.as_ref().len());
            dim.push(DIM_SENDER);
            dim.extend_from_slice(sender.as_ref());
            rows.push(Row {
                dimension_key: dim,
                tx_seq,
            });
        }
        Ok(rows)
    }
}

#[async_trait]
impl sequential::Handler for TransactionBitmap {
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
        let cf = &conn.store.schema().transaction_bitmap;
        for row in batch {
            let (k, v) = transaction_bitmap::store_match(row.dimension_key.clone(), row.tx_seq);
            conn.batch.merge(cf, &k, &v)?;
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
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let rows = TransactionBitmap.process(&checkpoint).await.unwrap();
        assert_eq!(rows.len(), checkpoint.transactions.len());
        for row in &rows {
            assert!(matches!(row.dimension_key.first(), Some(&DIM_SENDER)));
        }
    }
}
