use std::sync::Arc;
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use consensus::ConsensusOutput;
use executor::{ExecutionIndices, ExecutionState};

use tokio::sync::mpsc::Sender;

/// A simple/dumb execution engine.
pub struct SimpleExecutionState {
    tx_transaction_confirmation: Sender<Vec<u8>>,
}

impl SimpleExecutionState {
    pub fn new(tx_transaction_confirmation: Sender<Vec<u8>>) -> Self {
        Self {
            tx_transaction_confirmation,
        }
    }
}

#[async_trait]
impl ExecutionState for SimpleExecutionState {
    async fn handle_consensus_transaction(
        &self,
        _consensus_output: &Arc<ConsensusOutput>,
        _execution_indices: ExecutionIndices,
        transaction: Vec<u8>,
    ) {
        if let Err(err) = self.tx_transaction_confirmation.send(transaction).await {
            eprintln!("Failed to send txn in SimpleExecutionState: {}", err);
        }
    }

    async fn load_execution_indices(&self) -> ExecutionIndices {
        ExecutionIndices::default()
    }
}
