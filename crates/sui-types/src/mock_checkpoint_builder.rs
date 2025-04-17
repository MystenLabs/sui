// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, VerifiedExecutionData};
use crate::committee::Committee;
use crate::crypto::{AuthoritySignInfo, AuthoritySignature, SuiAuthoritySignature};
use crate::effects::{TransactionEffects, TransactionEffectsAPI};
use crate::gas::GasCostSummary;
use crate::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary,
    CheckpointVersionSpecificData, EndOfEpochData, FullCheckpointContents, VerifiedCheckpoint,
    VerifiedCheckpointContents,
};
use crate::transaction::VerifiedTransaction;
use fastcrypto::traits::Signer;
use std::mem;

pub trait ValidatorKeypairProvider {
    fn get_validator_key(&self, name: &AuthorityName) -> &dyn Signer<AuthoritySignature>;
    fn get_committee(&self) -> &Committee;
}

/// A utility to build consecutive checkpoints by adding transactions to the checkpoint builder.
/// It's mostly used by simulations, tests and benchmarks.
#[derive(Debug)]
pub struct MockCheckpointBuilder {
    previous_checkpoint: Option<VerifiedCheckpoint>,
    transactions: Vec<VerifiedExecutionData>,
    epoch_rolling_gas_cost_summary: GasCostSummary,
    epoch: u64,
}

impl MockCheckpointBuilder {
    pub fn new(previous_checkpoint: VerifiedCheckpoint) -> Self {
        let epoch_rolling_gas_cost_summary =
            previous_checkpoint.epoch_rolling_gas_cost_summary.clone();
        let epoch = previous_checkpoint.epoch;

        Self {
            previous_checkpoint: Some(previous_checkpoint),
            transactions: Vec::new(),
            epoch_rolling_gas_cost_summary,
            epoch,
        }
    }

    pub fn size(&self) -> usize {
        self.transactions.len()
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

    /// Override the next checkpoint number to generate.
    /// This can be useful to generate checkpoints with specific sequence numbers.
    pub fn override_next_checkpoint_number(
        &mut self,
        checkpoint_number: u64,
        validator_keys: &impl ValidatorKeypairProvider,
    ) {
        if checkpoint_number > 0 {
            let mut summary = self.previous_checkpoint.as_ref().unwrap().data().clone();
            summary.sequence_number = checkpoint_number - 1;
            let checkpoint = Self::create_certified_checkpoint(validator_keys, summary);
            self.previous_checkpoint = Some(checkpoint);
        } else {
            self.previous_checkpoint = None;
        }
    }

    /// Builds a checkpoint using internally buffered transactions.
    pub fn build(
        &mut self,
        validator_keys: &impl ValidatorKeypairProvider,
        timestamp_ms: u64,
    ) -> (
        VerifiedCheckpoint,
        CheckpointContents,
        VerifiedCheckpointContents,
    ) {
        self.build_internal(validator_keys, timestamp_ms, None)
    }

    pub fn build_end_of_epoch(
        &mut self,
        validator_keys: &impl ValidatorKeypairProvider,
        timestamp_ms: u64,
        new_epoch: u64,
        end_of_epoch_data: EndOfEpochData,
    ) -> (
        VerifiedCheckpoint,
        CheckpointContents,
        VerifiedCheckpointContents,
    ) {
        self.build_internal(
            validator_keys,
            timestamp_ms,
            Some((new_epoch, end_of_epoch_data)),
        )
    }

    fn build_internal(
        &mut self,
        validator_keys: &impl ValidatorKeypairProvider,
        timestamp_ms: u64,
        new_epoch_data: Option<(u64, EndOfEpochData)>,
    ) -> (
        VerifiedCheckpoint,
        CheckpointContents,
        VerifiedCheckpointContents,
    ) {
        let contents =
            CheckpointContents::new_with_causally_ordered_execution_data(self.transactions.iter());
        let full_contents = VerifiedCheckpointContents::new_unchecked(
            FullCheckpointContents::new_with_causally_ordered_transactions(
                mem::take(&mut self.transactions)
                    .into_iter()
                    .map(|e| e.into_inner()),
            ),
        );

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
            sequence_number: self
                .previous_checkpoint
                .as_ref()
                .map(|c| c.sequence_number + 1)
                .unwrap_or_default(),
            network_total_transactions: self
                .previous_checkpoint
                .as_ref()
                .map(|c| c.network_total_transactions)
                .unwrap_or_default()
                + contents.size() as u64,
            content_digest: *contents.digest(),
            previous_digest: self.previous_checkpoint.as_ref().map(|c| *c.digest()),
            epoch_rolling_gas_cost_summary,
            end_of_epoch_data,
            timestamp_ms,
            version_specific_data: bcs::to_bytes(&CheckpointVersionSpecificData::empty_for_tests())
                .unwrap(),
            checkpoint_commitments: Default::default(),
        };

        let checkpoint = Self::create_certified_checkpoint(validator_keys, summary);
        self.previous_checkpoint = Some(checkpoint.clone());
        (checkpoint, contents, full_contents)
    }

    fn create_certified_checkpoint(
        validator_keys: &impl ValidatorKeypairProvider,
        checkpoint: CheckpointSummary,
    ) -> VerifiedCheckpoint {
        let signatures = validator_keys
            .get_committee()
            .voting_rights
            .iter()
            .map(|(name, _)| {
                let intent_msg = shared_crypto::intent::IntentMessage::new(
                    shared_crypto::intent::Intent::sui_app(
                        shared_crypto::intent::IntentScope::CheckpointSummary,
                    ),
                    &checkpoint,
                );
                let key = validator_keys.get_validator_key(name);
                let signature = AuthoritySignature::new_secure(&intent_msg, &checkpoint.epoch, key);
                AuthoritySignInfo {
                    epoch: checkpoint.epoch,
                    authority: *name,
                    signature,
                }
            })
            .collect();

        let checkpoint_cert =
            CertifiedCheckpointSummary::new(checkpoint, signatures, validator_keys.get_committee())
                .unwrap();
        VerifiedCheckpoint::new_unchecked(checkpoint_cert)
    }
}
