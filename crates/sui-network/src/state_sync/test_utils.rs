// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::{
    base_types::AuthorityName,
    committee::{Committee, EpochId, StakeUnit},
    crypto::{
        AuthorityKeyPair, AuthorityPublicKeyBytes, AuthoritySignInfo, AuthoritySignature,
        KeypairTraits, SuiAuthoritySignature,
    },
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointContents, CheckpointDigest, CheckpointSequenceNumber,
        CheckpointSummary, VerifiedCheckpoint,
    },
};

pub struct CommitteeFixture {
    epoch: EpochId,
    validators: HashMap<AuthorityName, (AuthorityKeyPair, StakeUnit)>,
    committee: Committee,
}

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

        let committee = Committee::new(
            epoch,
            validators
                .iter()
                .map(|(name, (_, stake))| (*name, *stake))
                .collect(),
        )
        .unwrap();

        Self {
            epoch,
            validators,
            committee,
        }
    }

    pub fn committee(&self) -> &Committee {
        &self.committee
    }

    fn create_root_checkpoint(&self) -> VerifiedCheckpoint {
        assert_eq!(self.epoch, 0, "root checkpoint must be epoch 0");
        let checkpoint = CheckpointSummary {
            epoch: 0,
            sequence_number: 0,
            network_total_transactions: 0,
            content_digest: empty_contents().digest(),
            previous_digest: None,
            epoch_rolling_gas_cost_summary: Default::default(),
            next_epoch_committee: None,
            root_state_digest: None,
            timestamp_ms: 0,
        };

        self.create_certified_checkpoint(checkpoint)
    }

    fn create_certified_checkpoint(&self, checkpoint: CheckpointSummary) -> VerifiedCheckpoint {
        let signatures = self
            .validators
            .iter()
            .map(|(name, (key, _))| {
                let signature = AuthoritySignature::new(&checkpoint, checkpoint.epoch, key);
                AuthoritySignInfo {
                    epoch: checkpoint.epoch,
                    authority: *name,
                    signature,
                }
            })
            .collect();

        let checkpoint = CertifiedCheckpointSummary {
            summary: checkpoint,
            auth_signature: AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(
                signatures,
                self.committee(),
            )
            .unwrap(),
        };

        let checkpoint = VerifiedCheckpoint::new(checkpoint, self.committee()).unwrap();

        checkpoint
    }

    pub fn make_checkpoints(
        &self,
        number_of_checkpoints: usize,
        previous_checkpoint: Option<VerifiedCheckpoint>,
    ) -> (
        Vec<VerifiedCheckpoint>,
        HashMap<CheckpointSequenceNumber, CheckpointDigest>,
        HashMap<CheckpointDigest, VerifiedCheckpoint>,
    ) {
        // Only skip the first one if it was supplied
        let skip = previous_checkpoint.is_some() as usize;
        let first = previous_checkpoint.unwrap_or_else(|| self.create_root_checkpoint());

        let ordered_checkpoints = std::iter::successors(Some(first), |prev| {
            let summary = CheckpointSummary {
                epoch: self.epoch,
                sequence_number: prev.summary.sequence_number + 1,
                network_total_transactions: 0,
                content_digest: empty_contents().digest(),
                previous_digest: Some(prev.summary.digest()),
                epoch_rolling_gas_cost_summary: Default::default(),
                next_epoch_committee: None,
                root_state_digest: None,
                timestamp_ms: 0,
            };

            let checkpoint = self.create_certified_checkpoint(summary);

            Some(checkpoint)
        })
        .skip(skip)
        .take(number_of_checkpoints)
        .collect::<Vec<_>>();

        let (sequence_number_to_digest, checkpoints) = ordered_checkpoints
            .iter()
            .cloned()
            .map(|checkpoint| {
                let digest = checkpoint.summary.digest();
                (
                    (checkpoint.summary.sequence_number, digest),
                    (digest, checkpoint),
                )
            })
            .unzip();

        (ordered_checkpoints, sequence_number_to_digest, checkpoints)
    }

    pub fn make_end_of_epoch_checkpoint(
        &self,
        previous_checkpoint: VerifiedCheckpoint,
        next_epoch_committee: Vec<(AuthorityPublicKeyBytes, u64)>,
    ) -> (
        CheckpointSequenceNumber,
        CheckpointDigest,
        VerifiedCheckpoint,
    ) {
        let summary = CheckpointSummary {
            epoch: self.epoch,
            sequence_number: previous_checkpoint.summary.sequence_number + 1,
            network_total_transactions: 0,
            content_digest: empty_contents().digest(),
            previous_digest: Some(previous_checkpoint.summary.digest()),
            epoch_rolling_gas_cost_summary: Default::default(),
            next_epoch_committee: Some(next_epoch_committee),
            // TODO(william): make non-None once we run this in consensus
            root_state_digest: None,
            timestamp_ms: 0,
        };

        let checkpoint = self.create_certified_checkpoint(summary);

        (
            checkpoint.summary.sequence_number,
            checkpoint.summary.digest(),
            checkpoint,
        )
    }
}

pub fn empty_contents() -> CheckpointContents {
    CheckpointContents::new_with_causally_ordered_transactions(std::iter::empty())
}
