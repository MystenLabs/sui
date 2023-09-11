// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::{
    base_types::VerifiedExecutionData,
    effects::{TransactionEffects, TransactionEffectsAPI},
    gas::GasCostSummary,
    messages_checkpoint::{
        CheckpointContents, CheckpointSummary, EndOfEpochData, VerifiedCheckpoint,
    },
    transaction::VerifiedTransaction,
};

use super::CommitteeWithKeys;

#[derive(Debug)]
pub struct CheckpointBuilder {
    previous_checkpoint: VerifiedCheckpoint,
    transactions: Vec<VerifiedExecutionData>,
    epoch_rolling_gas_cost_summary: GasCostSummary,
    epoch: u64,
}

impl CheckpointBuilder {
    pub fn new(previous_checkpoint: VerifiedCheckpoint) -> Self {
        let epoch_rolling_gas_cost_summary =
            previous_checkpoint.epoch_rolling_gas_cost_summary.clone();
        let epoch = previous_checkpoint.epoch;

        Self {
            previous_checkpoint,
            transactions: Vec::new(),
            epoch_rolling_gas_cost_summary,
            epoch,
        }
    }

    pub fn epoch_rolling_gas_cost_summary(&self) -> &GasCostSummary {
        &self.epoch_rolling_gas_cost_summary
    }

    pub fn push_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
    ) {
        self.epoch_rolling_gas_cost_summary += effects.gas_cost_summary();

        self.transactions
            .push(VerifiedExecutionData::new(transaction, effects))
    }

    /// Builds a checkpoint using internally buffered transactions.
    pub fn build(
        &mut self,
        committee: &CommitteeWithKeys<'_>,
        timestamp_ms: u64,
    ) -> (VerifiedCheckpoint, CheckpointContents) {
        self.build_internal(committee, timestamp_ms, None)
    }

    pub fn build_end_of_epoch(
        &mut self,
        committee: &CommitteeWithKeys<'_>,
        timestamp_ms: u64,
        new_epoch: u64,
        end_of_epoch_data: EndOfEpochData,
    ) -> (VerifiedCheckpoint, CheckpointContents) {
        self.build_internal(
            committee,
            timestamp_ms,
            Some((new_epoch, end_of_epoch_data)),
        )
    }

    fn build_internal(
        &mut self,
        committee: &CommitteeWithKeys<'_>,
        timestamp_ms: u64,
        new_epoch_data: Option<(u64, EndOfEpochData)>,
    ) -> (VerifiedCheckpoint, CheckpointContents) {
        let contents =
            CheckpointContents::new_with_causally_ordered_execution_data(self.transactions.iter());
        self.transactions.clear();

        let (epoch, epoch_rolling_gas_cost_summary, end_of_epoch_data) =
            if let Some((next_epoch, end_of_epoch_data)) = new_epoch_data {
                let epoch = std::mem::replace(&mut self.epoch, next_epoch);
                assert_eq!(next_epoch, epoch + 1);
                let epoch_rolling_gas_cost_summary =
                    std::mem::take(&mut self.epoch_rolling_gas_cost_summary);

                (
                    epoch,
                    epoch_rolling_gas_cost_summary,
                    Some(end_of_epoch_data),
                )
            } else {
                (
                    self.epoch,
                    self.epoch_rolling_gas_cost_summary.clone(),
                    None,
                )
            };

        let summary = CheckpointSummary {
            epoch,
            sequence_number: self.previous_checkpoint.sequence_number.saturating_add(1),
            network_total_transactions: self.previous_checkpoint.network_total_transactions
                + contents.size() as u64,
            content_digest: *contents.digest(),
            previous_digest: Some(*self.previous_checkpoint.digest()),
            epoch_rolling_gas_cost_summary,
            end_of_epoch_data,
            timestamp_ms,
            version_specific_data: Vec::new(),
            checkpoint_commitments: Default::default(),
        };

        let checkpoint = committee.create_certified_checkpoint(summary);

        self.previous_checkpoint = checkpoint.clone();
        (checkpoint, contents)
    }
}
