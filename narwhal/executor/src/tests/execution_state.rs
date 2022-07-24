// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{ExecutionIndices, ExecutionState, ExecutionStateError};
use async_trait::async_trait;
use config::Committee;
use consensus::ConsensusOutput;
use crypto::traits::VerifyingKey;
use futures::executor::block_on;
use std::path::Path;
use store::{
    reopen,
    rocks::{open_cf, DBMap},
    Store,
};
use thiserror::Error;

/// A malformed transaction.
pub const MALFORMED_TRANSACTION: <TestState as ExecutionState>::Transaction = 400;

/// A special transaction that makes the executor engine crash.
pub const KILLER_TRANSACTION: <TestState as ExecutionState>::Transaction = 500;

/// A dumb execution state for testing.
pub struct TestState {
    store: Store<u64, ExecutionIndices>,
}

impl std::fmt::Debug for TestState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", block_on(self.get_execution_indices()))
    }
}

impl Default for TestState {
    fn default() -> Self {
        Self::new(tempfile::tempdir().unwrap().path())
    }
}

#[async_trait]
impl ExecutionState for TestState {
    type Transaction = u64;
    type Error = TestStateError;
    type Outcome = Vec<u8>;

    async fn handle_consensus_transaction<PublicKey: VerifyingKey>(
        &self,
        _consensus_output: &ConsensusOutput<PublicKey>,
        execution_indices: ExecutionIndices,
        transaction: Self::Transaction,
    ) -> Result<(Self::Outcome, Option<Committee<PublicKey>>), Self::Error> {
        if transaction == MALFORMED_TRANSACTION {
            Err(Self::Error::ClientError)
        } else if transaction == KILLER_TRANSACTION {
            Err(Self::Error::ServerError)
        } else {
            self.store
                .write(Self::INDICES_ADDRESS, execution_indices)
                .await;
            Ok((Vec::default(), None))
        }
    }

    fn ask_consensus_write_lock(&self) -> bool {
        true
    }

    fn release_consensus_write_lock(&self) {}

    async fn load_execution_indices(&self) -> Result<ExecutionIndices, Self::Error> {
        let indices = self
            .store
            .read(Self::INDICES_ADDRESS)
            .await
            .unwrap()
            .unwrap_or_default();
        Ok(indices)
    }
}

impl TestState {
    /// The address at which to store the indices (rocksdb is a key-value store).
    pub const INDICES_ADDRESS: u64 = 14;

    /// Create a new test state.
    pub fn new(store_path: &Path) -> Self {
        const STATE_CF: &str = "test_state";
        let rocksdb = open_cf(store_path, None, &[STATE_CF]).unwrap();
        let map = reopen!(&rocksdb, STATE_CF;<u64, ExecutionIndices>);
        Self {
            store: Store::new(map),
        }
    }

    /// Load the execution indices; ie. the state.
    pub async fn get_execution_indices(&self) -> ExecutionIndices {
        self.load_execution_indices().await.unwrap()
    }
}

#[derive(Debug, Error)]
pub enum TestStateError {
    #[error("Something went wrong in the authority")]
    ServerError,

    #[error("The client made something bad")]
    ClientError,
}

#[async_trait]
impl ExecutionStateError for TestStateError {
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
