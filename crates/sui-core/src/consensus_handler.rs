// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store_tables::ExecutionIndicesWithHash;
use crate::authority::AuthorityState;
use async_trait::async_trait;
use narwhal_executor::{ExecutionIndices, ExecutionState};
use narwhal_types::{CommittedSubDag, ConsensusOutput};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use sui_types::messages::ConsensusTransaction;
use tracing::{debug, instrument, warn};

pub struct ConsensusHandler {
    state: Arc<AuthorityState>,
    last_seen: Mutex<ExecutionIndicesWithHash>,
}

impl ConsensusHandler {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        let last_seen = Mutex::new(Default::default());
        Self { state, last_seen }
    }

    fn update_hash(
        last_seen: &Mutex<ExecutionIndicesWithHash>,
        index: ExecutionIndices,
        v: &[u8],
    ) -> Option<ExecutionIndicesWithHash> {
        let mut last_seen_guard = last_seen
            .try_lock()
            .expect("Should not have contention on ExecutionState::update_hash");
        if last_seen_guard.index >= index {
            return None;
        }

        let previous_hash = last_seen_guard.hash;
        let mut hasher = DefaultHasher::new();
        previous_hash.hash(&mut hasher);
        v.hash(&mut hasher);
        let hash = hasher.finish();
        // Log hash for every certificate
        if index.next_transaction_index == 1 && index.next_batch_index == 1 {
            debug!(
                "Integrity hash for consensus output at certificate {} is {:016x}",
                index.next_certificate_index, hash
            );
        }
        let last_seen = ExecutionIndicesWithHash { index, hash };
        *last_seen_guard = last_seen.clone();
        Some(last_seen)
    }
}

#[async_trait]
impl ExecutionState for ConsensusHandler {
    /// This function will be called by Narwhal, after Narwhal sequenced this certificate.
    #[instrument(level = "trace", skip_all)]
    async fn handle_consensus_transaction(
        &self,
        // TODO [2533]: use this once integrating Narwhal reconfiguration
        consensus_output: &Arc<ConsensusOutput>,
        consensus_index: ExecutionIndices,
        serialized_transaction: Vec<u8>,
    ) {
        let index = Self::update_hash(
            &self.last_seen,
            consensus_index.clone(),
            &serialized_transaction,
        );
        let index = if let Some(index) = index {
            index
        } else {
            debug!(
                "Ignore consensus transaction at index {:?} as it appear to be already processed",
                consensus_index
            );
            return;
        };
        let transaction =
            match bincode::deserialize::<ConsensusTransaction>(&serialized_transaction) {
                Ok(transaction) => transaction,
                Err(err) => {
                    warn!(
                        "Ignoring malformed transaction (failed to deserialize) from {}: {}",
                        consensus_output.certificate.header.author, err
                    );
                    return;
                }
            };
        let sequenced_transaction = SequencedConsensusTransaction {
            consensus_output: consensus_output.clone(),
            consensus_index: index,
            transaction,
        };
        let verified_transaction = match self
            .state
            .verify_consensus_transaction(consensus_output.as_ref(), sequenced_transaction)
        {
            Ok(verified_transaction) => verified_transaction,
            Err(()) => return,
        };
        self.state
            .handle_consensus_transaction(consensus_output.as_ref(), verified_transaction)
            .await
            .expect("Unrecoverable error in consensus handler");
    }

    #[instrument(level = "debug", skip_all, fields(result))]
    async fn load_execution_indices(&self) -> ExecutionIndices {
        let index_with_hash = self
            .state
            .database
            .last_consensus_index()
            .expect("Failed to load consensus indices");
        *self
            .last_seen
            .try_lock()
            .expect("Should not have contention on ExecutionState::load_execution_indices") =
            index_with_hash.clone();
        index_with_hash.index
    }

    #[instrument(level = "trace", skip_all)]
    async fn notify_commit_boundary(&self, committed_dag: &Arc<CommittedSubDag>) {
        self.state
            .handle_commit_boundary(committed_dag)
            .expect("Unrecoverable error in consensus handler when processing commit boundary")
    }
}

pub struct SequencedConsensusTransaction {
    pub consensus_output: Arc<narwhal_types::ConsensusOutput>,
    pub consensus_index: ExecutionIndicesWithHash,
    pub transaction: ConsensusTransaction,
}

pub struct VerifiedSequencedConsensusTransaction(pub SequencedConsensusTransaction);

#[cfg(test)]
impl VerifiedSequencedConsensusTransaction {
    pub fn new_test(transaction: ConsensusTransaction) -> Self {
        Self(SequencedConsensusTransaction::new_test(transaction))
    }
}

#[cfg(test)]
impl SequencedConsensusTransaction {
    pub fn new_test(transaction: ConsensusTransaction) -> Self {
        Self {
            transaction,
            consensus_output: Default::default(),
            consensus_index: Default::default(),
        }
    }
}

#[test]
pub fn test_update_hash() {
    let index0 = ExecutionIndices {
        next_certificate_index: 0,
        next_batch_index: 0,
        next_transaction_index: 0,
        last_committed_round: 0,
    };
    let index1 = ExecutionIndices {
        next_certificate_index: 0,
        next_batch_index: 1,
        next_transaction_index: 0,
        last_committed_round: 0,
    };
    let index2 = ExecutionIndices {
        next_certificate_index: 0,
        next_batch_index: 2,
        next_transaction_index: 0,
        last_committed_round: 0,
    };

    let last_seen = ExecutionIndicesWithHash {
        index: index1.clone(),
        hash: 1000,
    };
    let last_seen = Mutex::new(last_seen);
    assert!(ConsensusHandler::update_hash(&last_seen, index0, &[0]).is_none());
    assert!(ConsensusHandler::update_hash(&last_seen, index1, &[0]).is_none());
    assert!(ConsensusHandler::update_hash(&last_seen, index2, &[0]).is_some());
}
