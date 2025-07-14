// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use sui_types::{
    effects::TransactionEffectsAPI,
    event::{Event, EventID},
    full_checkpoint_content::CheckpointData,
    full_checkpoint_content::CheckpointTransaction,
};

use crate::proof::{
    base::{Proof, ProofBuilder, ProofContents, ProofTarget},
    transaction_proof::TransactionProof,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventsTarget {
    pub events: Vec<(EventID, Event)>,
}

impl ProofBuilder for EventsTarget {
    fn construct(self, checkpoint: &CheckpointData) -> anyhow::Result<Proof> {
        let mut event_txs = self.events.iter().map(|(eid, _)| eid.tx_digest);

        let target_tx = event_txs.next().ok_or(anyhow!("No transaction found"))?;

        // Check that all targets refer to the same transaction
        if !event_txs.all(|tx| tx == target_tx) {
            return Err(anyhow!("All targets must refer to the same transaction"));
        }

        let tx = checkpoint
            .transactions
            .iter()
            .find(|t| t.effects.transaction_digest() == &target_tx)
            .ok_or(anyhow!("Transaction not found"))?;

        let CheckpointTransaction {
            transaction,
            effects,
            events,
            ..
        } = tx;

        Ok(Proof {
            targets: ProofTarget::Events(self),
            checkpoint_summary: checkpoint.checkpoint_summary.clone(),
            proof_contents: ProofContents::TransactionProof(TransactionProof {
                checkpoint_contents: checkpoint.checkpoint_contents.clone(),
                transaction: transaction.clone(),
                effects: effects.clone(),
                events: events.clone(),
            }),
        })
    }
}
