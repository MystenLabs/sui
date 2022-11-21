// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use executor::{ExecutionIndices, ExecutionState, TransactionExecutionPair};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use types::CommittedSubDag;

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
    async fn handle_consensus_transactions(
        &self,
        _consensus_output: &Arc<CommittedSubDag>,
        transaction_execution_pairs: Vec<TransactionExecutionPair>,
    ) {
        let transaction = transaction_execution_pairs
            .first()
            .unwrap()
            .clone()
            .transaction;
        if let Err(err) = self.tx_transaction_confirmation.send(transaction).await {
            eprintln!("Failed to send txn in SimpleExecutionState: {}", err);
        }
    }

    async fn load_execution_indices(&self) -> ExecutionIndices {
        ExecutionIndices::default()
    }
}
