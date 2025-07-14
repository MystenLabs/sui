// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use serde::{Deserialize, Serialize};

use sui_types::{
    base_types::ObjectRef,
    committee::Committee,
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    event::{Event, EventID},
    messages_checkpoint::{CertifiedCheckpointSummary, CheckpointContents},
    object::Object,
    transaction::Transaction,
};

use crate::proof::base::ProofTarget;

/// A proof that provides evidence relating to a specific transaction to
/// certify objects and events.
/// Implements the tx effects -> contents -> summary pathway.
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

impl TransactionProof {
    pub fn verify(
        &self,
        committee: &Committee,
        summary: &CertifiedCheckpointSummary,
        targets: &ProofTarget,
    ) -> anyhow::Result<()> {
        let contents_ref = &self.checkpoint_contents;
        summary.verify_with_contents(committee, Some(contents_ref))?;

        // Extract Transaction Digests and check they are in contents
        let digests = self.effects.execution_digests();
        if self.transaction.digest() != &digests.transaction {
            return Err(anyhow!(
                "Transaction digest does not match the execution digest"
            ));
        }

        // Ensure the digests are in the checkpoint contents
        if !self
            .checkpoint_contents
            .enumerate_transactions(summary)
            .any(|x| x.1 == &digests)
        {
            // Could not find the digest in the checkpoint contents
            return Err(anyhow!(
                "Transaction digest not found in the checkpoint contents"
            ));
        }

        // MILESTONE: Transaction & Effect correct and in contents

        match targets {
            ProofTarget::Objects(target) => self.verify_objects(&target.objects),
            ProofTarget::Events(target) => self.verify_events(&target.events, &digests.transaction),
            _ => {
                return Err(anyhow!(
                    "Targets are not objects or events for transaction proof"
                ));
            }
        }
    }

    /// Check that the object references are correct and in the effects
    fn verify_objects(&self, objects: &Vec<(ObjectRef, Object)>) -> anyhow::Result<()> {
        // Now check all object references are correct and in the effects
        let changed_objects = self.effects.all_changed_objects();

        for (object_ref, object) in objects {
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
        Ok(())
    }

    /// 1/ Events digest & Events are correct and present if required
    /// 2/ Check that the events contents are correct
    fn verify_events(
        &self,
        events: &Vec<(EventID, Event)>,
        tx_digest: &TransactionDigest,
    ) -> anyhow::Result<()> {
        if self.effects.events_digest() != self.events.as_ref().map(|e| e.digest()).as_ref() {
            return Err(anyhow!("Events digest does not match the execution digest"));
        }

        // If the target includes any events ensure the events digest is not None
        if !events.is_empty() && self.events.is_none() {
            return Err(anyhow!("Events digest is missing"));
        }
        // MILESTONE 1 checked

        // Now we verify the content of any target events
        for (event_id, event) in events {
            // Check the event corresponds to the transaction
            if event_id.tx_digest != *tx_digest {
                return Err(anyhow!("Event does not belong to the transaction"));
            }

            // The sequence number must be a valid index
            // Note: safe to unwrap as we have checked that its not None above
            if event_id.event_seq as usize >= self.events.as_ref().unwrap().data.len() {
                return Err(anyhow!("Event sequence number out of bounds"));
            }

            // Now check that the contents of the event are the same
            if &self.events.as_ref().unwrap().data[event_id.event_seq as usize] != event {
                return Err(anyhow!("Event contents do not match"));
            }
        }

        // MILESTONE 2 checked
        Ok(())
    }
}
