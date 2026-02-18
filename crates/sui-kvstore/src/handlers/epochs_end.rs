// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use anyhow::bail;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::event::SystemEpochInfoEvent;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::TransactionKind;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::tables;

/// Pipeline that writes epoch end data to BigTable.
pub struct EpochEndPipeline;

#[async_trait::async_trait]
impl Processor for EpochEndPipeline {
    const NAME: &'static str = "kvstore_epochs_end";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let summary = &checkpoint.summary;

        let Some(end_of_epoch) = summary.end_of_epoch_data.as_ref() else {
            return Ok(vec![]);
        };

        let Some(transaction) = checkpoint.transactions.iter().find(|tx| {
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

        let epoch_id = summary.epoch;
        let end_timestamp_ms = summary.timestamp_ms;
        let end_checkpoint = summary.sequence_number;
        let cp_hi = summary.sequence_number + 1;
        let tx_hi = summary.network_total_transactions;

        let epoch_commitments = bcs::to_bytes(&end_of_epoch.epoch_commitments)
            .context("Failed to serialize EpochCommitment-s")?;

        let (
            safe_mode,
            total_stake,
            storage_fund_balance,
            storage_fund_reinvestment,
            storage_charge,
            storage_rebate,
            stake_subsidy_amount,
            total_gas_fees,
            total_stake_rewards_distributed,
            leftover_storage_fund_inflow,
        ) = if let Some(SystemEpochInfoEvent {
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
            (
                false,
                Some(total_stake),
                Some(storage_fund_balance),
                Some(storage_fund_reinvestment),
                Some(storage_charge),
                Some(storage_rebate),
                Some(stake_subsidy_amount),
                Some(total_gas_fees),
                Some(total_stake_rewards_distributed),
                Some(leftover_storage_fund_inflow),
            )
        } else {
            (true, None, None, None, None, None, None, None, None, None)
        };

        let entry = tables::make_entry(
            tables::epochs::encode_key(epoch_id),
            tables::epochs::encode_end(
                end_timestamp_ms,
                end_checkpoint,
                cp_hi,
                tx_hi,
                safe_mode,
                total_stake,
                storage_fund_balance,
                storage_fund_reinvestment,
                storage_charge,
                storage_rebate,
                stake_subsidy_amount,
                total_gas_fees,
                total_stake_rewards_distributed,
                leftover_storage_fund_inflow,
                &epoch_commitments,
            ),
            Some(end_timestamp_ms),
        );

        Ok(vec![entry])
    }
}

impl BigTableProcessor for EpochEndPipeline {
    const TABLE: &'static str = tables::epochs::NAME;
    const FANOUT: usize = 100;
    const MIN_EAGER_ROWS: usize = 1;
}
