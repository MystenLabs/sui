// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::ExecutionIndicesWithHash;
use crate::authority::AuthorityState;
use crate::checkpoints::CheckpointService;
use async_trait::async_trait;
use mysten_metrics::monitored_scope;
use narwhal_executor::{ExecutionIndices, ExecutionState};
use narwhal_types::{ConsensusOutput, Round};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use sui_types::messages::ConsensusTransaction;
use tracing::{debug, instrument, warn};

pub struct ConsensusHandler {
    state: Arc<AuthorityState>,
    last_seen: Mutex<ExecutionIndicesWithHash>,
    checkpoint_service: Arc<CheckpointService>,
}

impl ConsensusHandler {
    pub fn new(state: Arc<AuthorityState>, checkpoint_service: Arc<CheckpointService>) -> Self {
        let last_seen = Mutex::new(Default::default());
        Self {
            state,
            last_seen,
            checkpoint_service,
        }
    }

    fn update_hash(
        last_seen: &Mutex<ExecutionIndicesWithHash>,
        index: ExecutionIndices,
        v: &[u8; 8],
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
        // Log hash for every sub dag
        if index.sub_dag_index == 1 && last_seen_guard.index.sub_dag_index == 1 {
            debug!(
                "Integrity hash for consensus output at subdag {} is {:016x}",
                index.sub_dag_index, hash
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
    async fn handle_consensus_output(
        &self,
        // TODO [2533]: use this once integrating Narwhal reconfiguration
        consensus_output: ConsensusOutput,
    ) {
        let _scope = monitored_scope("HandleConsensusOutputFull");
        let mut sequenced_transactions = Vec::new();
        let mut seq = 0;

        for (cert, batches) in consensus_output.batches {
            let round = cert.header.round;
            let author = cert.header.author.clone();
            let output_cert = Arc::new(cert);
            for batch in batches {
                for serialized_transaction in batch.transactions {
                    let transaction = match bincode::deserialize::<ConsensusTransaction>(
                        &serialized_transaction,
                    ) {
                        Ok(transaction) => transaction,
                        Err(err) => {
                            warn!(
                                    "Ignoring malformed transaction (failed to deserialize) from {}: {}",
                                    author, err
                                );
                            continue;
                        }
                    };
                    let index = ExecutionIndices {
                        last_committed_round: round,
                        sub_dag_index: consensus_output.sub_dag.sub_dag_index,
                        transaction_index: seq,
                    };

                    let index_with_hash =
                        match Self::update_hash(&self.last_seen, index, &transaction.tracking_id) {
                            Some(i) => i,
                            None => {
                                debug!(
                "Ignore consensus transaction at index {:?} as it appear to be already processed",
                index
            );
                                continue;
                            }
                        };

                    sequenced_transactions.push(SequencedConsensusTransaction {
                        certificate: output_cert.clone(),
                        consensus_index: index_with_hash,
                        transaction,
                    });
                    seq += 1;
                }
            }
        }

        for sequenced_transaction in sequenced_transactions {
            let verified_transaction = match self
                .state
                .verify_consensus_transaction(sequenced_transaction)
            {
                Ok(verified_transaction) => verified_transaction,
                Err(()) => return,
            };

            self.state
                .handle_consensus_transaction(verified_transaction, &self.checkpoint_service)
                .await
                .expect("Unrecoverable error in consensus handler");
        }

        self.state
            .handle_commit_boundary(&consensus_output.sub_dag, &self.checkpoint_service)
            .expect("Unrecoverable error in consensus handler when processing commit boundary")
    }

    async fn last_committed_round(&self) -> Round {
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
        index_with_hash.index.last_committed_round
    }
}

pub struct SequencedConsensusTransaction {
    pub certificate: Arc<narwhal_types::Certificate>,
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
            certificate: Default::default(),
            consensus_index: Default::default(),
        }
    }
}

#[test]
pub fn test_update_hash() {
    let index0 = ExecutionIndices {
        sub_dag_index: 0,
        transaction_index: 0,
        last_committed_round: 0,
    };
    let index1 = ExecutionIndices {
        sub_dag_index: 0,
        transaction_index: 1,
        last_committed_round: 0,
    };
    let index2 = ExecutionIndices {
        sub_dag_index: 1,
        transaction_index: 0,
        last_committed_round: 0,
    };

    let last_seen = ExecutionIndicesWithHash {
        index: index1,
        hash: 1000,
    };

    let last_seen = Mutex::new(last_seen);
    let tx = &[0, 0, 0, 0, 0, 0, 0, 0];
    assert!(ConsensusHandler::update_hash(&last_seen, index0, tx).is_none());
    assert!(ConsensusHandler::update_hash(&last_seen, index1, tx).is_none());
    assert!(ConsensusHandler::update_hash(&last_seen, index2, tx).is_some());
}
