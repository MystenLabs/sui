// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::transactions`](crate::schema::transactions) CF: one
//! row per executed transaction, keyed by its assigned `tx_seq`.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::tx_seq_at;
use crate::schema::primitives::U64Be;
use crate::schema::transactions;

/// Pipeline marker for `transactions`.
pub struct Transactions;

pub struct Row {
    pub tx_seq: u64,
    pub value: transactions::Value,
}

#[async_trait]
impl Processor for Transactions {
    const NAME: &'static str = "transactions";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::with_capacity(checkpoint.transactions.len());
        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            rows.push(Row {
                tx_seq: tx_seq_at(checkpoint, i),
                value: transactions::store(&tx.transaction, &tx.signatures),
            });
        }
        Ok(rows)
    }
}

#[async_trait]
impl sequential::Handler for Transactions {
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
        let cf = &conn.store.schema().transactions;
        for row in batch {
            conn.batch.put(cf, &U64Be(row.tx_seq), &row.value)?;
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
        let checkpoint = Arc::new(
            TestCheckpointBuilder::new(5)
                .with_network_total_transactions(10)
                .build_checkpoint(),
        );
        let n = checkpoint.transactions.len() as u64;
        let rows = Transactions.process(&checkpoint).await.unwrap();
        assert_eq!(rows.len() as u64, n);
        // tx_seqs should be a contiguous range ending at
        // network_total_transactions - 1.
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.tx_seq, 10 - n + i as u64);
        }
    }
}
