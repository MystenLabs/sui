// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that emits "epoch end" partial records
//! into the [`schema::epochs`](crate::schema::epochs) CF. The
//! schema's merge operator combines them field-wise with the
//! "epoch start" record once the start side lands.
//!
//! The start side is intentionally deferred: emitting it requires
//! pulling `protocol_version`, `reference_gas_price`, and the
//! BCS-encoded `SuiSystemState` out of the system-state object
//! at the epoch boundary, and the Move-side accessors that
//! cover every system-state version are still in flight. The
//! merge operator is designed for this — a row that only ever
//! saw an end record decodes with `start_*` fields as `None`.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::schema::epochs;
use crate::schema::keys::U64Be;

/// Pipeline marker for `epochs`.
pub struct Epochs;

pub struct Row {
    pub epoch: u64,
    pub end_timestamp_ms: u64,
    pub end_checkpoint: u64,
}

#[async_trait]
impl Processor for Epochs {
    const NAME: &'static str = "epochs";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let summary = checkpoint.summary.data();
        // Only checkpoints that mark the end of an epoch carry
        // `end_of_epoch_data`. All others contribute nothing to
        // this CF.
        if summary.end_of_epoch_data.is_none() {
            return Ok(vec![]);
        }
        Ok(vec![Row {
            epoch: summary.epoch,
            end_timestamp_ms: summary.timestamp_ms,
            end_checkpoint: summary.sequence_number,
        }])
    }
}

#[async_trait]
impl sequential::Handler for Epochs {
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
        let cf = &conn.store.schema().epochs;
        for row in batch {
            conn.batch.merge(
                cf,
                &U64Be(row.epoch),
                &epochs::end(row.end_timestamp_ms, row.end_checkpoint),
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
    async fn process_emits_nothing_for_non_epoch_end_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let rows = Epochs.process(&checkpoint).await.unwrap();
        assert!(rows.is_empty());
    }
}
