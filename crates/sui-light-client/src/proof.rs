// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use serde::{Deserialize, Serialize};
use sui_types::{
    base_types::ObjectRef,
    committee::Committee,
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    event::{Event, EventID},
    messages_checkpoint::{CertifiedCheckpointSummary, CheckpointContents, EndOfEpochData},
    object::Object,
    transaction::Transaction,
};

/// Define aspect of Sui state that need to be certified in a proof
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ProofTarget {
    /// Objects that need to be certified.
    pub objects: Vec<(ObjectRef, Object)>,

    /// Events that need to be certified.
    pub events: Vec<(EventID, Event)>,

    /// The next committee being certified.
    pub committee: Option<Committee>,
}

impl ProofTarget {
    /// Create a new empty proof target. An empty proof target still ensures that the
    /// checkpoint summary is correct.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an object to be certified by object reference and content. A verified proof will
    /// ensure that both the reference and content are correct. Note that some content is
    /// metadata such as the transaction that created this object.
    pub fn add_object(mut self, object_ref: ObjectRef, object: Object) -> Self {
        self.objects.push((object_ref, object));
        self
    }

    /// Add an event to be certified by event ID and content. A verified proof will ensure that
    /// both the ID and content are correct.
    pub fn add_event(mut self, event_id: EventID, event: Event) -> Self {
        self.events.push((event_id, event));
        self
    }

    /// Add the next committee to be certified. A verified proof will ensure that the next
    /// committee is correct.
    pub fn set_committee(mut self, committee: Committee) -> Self {
        self.committee = Some(committee);
        self
    }
}

/// Part of a proof that provides evidence relating to a specific transaction to
/// certify objects and events.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionProof {
    /// Checkpoint contents including this transaction.
    pub checkpoint_contents: CheckpointContents,

    /// The transaction being certified.
    pub transaction: Transaction,

    /// The effects of the transaction being certified.
    pub effects: TransactionEffects,

    /// The events of the transaction being certified.
    pub events: Option<TransactionEvents>,
}

/// A proof for specific targets. It certifies a checkpoint summary and optionally includes
/// transaction evidence to certify objects and events.
#[derive(Debug, Serialize, Deserialize)]
pub struct Proof {
    /// Targets of the proof are a committee, objects, or events that need to be certified.
    pub targets: ProofTarget,

    /// A summary of the checkpoint being certified.
    pub checkpoint_summary: CertifiedCheckpointSummary,

    /// Optional transaction proof to certify objects and events.
    pub contents_proof: Option<TransactionProof>,
}

/// Verify a proof against a committee. A proof is valid if it certifies the checkpoint summary
/// and optionally includes transaction evidence to certify objects and events.
///
/// If the result is `Ok(())` then the proof is valid. If Err is returned then the proof is invalid
/// and the error message will describe the reason. Once a proof is verified it can be trusted,
/// and information in `targets` as well as `checkpoint_summary` or `contents_proof` can be
/// trusted as being authentic.
///
/// The authoritative committee is required to verify the proof. The sequence of committees can be
/// verified through a Committee proof target on the last checkpoint of each epoch,
/// sequentially since the first epoch.
pub fn verify_proof(committee: &Committee, proof: &Proof) -> anyhow::Result<()> {
    // Get checkpoint summary and optional contents
    let summary = &proof.checkpoint_summary;
    let contents_ref = proof
        .contents_proof
        .as_ref()
        .map(|x| &x.checkpoint_contents);

    // Verify the checkpoint summary using the committee
    summary.verify_with_contents(committee, contents_ref)?;

    // MILESTONE 1 : summary and contents is correct
    // Note: this is unconditional on the proof targets, and always checked.

    // If the proof target is the next committee check it
    if let Some(committee) = &proof.targets.committee {
        match &summary.end_of_epoch_data {
            Some(EndOfEpochData {
                next_epoch_committee,
                ..
            }) => {
                // Extract the end of epoch committee
                let next_committee_data = next_epoch_committee.iter().cloned().collect();
                let new_committee =
                    Committee::new(summary.epoch().checked_add(1).unwrap(), next_committee_data);

                if new_committee != *committee {
                    return Err(anyhow!(
                        "Given committee does not match the end of epoch committee"
                    ));
                }
            }
            None => {
                return Err(anyhow!(
                    "No end of epoch committee in the checkpoint summary"
                ));
            }
        }
    }

    // MILESTONE 2: committee if requested is correct

    // Non empty object or event targets require the optional contents proof
    // If it is not present return an error

    if (!proof.targets.objects.is_empty() || !proof.targets.events.is_empty())
        && proof.contents_proof.is_none()
    {
        return Err(anyhow!("Contents proof is missing"));
    }

    // MILESTONE 3: contents proof is present if required

    if let Some(contents_proof) = &proof.contents_proof {
        // Extract Transaction Digests and check they are in contents
        let digests = contents_proof.effects.execution_digests();
        if contents_proof.transaction.digest() != &digests.transaction {
            return Err(anyhow!(
                "Transaction digest does not match the execution digest"
            ));
        }

        // Ensure the digests are in the checkpoint contents
        if !contents_proof
            .checkpoint_contents
            .enumerate_transactions(summary)
            .any(|x| x.1 == &digests)
        {
            // Could not find the digest in the checkpoint contents
            return Err(anyhow!(
                "Transaction digest not found in the checkpoint contents"
            ));
        }

        // MILESTONE 4: Transaction & Effect correct and in contents

        if contents_proof.effects.events_digest()
            != contents_proof.events.as_ref().map(|e| e.digest()).as_ref()
        {
            return Err(anyhow!("Events digest does not match the execution digest"));
        }

        // If the target includes any events ensure the events digest is not None
        if !proof.targets.events.is_empty() && contents_proof.events.is_none() {
            return Err(anyhow!("Events digest is missing"));
        }

        // MILESTONE 5: Events digest & Events are correct and present if required

        // Now we verify the content of any target events

        for (event_id, event) in &proof.targets.events {
            // Check the event corresponds to the transaction
            if event_id.tx_digest != digests.transaction {
                return Err(anyhow!("Event does not belong to the transaction"));
            }

            // The sequence number must be a valid index
            // Note: safe to unwrap as we have checked that its not None above
            if event_id.event_seq as usize >= contents_proof.events.as_ref().unwrap().data.len() {
                return Err(anyhow!("Event sequence number out of bounds"));
            }

            // Now check that the contents of the event are the same
            if &contents_proof.events.as_ref().unwrap().data[event_id.event_seq as usize] != event {
                return Err(anyhow!("Event contents do not match"));
            }
        }

        // MILESTONE 6: Event contents are correct

        // Now check all object references are correct and in the effects
        let changed_objects = contents_proof.effects.all_changed_objects();

        for (object_ref, object) in &proof.targets.objects {
            // Is the given reference correct?
            if object_ref != &object.compute_object_reference() {
                return Err(anyhow!("Object reference does not match the object"));
            }

            // Has this object been created in these effects?
            changed_objects
                .iter()
                .find(|effects_object_ref| &effects_object_ref.0 == object_ref)
                .ok_or(anyhow!("Object not found"))?;
        }

        // MILESTONE 7: Object references are correct and in the effects
    }

    Ok(())
}
