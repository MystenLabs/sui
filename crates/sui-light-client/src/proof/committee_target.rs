// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use sui_types::{
    committee::Committee,
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::{CertifiedCheckpointSummary, EndOfEpochData},
};

use crate::proof::base::{Proof, ProofBuilder, ProofContents, ProofTarget};

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

// No additional data needed for committee proof
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommitteeProof {}

impl CommitteeProof {
    pub fn verify(
        &self,
        targets: &ProofTarget,
        summary: &CertifiedCheckpointSummary,
    ) -> anyhow::Result<()> {
        match targets {
            ProofTarget::Committee(target) => {
                verify_committee_with_summary(&target.committee, summary)
            }
            _ => {
                return Err(anyhow!("Targets are not a committee"));
            }
        }
    }
}

/// Verifies the new committee using the end of epoch checkpoint summary.
fn verify_committee_with_summary(
    committee: &Committee,
    summary: &CertifiedCheckpointSummary,
) -> anyhow::Result<()> {
    // let next_epoch_committee = summary.next_epoch_committee();
    match &summary.end_of_epoch_data {
        Some(EndOfEpochData {
            next_epoch_committee,
            ..
        }) => {
            let next_committee_data = next_epoch_committee.iter().cloned().collect();
            let new_committee =
                Committee::new(summary.epoch().checked_add(1).unwrap(), next_committee_data);

            if new_committee != *committee {
                return Err(anyhow!(
                    "Given committee does not match the end of epoch committee"
                ));
            }

            Ok(())
        }
        None => {
            return Err(anyhow!(
                "No end of epoch committee in the checkpoint summary"
            ));
        }
    }
}
