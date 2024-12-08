// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proof::{Proof, ProofTarget, TransactionProof};

use anyhow::anyhow;
use sui_rpc_api::{CheckpointData, CheckpointTransaction};
use sui_types::effects::TransactionEffectsAPI;

/// Construct a proof from the given checkpoint data and proof targets.
///
/// Only minimal cheaper checks are performed to ensure the proof is valid. If you need guaranteed
/// validity consider calling `verify_proof` function on the constructed proof. It either returns
/// `Ok` with a proof, or `Err` with a description of the error.
pub fn construct_proof(targets: ProofTarget, data: &CheckpointData) -> anyhow::Result<Proof> {
    let checkpoint_summary = data.checkpoint_summary.clone();
    let mut this_proof = Proof {
        targets,
        checkpoint_summary,
        contents_proof: None,
    };

    // Do a minimal check that the given checkpoint data is consistent with the committee
    if let Some(committee) = &this_proof.targets.committee {
        // Check we have the correct epoch
        if this_proof.checkpoint_summary.epoch() + 1 != committee.epoch {
            return Err(anyhow!("Epoch mismatch between checkpoint and committee"));
        }

        // Check its an end of epoch checkpoint
        if this_proof.checkpoint_summary.end_of_epoch_data.is_none() {
            return Err(anyhow!("Expected end of epoch checkpoint"));
        }
    }

    // If proof targets include objects or events, we need to include the contents proof
    // Need to ensure that all targets refer to the same transaction first of all
    let object_tx = this_proof
        .targets
        .objects
        .iter()
        .map(|(_, o)| o.previous_transaction);
    let event_tx = this_proof
        .targets
        .events
        .iter()
        .map(|(eid, _)| eid.tx_digest);
    let mut all_tx = object_tx.chain(event_tx);

    // Get the first tx ID
    let target_tx_id = if let Some(first_tx) = all_tx.next() {
        first_tx
    } else {
        // Since there is no target we just return the summary proof
        return Ok(this_proof);
    };

    // Basic check that all targets refer to the same transaction
    if !all_tx.all(|tx| tx == target_tx_id) {
        return Err(anyhow!("All targets must refer to the same transaction"));
    }

    // Find the transaction in the checkpoint data
    let tx = data
        .transactions
        .iter()
        .find(|t| t.effects.transaction_digest() == &target_tx_id)
        .ok_or(anyhow!("Transaction not found in checkpoint data"))?
        .clone();

    let CheckpointTransaction {
        transaction,
        effects,
        events,
        ..
    } = tx;

    // Add all the transaction data in there
    this_proof.contents_proof = Some(TransactionProof {
        checkpoint_contents: data.checkpoint_contents.clone(),
        transaction,
        effects,
        events,
    });

    // TODO: should we check that the objects & events are in the transaction, to
    //       avoid constructing invalid proofs? I opt to not check because the check
    //       is expensive (sequential scan of all objects).

    Ok(this_proof)
}
