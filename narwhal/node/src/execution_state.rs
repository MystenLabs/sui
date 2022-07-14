// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use executor::{ExecutionIndices, ExecutionState, ExecutionStateError};
use thiserror::Error;

/// A simple/dumb execution engine.
pub struct SimpleExecutionState;

#[async_trait]
impl ExecutionState for SimpleExecutionState {
    type Transaction = String;
    type Error = SimpleExecutionError;

    async fn handle_consensus_transaction(
        &self,
        _execution_indices: ExecutionIndices,
        _transaction: Self::Transaction,
    ) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::default())
    }

    fn ask_consensus_write_lock(&self) -> bool {
        true
    }

    fn release_consensus_write_lock(&self) {}

    async fn load_execution_indices(&self) -> Result<ExecutionIndices, Self::Error> {
        Ok(ExecutionIndices::default())
    }
}

/// A simple/dumb execution error.
#[derive(Debug, Error)]
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

    fn to_string(&self) -> String {
        ToString::to_string(&self)
    }
}
