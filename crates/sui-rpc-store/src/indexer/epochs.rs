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
//!    the prior epoch, built from the boundary checkpoint (the prior
//!    epoch's last, which carries `end_of_epoch_data`): its
//!    timestamp and sequence number, its
//!    `network_total_transactions` as `tx_hi`, the end-of-epoch
//!    commitments, and — unless the epoch ended in safe mode — the
//!    gas and stake counters from the change-epoch transaction's
//!    `SystemEpochInfoEvent`. Mirrors the postgres `kv_epoch_ends`
//!    handler.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::event::SystemEpochInfoEvent;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::schema::epochs;
use crate::schema::primitives::U64Be;

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
                value: epochs::end(epoch_end(checkpoint)?),
            });
        }

        Ok(rows)
    }
}

/// Build the end-of-epoch record for the epoch ending at
/// `checkpoint` — the boundary checkpoint carrying
/// `end_of_epoch_data`.
///
/// Mirrors the postgres `kv_epoch_ends` handler: the gas and stake
/// counters come from the `SystemEpochInfoEvent` emitted by the
/// change-epoch transaction. An epoch that ends in safe mode emits
/// no such event, so those counters stay `None` and `safe_mode` is
/// recorded as `true`.
fn epoch_end(checkpoint: &Checkpoint) -> anyhow::Result<epochs::EpochEnd> {
    let summary = &checkpoint.summary;

    let epoch_commitments = summary
        .end_of_epoch_data
        .as_ref()
        .map(|data| bcs::to_bytes(&data.epoch_commitments))
        .transpose()
        .map_err(|e| anyhow::anyhow!("bcs encode epoch_commitments: {e}"))?
        .unwrap_or_default();

    // The `SystemEpochInfoEvent` is emitted only by the change-epoch
    // transaction, so scanning every transaction's events finds it
    // without having to identify that transaction by kind.
    let event: Option<SystemEpochInfoEvent> = checkpoint
        .transactions
        .iter()
        .filter_map(|tx| tx.events.as_ref())
        .flat_map(|events| &events.data)
        .find_map(|event| {
            event
                .is_system_epoch_info_event()
                .then(|| bcs::from_bytes(&event.contents))
        })
        .transpose()
        .map_err(|e| anyhow::anyhow!("bcs decode SystemEpochInfoEvent: {e}"))?;

    let mut end = epochs::EpochEnd {
        end_timestamp_ms: summary.timestamp_ms,
        end_checkpoint: summary.sequence_number,
        tx_hi: summary.network_total_transactions,
        safe_mode: event.is_none(),
        epoch_commitments,
        ..Default::default()
    };

    if let Some(e) = event {
        end.total_stake = Some(e.total_stake);
        end.storage_fund_balance = Some(e.storage_fund_balance);
        end.storage_fund_reinvestment = Some(e.storage_fund_reinvestment);
        end.storage_charge = Some(e.storage_charge);
        end.storage_rebate = Some(e.storage_rebate);
        end.stake_subsidy_amount = Some(e.stake_subsidy_amount);
        end.total_gas_fees = Some(e.total_gas_fees);
        end.total_stake_rewards_distributed = Some(e.total_stake_rewards_distributed);
        end.leftover_storage_fund_inflow = Some(e.leftover_storage_fund_inflow);
    }

    Ok(end)
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

    use sui_types::test_checkpoint_data_builder::AdvanceEpochConfig;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_emits_nothing_for_non_epoch_boundary_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let rows = Epochs.process(&checkpoint).await.unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn epoch_end_captures_system_epoch_info_event() {
        let mut builder = TestCheckpointBuilder::new(0);
        let checkpoint = builder.advance_epoch(AdvanceEpochConfig::default());

        let end = epoch_end(&checkpoint).unwrap();
        assert!(!end.safe_mode);
        // The builder emits a `SystemEpochInfoEvent` with default
        // (zero) counters, which we record as `Some(0)` — distinct
        // from the safe-mode `None`.
        assert_eq!(end.total_gas_fees, Some(0));
        assert_eq!(end.total_stake, Some(0));
        assert_eq!(end.storage_charge, Some(0));
        // Commitments are always recorded (BCS of the vec, never
        // empty bytes even for an empty vec).
        assert!(!end.epoch_commitments.is_empty());
    }

    #[test]
    fn epoch_end_safe_mode_leaves_counters_unset() {
        let mut builder = TestCheckpointBuilder::new(0);
        let checkpoint = builder.advance_epoch(AdvanceEpochConfig {
            safe_mode: true,
            ..Default::default()
        });

        let end = epoch_end(&checkpoint).unwrap();
        assert!(end.safe_mode);
        assert_eq!(end.total_gas_fees, None);
        assert_eq!(end.total_stake, None);
    }
}
