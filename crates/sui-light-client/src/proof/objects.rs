// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use serde::{Deserialize, Serialize};
use sui_types::{
    base_types::ObjectRef, effects::TransactionEffectsAPI, full_checkpoint_content::CheckpointData,
    full_checkpoint_content::CheckpointTransaction, object::Object,
};

use crate::proof::{
    base::{Proof, ProofBuilder, ProofContents, ProofTarget},
    transaction_proof::TransactionProof,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ObjectsTarget {
    pub objects: Vec<(ObjectRef, Object)>,
}

impl ProofBuilder for ObjectsTarget {
    fn construct(self, checkpoint: &CheckpointData) -> anyhow::Result<Proof> {
        let mut object_txs = self.objects.iter().map(|(_, o)| o.previous_transaction);

        let target_tx = object_txs.next().ok_or(anyhow!("No transaction found"))?;

        // Check that all targets refer to the same transaction
        if !object_txs.all(|tx| tx == target_tx) {
            return Err(anyhow!("All targets must refer to the same transaction"));
        }

        let transaction_proof = TransactionProof::new(target_tx, checkpoint, false)?;

        Ok(Proof {
            targets: ProofTarget::Objects(self.clone()),
            checkpoint_summary: checkpoint.checkpoint_summary.clone(),
            proof_contents: ProofContents::TransactionProof(transaction_proof),
        })
    }
}
