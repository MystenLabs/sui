// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use sui_types::{
    committee::Committee,
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::{CertifiedCheckpointSummary, VerifiedCheckpoint},
};

use crate::{
    base::ProofContentsVerifier,
    proof::base::{Proof, ProofBuilder, ProofContents, ProofTarget},
    proof::error::{ProofError, ProofResult},
};

/// The new committee to be verified.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitteeTarget {
    pub committee: Committee,
}

impl ProofBuilder for CommitteeTarget {
    fn construct(self, checkpoint: &CheckpointData) -> ProofResult<Proof> {
        // Do a minimal check that the given checkpoint data is consistent with the committee
        // Check we have the correct epoch
        if checkpoint.checkpoint_summary.epoch() + 1 != self.committee.epoch {
            return Err(ProofError::EpochMismatch);
        }

        // Check its an end of epoch checkpoint
        if checkpoint.checkpoint_summary.end_of_epoch_data.is_none() {
            return Err(ProofError::ExpectedEndOfEpochCheckpoint);
        }

        Ok(Proof {
            targets: ProofTarget::Committee(self),
            checkpoint_summary: checkpoint.checkpoint_summary.clone(),
            proof_contents: ProofContents::CommitteeProof(CommitteeProof {}),
        })
    }
}

/// Note: The summary is enough to verify the committee.
/// This is a placeholder for the committee proof.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitteeProof {}

impl ProofContentsVerifier for CommitteeProof {
    fn verify(self, targets: &ProofTarget, summary: &VerifiedCheckpoint) -> ProofResult<()> {
        // Note: We just need to verify the new committee is the same as the one in the checkpoint
        // summary as the summary is already verified.
        match targets {
            ProofTarget::Committee(target) => {
                let new_committee = extract_new_committee_info(summary)?;
                if new_committee != target.committee {
                    return Err(ProofError::CommitteeMismatch);
                }
                Ok(())
            }
            _ => Err(ProofError::MismatchedTargetAndProofType),
        }
    }
}

/// Get the new committee from the end of epoch checkpoint summary.
pub fn extract_new_committee_info(summary: &CertifiedCheckpointSummary) -> ProofResult<Committee> {
    if let Some(next_epoch_committee) = summary.next_epoch_committee() {
        let next_committee = next_epoch_committee.iter().cloned().collect();
        let next_epoch = summary
            .epoch()
            .checked_add(1)
            .ok_or(ProofError::EpochAddOverflow)?;
        Ok(Committee::new(next_epoch, next_committee))
    } else {
        Err(ProofError::ExpectedEndOfEpochCheckpoint)
    }
}
