// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use consensus::ConsensusOutput;
use executor::{ExecutionIndices, ExecutionState, ExecutionStateError, SingleExecutionState};
use thiserror::Error;

/// A simple/dumb execution engine.
pub struct SimpleExecutionState;

#[async_trait]
impl ExecutionState for SimpleExecutionState {
    type Error = SimpleExecutionError;

    fn ask_consensus_write_lock(&self) -> bool {
        true
    }

    fn release_consensus_write_lock(&self) {}
}

#[async_trait]
impl SingleExecutionState for SimpleExecutionState {
    type Transaction = String;
    type Outcome = Vec<u8>;

    async fn handle_consensus_transaction(
        &self,
        _consensus_output: &ConsensusOutput,
        _execution_indices: ExecutionIndices,
        _transaction: Self::Transaction,
    ) -> Result<Self::Outcome, Self::Error> {
        Ok(Vec::default())
    }

    async fn load_execution_indices(&self) -> Result<ExecutionIndices, Self::Error> {
        Ok(ExecutionIndices::default())
    }
}

impl Default for SimpleExecutionState {
    fn default() -> Self {
        Self
    }
}

/// A simple/dumb execution error.
#[derive(Debug, Error, Clone)]
pub enum SimpleExecutionError {
    #[error("Something went wrong in the authority")]
    ServerError,

    #[error("The client made something bad")]
    ClientError,
}

#[async_trait]
impl ExecutionStateError for SimpleExecutionError {
    fn node_error(&self) -> bool {
        match self {
            Self::ServerError => true,
            Self::ClientError => false,
        }
    }
}
