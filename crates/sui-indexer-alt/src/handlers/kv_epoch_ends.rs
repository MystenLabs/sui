// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    db,
    pipeline::{concurrent::Handler, Processor},
};
use sui_types::{
    event::SystemEpochInfoEvent,
    full_checkpoint_content::CheckpointData,
    transaction::{TransactionDataAPI, TransactionKind},
};

use crate::{models::epochs::StoredEpochEnd, schema::kv_epoch_ends};

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
}
