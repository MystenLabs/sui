// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::effects`](crate::schema::effects) CF: one row per
//! executed transaction carrying its [`sui_types::effects::TransactionEffects`] and
//! the set of objects loaded but not modified during execution.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::tx_seq_at;
use crate::schema::effects;
use crate::schema::keys::U64Be;

/// Pipeline marker for `effects`.
pub struct Effects;

pub struct Row {
    pub tx_seq: u64,
    pub value: effects::Value,
}

#[async_trait]
impl Processor for Effects {
    const NAME: &'static str = "effects";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::with_capacity(checkpoint.transactions.len());
        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            rows.push(Row {
                tx_seq: tx_seq_at(checkpoint, i),
                value: effects::store(&tx.effects, &tx.unchanged_loaded_runtime_objects),
            });
        }
        Ok(rows)
    }
}

#[async_trait]
impl sequential::Handler for Effects {
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
        let cf = &conn.store.schema().effects;
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
        let checkpoint = Arc::new(TestCheckpointBuilder::new(2).build_checkpoint());
        let rows = Effects.process(&checkpoint).await.unwrap();
        assert_eq!(rows.len(), checkpoint.transactions.len());
    }
}
