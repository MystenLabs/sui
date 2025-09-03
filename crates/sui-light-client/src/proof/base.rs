// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::{
    base_types::ObjectRef,
    committee::Committee,
    event::{Event, EventID},
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::{CertifiedCheckpointSummary, VerifiedCheckpoint},
    object::Object,
};

use crate::proof::{
    committee::{CommitteeProof, CommitteeTarget},
    error::{ProofError, ProofResult},
    events::EventsTarget,
    objects::ObjectsTarget,
    transaction_proof::TransactionProof,
};

pub trait ProofBuilder {
    fn construct(self, checkpoint: &CheckpointData) -> ProofResult<Proof>;
}

pub trait ProofVerifier {
    fn verify(self, committee: &Committee) -> ProofResult<()>;
}

pub trait ProofContentsVerifier {
    fn verify(self, targets: &ProofTarget, summary: &VerifiedCheckpoint) -> ProofResult<()>;
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ProofTarget {
    Objects(ObjectsTarget),
    Events(EventsTarget),
    Committee(CommitteeTarget),
}

impl ProofTarget {
    pub fn new_objects(objects: Vec<(ObjectRef, Object)>) -> Self {
        ProofTarget::Objects(ObjectsTarget { objects })
    }

    pub fn new_events(events: Vec<(EventID, Event)>) -> Self {
        ProofTarget::Events(EventsTarget { events })
    }

    pub fn new_committee(committee: Committee) -> Self {
        ProofTarget::Committee(CommitteeTarget { committee })
    }
}

impl ProofBuilder for ProofTarget {
    fn construct(self, checkpoint: &CheckpointData) -> ProofResult<Proof> {
        match self {
            ProofTarget::Objects(target) => target.construct(checkpoint),
            ProofTarget::Events(target) => target.construct(checkpoint),
            ProofTarget::Committee(target) => target.construct(checkpoint),
        }
    }
}

/// A proof for specific targets. It certifies a checkpoint summary and includes
/// evidence to certify objects and events.
#[derive(Debug, Serialize, Deserialize)]
pub struct Proof {
    /// Targets of the proof are a committee, objects, or events that need to be certified.
    pub targets: ProofTarget,

    /// A summary of the checkpoint being certified.
    pub checkpoint_summary: CertifiedCheckpointSummary,

    /// The contents of the proof.
    pub proof_contents: ProofContents,
}

/// Different types of proofs that can be constructed.
#[derive(Debug, Serialize, Deserialize)]
pub enum ProofContents {
    /// Used by ObjectsTarget & EventsTarget.
    TransactionProof(TransactionProof),

    /// Used by CommitteeTarget.
    CommitteeProof(CommitteeProof),
}

impl ProofVerifier for Proof {
    fn verify(self, committee: &Committee) -> ProofResult<()> {
        // Verify the checkpoint summary, which is common to all proof types.
        let verified_summary = self
            .checkpoint_summary
            .try_into_verified(committee)
            .map_err(|e| ProofError::SummaryVerificationFailed(e.to_string()))?;

        // Sanity check that targets & proof types match
        match &self.targets {
            ProofTarget::Objects(_) | ProofTarget::Events(_) => {
                if !matches!(self.proof_contents, ProofContents::TransactionProof(_)) {
                    return Err(ProofError::MismatchedTargetAndProofType);
                }
            }
            ProofTarget::Committee(_) => {
                if !matches!(self.proof_contents, ProofContents::CommitteeProof(_)) {
                    return Err(ProofError::MismatchedTargetAndProofType);
                }
            }
        }

        self.proof_contents.verify(&self.targets, &verified_summary)
    }
}

impl ProofContentsVerifier for ProofContents {
    fn verify(self, targets: &ProofTarget, summary: &VerifiedCheckpoint) -> ProofResult<()> {
        match self {
            ProofContents::TransactionProof(proof) => proof.verify(targets, summary),
            ProofContents::CommitteeProof(proof) => proof.verify(targets, summary),
        }
    }
}
