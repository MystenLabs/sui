// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::checkpoint_summary`](crate::schema::checkpoint_summary)
//! CF: one row per checkpoint carrying the BCS-encoded
//! `CheckpointSummary` and its quorum signature.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::schema::checkpoint_summary;
use crate::schema::keys::U64Be;

/// Pipeline marker for `checkpoint_summary`.
pub struct CheckpointSummary;

/// One stored row, ready to be put into the CF. The processor
/// pre-builds the typed [`checkpoint_summary::Value`] so the
/// commit path is just a `Batch::put` per entry.
pub struct Row {
    pub seq: u64,
    pub value: checkpoint_summary::Value,
}

#[async_trait]
impl Processor for CheckpointSummary {
    const NAME: &'static str = "checkpoint_summary";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        Ok(vec![Row {
            seq: checkpoint.summary.data().sequence_number,
            value: checkpoint_summary::store(
                checkpoint.summary.data(),
                checkpoint.summary.auth_sig(),
            ),
        }])
    }
}

#[async_trait]
impl sequential::Handler for CheckpointSummary {
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
        let cf = &conn.store.schema().checkpoint_summary;
        for row in batch {
            conn.batch.put(cf, &U64Be(row.seq), &row.value)?;
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
    async fn process_emits_one_row_per_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(42).build_checkpoint());
        let rows = CheckpointSummary.process(&checkpoint).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].seq, 42);
    }
}
