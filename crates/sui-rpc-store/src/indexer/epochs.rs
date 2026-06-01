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

pub enum Row {
    /// Stage a partial start record for `epoch`.
    Start {
        epoch: u64,
        protocol_version: u64,
        reference_gas_price: u64,
        start_timestamp_ms: u64,
        start_checkpoint: u64,
        system_state_bcs: Option<Vec<u8>>,
    },
    /// Stage a partial end record for `epoch`.
    End {
        epoch: u64,
        end_timestamp_ms: u64,
        end_checkpoint: u64,
    },
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

        rows.push(Row::Start {
            epoch: epoch_info.epoch,
            protocol_version: epoch_info.protocol_version.unwrap_or(0),
            reference_gas_price: epoch_info.reference_gas_price.unwrap_or(0),
            start_timestamp_ms: epoch_info.start_timestamp_ms.unwrap_or(0),
            start_checkpoint: epoch_info.start_checkpoint.unwrap_or(0),
            system_state_bcs,
        });

        // End record for the prior epoch — skip on genesis where
        // there is no prior epoch.
        if epoch_info.epoch > 0
            && let (Some(start_timestamp_ms), Some(start_checkpoint)) =
                (epoch_info.start_timestamp_ms, epoch_info.start_checkpoint)
            && let Some(end_checkpoint) = start_checkpoint.checked_sub(1)
        {
            rows.push(Row::End {
                epoch: epoch_info.epoch - 1,
                end_timestamp_ms: start_timestamp_ms,
                end_checkpoint,
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
            match row {
                Row::Start {
                    epoch,
                    protocol_version,
                    reference_gas_price,
                    start_timestamp_ms,
                    start_checkpoint,
                    system_state_bcs,
                } => {
                    conn.batch.merge(
                        cf,
                        &U64Be(*epoch),
                        &epochs::start(
                            *protocol_version,
                            *reference_gas_price,
                            *start_timestamp_ms,
                            *start_checkpoint,
                            system_state_bcs.clone(),
                        ),
                    )?;
                }
                Row::End {
                    epoch,
                    end_timestamp_ms,
                    end_checkpoint,
                } => {
                    conn.batch.merge(
                        cf,
                        &U64Be(*epoch),
                        &epochs::end(*end_timestamp_ms, *end_checkpoint),
                    )?;
                }
            }
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
