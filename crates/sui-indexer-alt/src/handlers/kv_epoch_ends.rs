// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    models::cp_sequence_numbers::epoch_interval,
    pipeline::{concurrent::Handler, Processor},
};
use sui_indexer_alt_schema::{epochs::StoredEpochEnd, schema::kv_epoch_ends};
use sui_pg_db as db;
use sui_types::{
    event::SystemEpochInfoEvent,
    full_checkpoint_content::CheckpointData,
    transaction::{TransactionDataAPI, TransactionKind},
};

#[derive(Default)]
pub(crate) struct KvEpochEnds;

impl Processor for KvEpochEnds {
    const NAME: &'static str = "kv_epoch_ends";

    type Value = StoredEpochEnd;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            checkpoint_summary,
            transactions,
            ..
        } = checkpoint.as_ref();

        let Some(end_of_epoch) = checkpoint_summary.end_of_epoch_data.as_ref() else {
            return Ok(vec![]);
        };

        let Some(transaction) = transactions.iter().find(|tx| {
            matches!(
                tx.transaction.intent_message().value.kind(),
                TransactionKind::ChangeEpoch(_) | TransactionKind::EndOfEpochTransaction(_)
            )
        }) else {
            bail!(
                "Failed to get end of epoch transaction in checkpoint {} with EndOfEpochData",
                checkpoint_summary.sequence_number,
            );
        };

        if let Some(SystemEpochInfoEvent {
            total_stake,
            storage_fund_reinvestment,
            storage_charge,
            storage_rebate,
            storage_fund_balance,
            stake_subsidy_amount,
            total_gas_fees,
            total_stake_rewards_distributed,
            leftover_storage_fund_inflow,
            ..
        }) = transaction
            .events
            .iter()
            .flat_map(|events| &events.data)
            .find_map(|event| {
                event
                    .is_system_epoch_info_event()
                    .then(|| bcs::from_bytes(&event.contents))
            })
            .transpose()
            .context("Failed to deserialize SystemEpochInfoEvent")?
        {
            Ok(vec![StoredEpochEnd {
                epoch: checkpoint_summary.epoch as i64,
                cp_hi: checkpoint_summary.sequence_number as i64 + 1,
                tx_hi: checkpoint_summary.network_total_transactions as i64,
                end_timestamp_ms: checkpoint_summary.timestamp_ms as i64,

                safe_mode: false,

                total_stake: Some(total_stake as i64),
                storage_fund_balance: Some(storage_fund_balance as i64),
                storage_fund_reinvestment: Some(storage_fund_reinvestment as i64),
                storage_charge: Some(storage_charge as i64),
                storage_rebate: Some(storage_rebate as i64),
                stake_subsidy_amount: Some(stake_subsidy_amount as i64),
                total_gas_fees: Some(total_gas_fees as i64),
                total_stake_rewards_distributed: Some(total_stake_rewards_distributed as i64),
                leftover_storage_fund_inflow: Some(leftover_storage_fund_inflow as i64),

                epoch_commitments: bcs::to_bytes(&end_of_epoch.epoch_commitments)
                    .context("Failed to serialize EpochCommitment-s")?,
            }])
        } else {
            Ok(vec![StoredEpochEnd {
                epoch: checkpoint_summary.epoch as i64,
                cp_hi: checkpoint_summary.sequence_number as i64 + 1,
                tx_hi: checkpoint_summary.network_total_transactions as i64,
                end_timestamp_ms: checkpoint_summary.timestamp_ms as i64,

                safe_mode: true,

                total_stake: None,
                storage_fund_balance: None,
                storage_fund_reinvestment: None,
                storage_charge: None,
                storage_rebate: None,
                stake_subsidy_amount: None,
                total_gas_fees: None,
                total_stake_rewards_distributed: None,
                leftover_storage_fund_inflow: None,

                epoch_commitments: bcs::to_bytes(&end_of_epoch.epoch_commitments)
                    .context("Failed to serialize EpochCommitment-s")?,
            }])
        }
    }
}

#[async_trait::async_trait]
impl Handler for KvEpochEnds {
    const MIN_EAGER_ROWS: usize = 1;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(kv_epoch_ends::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut db::Connection<'_>,
    ) -> Result<usize> {
        let Range {
            start: from_epoch,
            end: to_epoch,
        } = epoch_interval(conn, from..to_exclusive).await?;
        if from_epoch < to_epoch {
            let filter = kv_epoch_ends::table
                .filter(kv_epoch_ends::epoch.between(from_epoch as i64, to_epoch as i64 - 1));
            Ok(diesel::delete(filter).execute(conn).await?)
        } else {
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use sui_indexer_alt_framework::{handlers::cp_sequence_numbers::CpSequenceNumbers, Indexer};
    use sui_indexer_alt_schema::MIGRATIONS;
    use sui_pg_db::Connection;
    use sui_types::test_checkpoint_data_builder::TestCheckpointDataBuilder;

    async fn get_all_kv_epoch_ends(conn: &mut Connection<'_>) -> Result<Vec<i64>> {
        let result = kv_epoch_ends::table
            .select(kv_epoch_ends::epoch)
            .load(conn)
            .await?;
        Ok(result)
    }

    /// Epoch end table retention must be larger than one epoch's worth of checkpoints - otherwise
    /// we'll prune the entry for the previous epoch at boundary shortly after writing it.
    #[tokio::test]
    pub async fn test_kv_epoch_ends_same_epoch() -> () {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let mut builder = TestCheckpointDataBuilder::new(0);
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvEpochEnds.process(&checkpoint).unwrap();
        KvEpochEnds::commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 0);
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvEpochEnds.process(&checkpoint).unwrap();
        KvEpochEnds::commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 0);
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let checkpoint = Arc::new(builder.advance_epoch(false));
        let values = KvEpochEnds.process(&checkpoint).unwrap();
        KvEpochEnds::commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 1);
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvEpochEnds.process(&checkpoint).unwrap();
        KvEpochEnds::commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 0);
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvEpochEnds.process(&checkpoint).unwrap();
        KvEpochEnds::commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 0);
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let epochs = get_all_kv_epoch_ends(&mut conn).await.unwrap();
        assert_eq!(epochs, vec![0]);

        let rows_pruned = KvEpochEnds.prune(0, 4, &mut conn).await.unwrap();
        let epochs = get_all_kv_epoch_ends(&mut conn).await.unwrap();
        assert_eq!(epochs.len(), 0);
        assert_eq!(rows_pruned, 1);
    }

    #[tokio::test]
    pub async fn test_kv_epoch_ends_advance_multiple_epochs() -> () {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let mut builder = TestCheckpointDataBuilder::new(0);
        let checkpoint = Arc::new(builder.advance_epoch(false));
        let values = KvEpochEnds.process(&checkpoint).unwrap();
        KvEpochEnds::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let checkpoint = Arc::new(builder.advance_epoch(false));
        let values = KvEpochEnds.process(&checkpoint).unwrap();
        KvEpochEnds::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let checkpoint = Arc::new(builder.advance_epoch(false));
        let values = KvEpochEnds.process(&checkpoint).unwrap();
        KvEpochEnds::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let epochs = get_all_kv_epoch_ends(&mut conn).await.unwrap();
        assert_eq!(epochs, vec![0, 1, 2]);

        let rows_pruned = KvEpochEnds.prune(0, 2, &mut conn).await.unwrap();
        let epochs = get_all_kv_epoch_ends(&mut conn).await.unwrap();
        assert_eq!(epochs, vec![2]);
        assert_eq!(rows_pruned, 2);
    }
}
