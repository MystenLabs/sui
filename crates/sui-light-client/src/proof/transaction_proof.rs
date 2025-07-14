// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use serde::{Deserialize, Serialize};

use sui_types::{
    base_types::ObjectRef,
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    event::{Event, EventID},
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::{CheckpointContents, VerifiedCheckpoint},
    object::Object,
    transaction::Transaction,
};

use crate::proof::base::{ProofContentsVerifier, ProofTarget};

/// A proof that provides evidence relating to a specific transaction.
/// Implements the tx effects -> contents -> summary pathway.
/// The certified effects can be used to in turn verify objects / events, for example.
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
    pub fn new(
        tx_digest: TransactionDigest,
        checkpoint: &CheckpointData,
        add_events: bool,
    ) -> anyhow::Result<Self> {
        let tx = checkpoint
            .transactions
            .iter()
            .find(|t| t.transaction.digest() == &tx_digest)
            .ok_or(anyhow!("Transaction not found"))?;

        Ok(Self {
            checkpoint_contents: checkpoint.checkpoint_contents.clone(),
            transaction: tx.transaction.clone(),
            effects: tx.effects.clone(),
            events: if add_events { tx.events.clone() } else { None },
        })
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

impl ProofContentsVerifier for TransactionProof {
    fn verify(self, targets: &ProofTarget, summary: &VerifiedCheckpoint) -> anyhow::Result<()> {
        let contents_digest = *self.checkpoint_contents.digest();
        if contents_digest != summary.data().content_digest {
            return Err(anyhow!(
                "Contents digest does not match the checkpoint summary"
            ));
        }
        // MILESTONE: Contents is correct

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
}
