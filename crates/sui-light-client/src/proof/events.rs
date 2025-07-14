// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

use sui_types::{
    event::{Event, EventID},
    full_checkpoint_content::CheckpointData,
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

        let transaction_proof = TransactionProof::new(target_tx, checkpoint, true)?;

        Ok(Proof {
            targets: ProofTarget::Events(self),
            checkpoint_summary: checkpoint.checkpoint_summary.clone(),
            proof_contents: ProofContents::TransactionProof(transaction_proof),
        })
    }
}
