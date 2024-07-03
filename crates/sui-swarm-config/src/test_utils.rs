// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::network_config::NetworkConfig;
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};
use std::collections::HashMap;
use sui_types::{
    base_types::AuthorityName,
    committee::{Committee, EpochId, StakeUnit},
    crypto::{
        AuthorityKeyPair, AuthoritySignInfo, AuthoritySignature, KeypairTraits,
        SuiAuthoritySignature,
    },
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointDigest, CheckpointSequenceNumber, CheckpointSummary,
        CheckpointVersionSpecificData, EndOfEpochData, FullCheckpointContents, VerifiedCheckpoint,
        VerifiedCheckpointContents,
    },
};

pub struct CommitteeFixture {
    epoch: EpochId,
    validators: HashMap<AuthorityName, (AuthorityKeyPair, StakeUnit)>,
    committee: Committee,
}

type MakeCheckpointResults = (
    Vec<VerifiedCheckpoint>,
    Vec<VerifiedCheckpointContents>,
    HashMap<CheckpointSequenceNumber, CheckpointDigest>,
    HashMap<CheckpointDigest, VerifiedCheckpoint>,
);

impl CommitteeFixture {
    pub fn generate<R: ::rand::RngCore + ::rand::CryptoRng>(
        mut rng: R,
        epoch: EpochId,
        committee_size: usize,
    ) -> Self {
        let validators = (0..committee_size)
            .map(|_| sui_types::crypto::get_key_pair_from_rng::<AuthorityKeyPair, _>(&mut rng).1)
            .map(|keypair| (keypair.public().into(), (keypair, 1)))
            .collect::<HashMap<_, _>>();

        let committee = Committee::new_for_testing_with_normalized_voting_power(
            epoch,
            validators
                .iter()
                .map(|(name, (_, stake))| (*name, *stake))
                .collect(),
        );

        Self {
            epoch,
            validators,
            committee,
        }
    }

    pub fn from_network_config(network_config: &NetworkConfig) -> Self {
        let committee = network_config.genesis.committee().unwrap();
        Self {
            epoch: committee.epoch,
            validators: committee
                .members()
                .map(|(name, stake)| {
                    (
                        *name,
                        (
                            network_config
                                .validator_configs()
                                .iter()
                                .find(|config| config.protocol_public_key() == *name)
                                .unwrap()
                                .protocol_key_pair()
                                .copy(),
                            *stake,
                        ),
                    )
                })
                .collect(),
            committee,
        }
    }

    pub fn committee(&self) -> &Committee {
        &self.committee
    }

    fn create_root_checkpoint(&self) -> (VerifiedCheckpoint, VerifiedCheckpointContents) {
        assert_eq!(self.epoch, 0, "root checkpoint must be epoch 0");
        let checkpoint = CheckpointSummary {
            epoch: 0,
            sequence_number: 0,
            network_total_transactions: 0,
            content_digest: *empty_contents()
                .into_inner()
                .into_checkpoint_contents()
                .digest(),
            previous_digest: None,
            epoch_rolling_gas_cost_summary: Default::default(),
            end_of_epoch_data: None,
            timestamp_ms: 0,
            version_specific_data: bcs::to_bytes(&CheckpointVersionSpecificData::empty_for_tests())
                .unwrap(),
            checkpoint_commitments: Default::default(),
        };

        (
            self.create_certified_checkpoint(checkpoint),
            empty_contents(),
        )
    }

    fn create_certified_checkpoint(&self, checkpoint: CheckpointSummary) -> VerifiedCheckpoint {
        let signatures = self
            .validators
            .iter()
            .map(|(name, (key, _))| {
                let intent_msg = IntentMessage::new(
                    Intent::sui_app(IntentScope::CheckpointSummary),
                    checkpoint.clone(),
                );
                let signature = AuthoritySignature::new_secure(&intent_msg, &checkpoint.epoch, key);
                AuthoritySignInfo {
                    epoch: checkpoint.epoch,
                    authority: *name,
                    signature,
                }
            })
            .collect();

        let checkpoint = CertifiedCheckpointSummary::new(checkpoint, signatures, self.committee())
            .unwrap()
            .try_into_verified(self.committee())
            .unwrap();

        checkpoint
    }

    pub fn make_random_checkpoints(
        &self,
        number_of_checkpoints: usize,
        previous_checkpoint: Option<VerifiedCheckpoint>,
    ) -> MakeCheckpointResults {
        self.make_checkpoints(number_of_checkpoints, previous_checkpoint, random_contents)
    }

    pub fn make_empty_checkpoints(
        &self,
        number_of_checkpoints: usize,
        previous_checkpoint: Option<VerifiedCheckpoint>,
    ) -> MakeCheckpointResults {
        self.make_checkpoints(number_of_checkpoints, previous_checkpoint, empty_contents)
    }

    fn make_checkpoints<F: Fn() -> VerifiedCheckpointContents>(
        &self,
        number_of_checkpoints: usize,
        previous_checkpoint: Option<VerifiedCheckpoint>,
        content_generator: F,
    ) -> MakeCheckpointResults {
        // Only skip the first one if it was supplied
        let skip = previous_checkpoint.is_some() as usize;
        let first = previous_checkpoint
            .map(|c| (c, empty_contents()))
            .unwrap_or_else(|| self.create_root_checkpoint());

        let (ordered_checkpoints, contents): (Vec<_>, Vec<_>) =
            std::iter::successors(Some(first), |prev| {
                let contents = content_generator();
                let contents_digest = *contents
                    .clone()
                    .into_inner()
                    .into_checkpoint_contents()
                    .digest();
                let summary = CheckpointSummary {
                    epoch: self.epoch,
                    sequence_number: prev.0.sequence_number + 1,
                    network_total_transactions: prev.0.network_total_transactions
                        + contents.num_of_transactions() as u64,
                    content_digest: contents_digest,
                    previous_digest: Some(*prev.0.digest()),
                    epoch_rolling_gas_cost_summary: Default::default(),
                    end_of_epoch_data: None,
                    timestamp_ms: 0,
                    version_specific_data: bcs::to_bytes(
                        &CheckpointVersionSpecificData::empty_for_tests(),
                    )
                    .unwrap(),
                    checkpoint_commitments: Default::default(),
                };

                let checkpoint = self.create_certified_checkpoint(summary);

                Some((checkpoint, contents))
            })
            .skip(skip)
            .take(number_of_checkpoints)
            .unzip();

        let (sequence_number_to_digest, checkpoints) = ordered_checkpoints
            .iter()
            .cloned()
            .map(|checkpoint| {
                let digest = *checkpoint.digest();
                ((checkpoint.sequence_number, digest), (digest, checkpoint))
            })
            .unzip();

        (
            ordered_checkpoints,
            contents,
            sequence_number_to_digest,
            checkpoints,
        )
    }

    pub fn make_end_of_epoch_checkpoint(
        &self,
        previous_checkpoint: VerifiedCheckpoint,
        end_of_epoch_data: Option<EndOfEpochData>,
    ) -> (
        CheckpointSequenceNumber,
        CheckpointDigest,
        VerifiedCheckpoint,
    ) {
        let summary = CheckpointSummary {
            epoch: self.epoch,
            sequence_number: previous_checkpoint.sequence_number + 1,
            network_total_transactions: 0,
            content_digest: *empty_contents()
                .into_inner()
                .into_checkpoint_contents()
                .digest(),
            previous_digest: Some(*previous_checkpoint.digest()),
            epoch_rolling_gas_cost_summary: Default::default(),
            end_of_epoch_data,
            timestamp_ms: 0,
            version_specific_data: bcs::to_bytes(&CheckpointVersionSpecificData::empty_for_tests())
                .unwrap(),
            checkpoint_commitments: Default::default(),
        };

        let checkpoint = self.create_certified_checkpoint(summary);

        (checkpoint.sequence_number, *checkpoint.digest(), checkpoint)
    }
}

pub fn empty_contents() -> VerifiedCheckpointContents {
    VerifiedCheckpointContents::new_unchecked(
        FullCheckpointContents::new_with_causally_ordered_transactions(std::iter::empty()),
    )
}

pub fn random_contents() -> VerifiedCheckpointContents {
    VerifiedCheckpointContents::new_unchecked(FullCheckpointContents::random_for_testing())
}
