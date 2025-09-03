// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

use crate::proof::{
    base::{ProofContentsVerifier, ProofTarget},
    error::{ProofError, ProofResult},
};

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
    ) -> ProofResult<Self> {
        let tx = checkpoint
            .transactions
            .iter()
            .find(|t| t.transaction.digest() == &tx_digest)
            .ok_or(ProofError::TransactionNotFound)?;

        Ok(Self {
            checkpoint_contents: checkpoint.checkpoint_contents.clone(),
            transaction: tx.transaction.clone(),
            effects: tx.effects.clone(),
            events: if add_events { tx.events.clone() } else { None },
        })
    }

    /// Check that the object references are correct and in the effects
    fn verify_objects(&self, target_objects: &Vec<(ObjectRef, Object)>) -> ProofResult<()> {
        // Now check all object references are correct and in the effects
        let changed_objects = self.effects.all_changed_objects();

        for (object_ref, object) in target_objects {
            // Is the given reference correct?
            if object_ref != &object.compute_object_reference() {
                return Err(ProofError::ObjectReferenceMismatch);
            }

            // Has this object been created in these effects?
            changed_objects
                .iter()
                .find(|effects_object_ref| &effects_object_ref.0 == object_ref)
                .ok_or(ProofError::ObjectNotFound)?;
        }
        Ok(())
    }

    fn verify_events(
        &self,
        target_events: &Vec<(EventID, Event)>,
        tx_digest: &TransactionDigest,
    ) -> ProofResult<()> {
        if self.effects.events_digest() != self.events.as_ref().map(|e| e.digest()).as_ref() {
            return Err(ProofError::EventsDigestMismatch);
        }

        match &self.events {
            Some(tx_events) => {
                for (event_id, event) in target_events {
                    // Check the event corresponds to the transaction
                    if event_id.tx_digest != *tx_digest {
                        return Err(ProofError::EventTransactionMismatch);
                    }

                    // The sequence number must be a valid index
                    if event_id.event_seq as usize >= tx_events.data.len() {
                        return Err(ProofError::EventSequenceOutOfBounds);
                    }

                    // Check the event contents are the same
                    if tx_events.data[event_id.event_seq as usize] != *event {
                        return Err(ProofError::EventContentsMismatch);
                    }
                }
            }
            None => {
                // If the target includes any events, tx events must not be None
                if !target_events.is_empty() {
                    return Err(ProofError::EventsMissing);
                }
            }
        }

        Ok(())
    }
}

impl ProofContentsVerifier for TransactionProof {
    fn verify(self, targets: &ProofTarget, summary: &VerifiedCheckpoint) -> ProofResult<()> {
        let contents_digest = *self.checkpoint_contents.digest();
        if contents_digest != summary.data().content_digest {
            return Err(ProofError::ContentsDigestMismatch);
        }
        // MILESTONE: Contents is correct

        // Extract Transaction Digests and check they are in contents
        let digests = self.effects.execution_digests();
        if self.transaction.digest() != &digests.transaction {
            return Err(ProofError::TransactionDigestMismatch);
        }

        // Ensure the digests are in the checkpoint contents
        if !self
            .checkpoint_contents
            .enumerate_transactions(summary)
            .any(|x| x.1 == &digests)
        {
            // Could not find the digest in the checkpoint contents
            return Err(ProofError::TransactionDigestNotFound);
        }

        // MILESTONE: Transaction & Effect correct and in contents

        match targets {
            ProofTarget::Objects(target) => self.verify_objects(&target.objects),
            ProofTarget::Events(target) => self.verify_events(&target.events, &digests.transaction),
            _ => Err(ProofError::MismatchedTargetAndProofType),
        }
    }
}
