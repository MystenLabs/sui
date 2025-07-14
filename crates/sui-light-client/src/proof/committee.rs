// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use sui_types::{
    committee::Committee,
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::{CertifiedCheckpointSummary, VerifiedCheckpoint},
};

use crate::{
    base::ProofContentsVerifier,
    proof::base::{Proof, ProofBuilder, ProofContents, ProofTarget},
};

/// The new committee to be verified.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitteeTarget {
    pub committee: Committee,
}

impl ProofBuilder for CommitteeTarget {
    fn construct(self, checkpoint: &CheckpointData) -> anyhow::Result<Proof> {
        // Do a minimal check that the given checkpoint data is consistent with the committee
        // Check we have the correct epoch
        if checkpoint.checkpoint_summary.epoch() + 1 != self.committee.epoch {
            return Err(anyhow!("Epoch mismatch between checkpoint and committee"));
        }

        // Check its an end of epoch checkpoint
        if checkpoint.checkpoint_summary.end_of_epoch_data.is_none() {
            return Err(anyhow!("Expected end of epoch checkpoint"));
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
    fn verify(self, targets: &ProofTarget, summary: &VerifiedCheckpoint) -> anyhow::Result<()> {
        // Note: We just need to verify the new committee is the same as the one in the checkpoint
        // summary as the summary is already verified.
        match targets {
            ProofTarget::Committee(target) => {
                let new_committee = extract_new_committee_info(summary)?;
                if new_committee != target.committee {
                    return Err(anyhow!(
                        "Given committee does not match the end of epoch committee"
                    ));
                }
                Ok(())
            }
            _ => {
                return Err(anyhow!("Targets are not a committee"));
            }
        }
    }
}

/// Get the new committee from the end of epoch checkpoint summary.
pub fn extract_new_committee_info(
    summary: &CertifiedCheckpointSummary,
) -> anyhow::Result<Committee> {
    let next_epoch_committee = summary.next_epoch_committee();
    if next_epoch_committee.is_none() {
        return Err(anyhow!(
            "No end of epoch committee in the checkpoint summary"
        ));
    }

    let next_committee_data = next_epoch_committee.unwrap().iter().cloned().collect();

    Ok(Committee::new(
        summary.epoch().checked_add(1).unwrap(),
        next_committee_data,
    ))
}
