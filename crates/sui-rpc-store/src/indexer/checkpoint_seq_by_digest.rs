// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::checkpoint_seq_by_digest`](crate::schema::checkpoint_seq_by_digest)
//! CF: one `CheckpointDigest → checkpoint_seq` row per checkpoint.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::digests::CheckpointDigest;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointSummary;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::schema::checkpoint_seq_by_digest;
use crate::schema::keys::U64Varint;

/// Pipeline marker for `checkpoint_seq_by_digest`.
pub struct CheckpointSeqByDigest;

pub struct Row {
    pub digest: CheckpointDigest,
    pub seq: u64,
}

#[async_trait]
impl Processor for CheckpointSeqByDigest {
    const NAME: &'static str = "checkpoint_seq_by_digest";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let summary: &CheckpointSummary = checkpoint.summary.data();
        Ok(vec![Row {
            digest: summary.digest(),
            seq: summary.sequence_number,
        }])
    }
}

#[async_trait]
impl sequential::Handler for CheckpointSeqByDigest {
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
        let cf = &conn.store.schema().checkpoint_seq_by_digest;
        for row in batch {
            conn.batch.put(
                cf,
                &checkpoint_seq_by_digest::Key(row.digest),
                &U64Varint(row.seq),
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
    async fn process_emits_one_row_per_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(11).build_checkpoint());
        let rows = CheckpointSeqByDigest.process(&checkpoint).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].seq, 11);
    }
}
