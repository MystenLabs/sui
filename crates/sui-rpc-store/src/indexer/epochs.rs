// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::epochs`](crate::schema::epochs) CF.
//!
//! Driven by [`Checkpoint::epoch_info`], which returns a populated
//! [`sui_types::storage::EpochInfo`] for the *new* epoch on
//! two cases: the genesis checkpoint, and the end-of-epoch
//! checkpoint of the prior epoch (the one carrying
//! `end_of_epoch_data`). Every other checkpoint returns `None`
//! and contributes nothing.
//!
//! Mirroring the `index_epoch` flow in `sui-core::rpc_index`:
//!
//! 1. Emit a *start* operand for the new epoch — protocol version,
//!    reference gas price, start timestamp, start checkpoint, and
//!    BCS-encoded `SuiSystemState`, all read off the new
//!    system-state object the end-of-epoch transaction wrote to
//!    its outputs.
//! 2. If the new epoch is non-genesis, emit an *end* operand for
//!    the prior epoch — `end_timestamp_ms` is taken from the new
//!    epoch's start (the prior epoch ended at the moment the new
//!    one began) and `end_checkpoint` is the checkpoint
//!    immediately before the new epoch's first.

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

/// A partial [`StoredEpoch`](crate::proto::StoredEpoch) record keyed
/// by epoch number, staged as a merge operand at commit time. Both
/// the start and end partial records share this shape; the field-wise
/// merge operator in [`schema::epochs`](crate::schema::epochs)
/// combines them (and any re-seeds) into a full row.
pub struct Row {
    pub epoch: u64,
    pub value: epochs::Value,
}

#[async_trait]
impl Processor for Epochs {
    const NAME: &'static str = "epochs";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let Some(epoch_info) = checkpoint
            .epoch_info()
            .map_err(|e| anyhow::anyhow!("extract epoch_info: {e}"))?
        else {
            return Ok(vec![]);
        };

        let mut rows = Vec::with_capacity(2);

        // Start record for the new epoch. `EpochInfo` from
        // `epoch_info()` populates every field except the end-of-
        // epoch ones; pull them out into our `Row::Start` shape.
        let system_state_bcs = epoch_info
            .system_state
            .as_ref()
            .map(bcs::to_bytes)
            .transpose()
            .map_err(|e| anyhow::anyhow!("bcs encode SuiSystemState: {e}"))?;

        rows.push(Row {
            epoch: epoch_info.epoch,
            value: epochs::start(
                epoch_info.protocol_version.unwrap_or(0),
                epoch_info.reference_gas_price.unwrap_or(0),
                epoch_info.start_timestamp_ms.unwrap_or(0),
                Some(epoch_info.start_checkpoint.unwrap_or(0)),
                system_state_bcs,
            ),
        });

        // End record for the prior epoch — skip on genesis where
        // there is no prior epoch.
        if epoch_info.epoch > 0 {
            rows.push(Row {
                epoch: checkpoint.summary.epoch(),
                value: epochs::end(
                    checkpoint.summary.timestamp_ms,
                    checkpoint.summary.sequence_number,
                ),
            });
        }

        Ok(rows)
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
            conn.batch.merge(cf, &U64Be(row.epoch), &row.value)?;
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
    async fn process_emits_nothing_for_non_epoch_boundary_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let rows = Epochs.process(&checkpoint).await.unwrap();
        assert!(rows.is_empty());
    }
}
