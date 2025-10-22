// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{concurrent::Handler, Processor},
    postgres::{Connection, Db},
    types::{
        event::SystemEpochInfoEvent,
        full_checkpoint_content::Checkpoint,
        transaction::{TransactionDataAPI, TransactionKind},
    },
};
use sui_indexer_alt_schema::{epochs::StoredEpochEnd, schema::kv_epoch_ends};

use crate::handlers::cp_sequence_numbers::epoch_interval;
use async_trait::async_trait;

pub(crate) struct KvEpochEnds;

#[async_trait]
impl Processor for KvEpochEnds {
    const NAME: &'static str = "kv_epoch_ends";

    type Value = StoredEpochEnd;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let Checkpoint {
            summary,
            transactions,
            ..
        } = checkpoint.as_ref();

        let Some(end_of_epoch) = summary.end_of_epoch_data.as_ref() else {
            return Ok(vec![]);
        };

        let Some(transaction) = transactions.iter().find(|tx| {
            matches!(
                tx.transaction.kind(),
                TransactionKind::ChangeEpoch(_) | TransactionKind::EndOfEpochTransaction(_)
            )
        }) else {
            bail!(
                "Failed to get end of epoch transaction in checkpoint {} with EndOfEpochData",
                summary.sequence_number,
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
                epoch: summary.epoch as i64,
                cp_hi: summary.sequence_number as i64 + 1,
                tx_hi: summary.network_total_transactions as i64,
                end_timestamp_ms: summary.timestamp_ms as i64,

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
                epoch: summary.epoch as i64,
                cp_hi: summary.sequence_number as i64 + 1,
                tx_hi: summary.network_total_transactions as i64,
                end_timestamp_ms: summary.timestamp_ms as i64,

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

#[async_trait]
impl Handler for KvEpochEnds {
    type Store = Db;
    type Batch = Vec<Self::Value>;

    const MIN_EAGER_ROWS: usize = 1;

    async fn commit<'a>(&self, batch: &Self::Batch, conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(kv_epoch_ends::table)
            .values(batch)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
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
    use sui_indexer_alt_framework::types::test_checkpoint_data_builder::AdvanceEpochConfig;
    use sui_indexer_alt_framework::{
        types::test_checkpoint_data_builder::TestCheckpointDataBuilder, Indexer,
    };
    use sui_indexer_alt_schema::MIGRATIONS;

    use crate::handlers::cp_sequence_numbers::CpSequenceNumbers;

    async fn get_all_kv_epoch_ends(conn: &mut Connection<'_>) -> Result<Vec<StoredEpochEnd>> {
        let result = kv_epoch_ends::table
            .order_by(kv_epoch_ends::epoch.asc())
            .load(conn)
            .await?;
        Ok(result)
    }

    async fn get_epoch_num_of_all_kv_epoch_ends(conn: &mut Connection<'_>) -> Result<Vec<i64>> {
        let epochs = get_all_kv_epoch_ends(conn).await?;
        Ok(epochs.iter().map(|e| e.epoch).collect())
    }

    #[tokio::test]
    pub async fn test_kv_epoch_ends_safe_mode() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let mut builder = TestCheckpointDataBuilder::new(0);
        let checkpoint: Arc<Checkpoint> = Arc::new(
            builder
                .advance_epoch(AdvanceEpochConfig {
                    safe_mode: true,
                    ..Default::default()
                })
                .into(),
        );
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();

        let epochs = get_all_kv_epoch_ends(&mut conn).await.unwrap();
        assert_eq!(epochs.len(), 1);
        assert!(epochs[0].safe_mode);
        assert_eq!(epochs[0].total_gas_fees, None);

        let checkpoint: Arc<Checkpoint> =
            Arc::new(builder.advance_epoch(Default::default()).into());
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();

        let epochs = get_all_kv_epoch_ends(&mut conn).await.unwrap();
        assert_eq!(epochs.len(), 2);
        assert!(!epochs[1].safe_mode);
        assert_eq!(epochs[1].total_gas_fees, Some(0));
    }

    #[tokio::test]
    pub async fn test_kv_epoch_ends_same_epoch() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        // Test that there is nothing to commit while we haven't reached epoch end.
        let mut builder = TestCheckpointDataBuilder::new(0);
        let checkpoint = Arc::new(builder.build_checkpoint().into());
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 0);
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers.commit(&values, &mut conn).await.unwrap();

        let checkpoint = Arc::new(builder.build_checkpoint().into());
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 0);
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers.commit(&values, &mut conn).await.unwrap();

        // When the advance epoch tx is detected, there should be an entry to commit.
        let checkpoint: Arc<Checkpoint> =
            Arc::new(builder.advance_epoch(Default::default()).into());
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 1);
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers.commit(&values, &mut conn).await.unwrap();

        // Afterwards, kv_epoch_ends should not have anything to commit until the next advance epoch
        // tx.
        let checkpoint = Arc::new(builder.build_checkpoint().into());
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 0);
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers.commit(&values, &mut conn).await.unwrap();

        let checkpoint = Arc::new(builder.build_checkpoint().into());
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();
        assert_eq!(values.len(), 0);
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers.commit(&values, &mut conn).await.unwrap();

        let epochs = get_epoch_num_of_all_kv_epoch_ends(&mut conn).await.unwrap();
        assert_eq!(epochs, vec![0]);

        let rows_pruned = KvEpochEnds.prune(0, 4, &mut conn).await.unwrap();
        let epochs = get_epoch_num_of_all_kv_epoch_ends(&mut conn).await.unwrap();
        assert_eq!(epochs.len(), 0);
        assert_eq!(rows_pruned, 1);
    }

    #[tokio::test]
    pub async fn test_kv_epoch_ends_advance_multiple_epochs() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        // Advance epoch three times, 0, 1, 2
        let mut builder = TestCheckpointDataBuilder::new(0);
        let checkpoint: Arc<Checkpoint> =
            Arc::new(builder.advance_epoch(Default::default()).into());
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers.commit(&values, &mut conn).await.unwrap();

        let checkpoint: Arc<Checkpoint> =
            Arc::new(builder.advance_epoch(Default::default()).into());
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers.commit(&values, &mut conn).await.unwrap();

        let checkpoint: Arc<Checkpoint> =
            Arc::new(builder.advance_epoch(Default::default()).into());
        let values = KvEpochEnds.process(&checkpoint).await.unwrap();
        KvEpochEnds.commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers.commit(&values, &mut conn).await.unwrap();

        let epochs = get_epoch_num_of_all_kv_epoch_ends(&mut conn).await.unwrap();
        assert_eq!(epochs, vec![0, 1, 2]);

        let rows_pruned = KvEpochEnds.prune(0, 2, &mut conn).await.unwrap();
        let epochs = get_epoch_num_of_all_kv_epoch_ends(&mut conn).await.unwrap();
        // Only epoch 2 remains, after pruning 0 and 1.
        assert_eq!(epochs, vec![2]);
        assert_eq!(rows_pruned, 2);
    }
}
