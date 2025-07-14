// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use serde::{Deserialize, Serialize};
use sui_types::{
    base_types::ObjectRef,
    committee::Committee,
    event::{Event, EventID},
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::CertifiedCheckpointSummary,
    object::Object,
};

use crate::proof::{
    committee::{CommitteeProof, CommitteeTarget},
    events::EventsTarget,
    objects::ObjectsTarget,
    transaction_proof::TransactionProof,
};

pub trait ProofBuilder {
    fn construct(self, checkpoint: &CheckpointData) -> anyhow::Result<Proof>;
}

pub trait ProofVerifier {
    fn verify(&self, committee: &Committee) -> anyhow::Result<()>;
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
    fn construct(self, checkpoint: &CheckpointData) -> anyhow::Result<Proof> {
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
    fn verify(&self, committee: &Committee) -> anyhow::Result<()> {
        // Verify the checkpoint summary, which is common to all proof types.
        self.checkpoint_summary
            .verify_authority_signatures(committee)?;

        // Sanity check that targets & proof types match
        match &self.targets {
            ProofTarget::Objects(_) | ProofTarget::Events(_) => {
                if !matches!(self.proof_contents, ProofContents::TransactionProof(_)) {
                    return Err(anyhow!("Targets are objects or events, but proof contents is not a transaction proof"));
                }
            }
            ProofTarget::Committee(_) => {
                if !matches!(self.proof_contents, ProofContents::CommitteeProof(_)) {
                    return Err(anyhow!(
                        "Targets are a committee, but proof contents is not a committee proof"
                    ));
                }
            }
        }

        match &self.proof_contents {
            ProofContents::TransactionProof(transaction_proof) => {
                transaction_proof.verify(committee, &self.checkpoint_summary, &self.targets)
            }
            ProofContents::CommitteeProof(committee_proof) => {
                committee_proof.verify(&self.targets, &self.checkpoint_summary)
            }
        }
    }
}
