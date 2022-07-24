// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use config::Committee;
use consensus::ConsensusOutput;
use crypto::traits::VerifyingKey;
use executor::{ExecutionIndices, ExecutionState, ExecutionStateError};
use thiserror::Error;

/// A simple/dumb execution engine.
pub struct SimpleExecutionState;

#[async_trait]
impl ExecutionState for SimpleExecutionState {
    type Transaction = String;
    type Error = SimpleExecutionError;
    type Outcome = Vec<u8>;

    async fn handle_consensus_transaction<PublicKey: VerifyingKey>(
        &self,
        _consensus_output: &ConsensusOutput<PublicKey>,
        _execution_indices: ExecutionIndices,
        _transaction: Self::Transaction,
    ) -> Result<(Self::Outcome, Option<Committee<PublicKey>>), Self::Error> {
        Ok((Vec::default(), None))
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
