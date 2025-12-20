// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Common test utilities for consensus handler testing

use std::collections::HashSet;
use std::sync::Arc;

use arc_swap::ArcSwap;
use consensus_core::BlockStatus;
use consensus_types::block::BlockRef;
use parking_lot::Mutex;
use prometheus::Registry;
use sui_types::digests::{Digest, TransactionDigest};
use sui_types::error::SuiResult;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::{
    AuthorityIndex, ConsensusPosition, ConsensusTransaction, ConsensusTransactionKind,
};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::transaction::{VerifiedCertificate, VerifiedTransaction};

use crate::authority::authority_per_epoch_store::{
    AuthorityPerEpochStore, ExecutionIndicesWithStats,
};
use crate::authority::backpressure::BackpressureManager;
use crate::authority::shared_object_version_manager::{AssignedTxAndVersions, Schedulable};
use crate::authority::{AuthorityMetrics, AuthorityState, ExecutionEnv};
use crate::consensus_adapter::{
    BlockStatusReceiver, ConnectionMonitorStatusForTests, ConsensusAdapter,
    ConsensusAdapterMetrics, ConsensusClient,
};
use crate::consensus_handler::{
    ConsensusHandler, ExecutionSchedulerSender, SequencedConsensusTransaction,
    SequencedConsensusTransactionKind,
};
use crate::consensus_throughput_calculator::ConsensusThroughputCalculator;
use crate::consensus_types::consensus_output_api::{ConsensusCommitAPI, ParsedTransaction};
use crate::execution_scheduler::SchedulingSource;
use crate::mock_consensus::with_block_status;

pub(crate) type CapturedTransactions =
    Arc<Mutex<Vec<(Vec<Schedulable>, AssignedTxAndVersions, SchedulingSource)>>>;

pub struct TestConsensusCommit {
    pub transactions: Vec<ConsensusTransaction>,
    pub round: u64,
    pub timestamp_ms: u64,
    pub sub_dag_index: u64,
}

impl TestConsensusCommit {
    pub fn new(
        transactions: Vec<ConsensusTransaction>,
        round: u64,
        timestamp_ms: u64,
        sub_dag_index: u64,
    ) -> Self {
        Self {
            transactions,
            round,
            timestamp_ms,
            sub_dag_index,
        }
    }

    pub fn empty(round: u64, timestamp_ms: u64, sub_dag_index: u64) -> Self {
        Self {
            transactions: vec![],
            round,
            timestamp_ms,
            sub_dag_index,
        }
    }
}

impl std::fmt::Display for TestConsensusCommit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TestConsensusCommitAPI(round={}, timestamp_ms={}, sub_dag_index={})",
            self.round, self.timestamp_ms, self.sub_dag_index
        )
    }
}

impl ConsensusCommitAPI for TestConsensusCommit {
    fn commit_ref(&self) -> consensus_core::CommitRef {
        consensus_core::CommitRef::default()
    }

    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>> {
        None
    }

    fn leader_round(&self) -> u64 {
        self.round
    }

    fn leader_author_index(&self) -> AuthorityIndex {
        0
    }

    fn commit_timestamp_ms(&self) -> u64 {
        self.timestamp_ms
    }

    fn commit_sub_dag_index(&self) -> u64 {
        self.sub_dag_index
    }

    fn transactions(&self) -> Vec<(BlockRef, Vec<ParsedTransaction>)> {
        let block_ref = BlockRef {
            author: consensus_config::AuthorityIndex::ZERO,
            round: self.round as u32,
            digest: Default::default(),
        };

        let parsed_txs: Vec<ParsedTransaction> = self
            .transactions
            .iter()
            .map(|tx| ParsedTransaction {
                transaction: tx.clone(),
                rejected: false,
                serialized_len: 0,
            })
            .collect();

        vec![(block_ref, parsed_txs)]
    }

    fn rejected_transactions_digest(&self) -> Digest {
        Digest::default()
    }

    fn rejected_transactions_debug_string(&self) -> String {
        "no rejected transactions from TestConsensusCommit".to_string()
    }
}

pub struct TestConsensusHandlerSetup<C> {
    pub consensus_handler: ConsensusHandler<C>,
    pub captured_transactions: CapturedTransactions,
}

pub fn make_consensus_adapter_for_test(
    state: Arc<AuthorityState>,
    process_via_checkpoint: HashSet<TransactionDigest>,
    execute: bool,
    mock_block_status_receivers: Vec<BlockStatusReceiver>,
) -> Arc<ConsensusAdapter> {
    let metrics = ConsensusAdapterMetrics::new_test();

    #[derive(Clone)]
    struct SubmitDirectly {
        state: Arc<AuthorityState>,
        process_via_checkpoint: HashSet<TransactionDigest>,
        execute: bool,
        mock_block_status_receivers: Arc<Mutex<Vec<BlockStatusReceiver>>>,
    }

    #[async_trait::async_trait]
    impl ConsensusClient for SubmitDirectly {
        async fn submit(
            &self,
            transactions: &[ConsensusTransaction],
            epoch_store: &Arc<AuthorityPerEpochStore>,
        ) -> SuiResult<(Vec<ConsensusPosition>, BlockStatusReceiver)> {
            // If transactions are empty, then we are performing a ping check and will attempt to ping consensus and simulate a transaction submission to consensus.
            if transactions.is_empty() {
                return Ok((
                    vec![ConsensusPosition::ping(epoch_store.epoch(), BlockRef::MIN)],
                    with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                ));
            }

            let num_transactions = transactions.len();
            let mut executed_via_checkpoint = 0;

            // Simple processing - just mark transactions for checkpoint execution if needed
            for txn in transactions {
                if let ConsensusTransactionKind::CertifiedTransaction(cert) = &txn.kind {
                    let transaction_digest = cert.digest();
                    if self.process_via_checkpoint.contains(transaction_digest) {
                        epoch_store
                            .insert_finalized_transactions(vec![*transaction_digest].as_slice(), 10)
                            .expect("Should not fail");
                        executed_via_checkpoint += 1;
                    }
                } else if let ConsensusTransactionKind::UserTransaction(tx) = &txn.kind {
                    let transaction_digest = tx.digest();
                    if self.process_via_checkpoint.contains(transaction_digest) {
                        epoch_store
                            .insert_finalized_transactions(vec![*transaction_digest].as_slice(), 10)
                            .expect("Should not fail");
                        executed_via_checkpoint += 1;
                    }
                } else if let ConsensusTransactionKind::UserTransactionV2(tx) = &txn.kind {
                    let transaction_digest = tx.tx().digest();
                    if self.process_via_checkpoint.contains(transaction_digest) {
                        epoch_store
                            .insert_finalized_transactions(vec![*transaction_digest].as_slice(), 10)
                            .expect("Should not fail");
                        executed_via_checkpoint += 1;
                    }
                }
            }

            let sequenced_transactions: Vec<SequencedConsensusTransaction> = transactions
                .iter()
                .map(|txn| SequencedConsensusTransaction::new_test(txn.clone()))
                .collect();

            let keys = sequenced_transactions
                .iter()
                .map(|tx| tx.key())
                .collect::<Vec<_>>();

            // Only execute transactions if explicitly requested and not via checkpoint
            if self.execute {
                for tx in sequenced_transactions {
                    if let Some(transaction_digest) = tx.transaction.executable_transaction_digest()
                    {
                        // Skip if already executed via checkpoint
                        if self.process_via_checkpoint.contains(&transaction_digest) {
                            continue;
                        }

                        // Extract executable transaction from consensus transaction
                        let executable_tx = match &tx.transaction {
                            SequencedConsensusTransactionKind::External(ext) => match &ext.kind {
                                ConsensusTransactionKind::CertifiedTransaction(cert) => {
                                    Some(VerifiedExecutableTransaction::new_from_certificate(
                                        VerifiedCertificate::new_unchecked(*cert.clone()),
                                    ))
                                }
                                ConsensusTransactionKind::UserTransaction(tx) => {
                                    Some(VerifiedExecutableTransaction::new_from_consensus(
                                        VerifiedTransaction::new_unchecked(*tx.clone()),
                                        0,
                                    ))
                                }
                                ConsensusTransactionKind::UserTransactionV2(tx) => {
                                    Some(VerifiedExecutableTransaction::new_from_consensus(
                                        VerifiedTransaction::new_unchecked(tx.tx().clone()),
                                        0,
                                    ))
                                }
                                _ => None,
                            },
                            SequencedConsensusTransactionKind::System(sys_tx) => {
                                Some(sys_tx.clone())
                            }
                        };

                        if let Some(exec_tx) = executable_tx {
                            let versions = epoch_store.assign_shared_object_versions_for_tests(
                                self.state.get_object_cache_reader().as_ref(),
                                &vec![exec_tx.clone()],
                            )?;

                            let assigned_version = versions
                                .into_map()
                                .into_iter()
                                .next()
                                .map(|(_, v)| v)
                                .unwrap_or_default();

                            self.state.execution_scheduler().enqueue(
                                vec![(
                                    Schedulable::Transaction(exec_tx),
                                    ExecutionEnv::new().with_assigned_versions(assigned_version),
                                )],
                                epoch_store,
                            );
                        }
                    }
                }
            }

            epoch_store.process_notifications(keys.iter());

            assert_eq!(
                executed_via_checkpoint,
                self.process_via_checkpoint.len(),
                "Some transactions were not executed via checkpoint"
            );

            assert!(
                !self.mock_block_status_receivers.lock().is_empty(),
                "No mock submit responses left"
            );

            let mut consensus_positions = Vec::new();
            for index in 0..num_transactions {
                consensus_positions.push(ConsensusPosition {
                    epoch: epoch_store.epoch(),
                    index: index as u16,
                    block: BlockRef::MIN,
                });
            }

            Ok((
                consensus_positions,
                self.mock_block_status_receivers.lock().remove(0),
            ))
        }
    }
    let epoch_store = state.epoch_store_for_testing();
    // Make a new consensus adapter instance.
    Arc::new(ConsensusAdapter::new(
        Arc::new(SubmitDirectly {
            state: state.clone(),
            process_via_checkpoint,
            execute,
            mock_block_status_receivers: Arc::new(Mutex::new(mock_block_status_receivers)),
        }),
        state.checkpoint_store.clone(),
        state.name,
        Arc::new(ConnectionMonitorStatusForTests {}),
        100_000,
        100_000,
        None,
        None,
        metrics,
        epoch_store.protocol_config().clone(),
    ))
}

/// Creates a ConsensusHandler for testing with a mock ExecutionSchedulerSender that captures transactions
pub async fn setup_consensus_handler_for_testing_with_checkpoint_service<C>(
    authority: &Arc<AuthorityState>,
    checkpoint_service: Arc<C>,
) -> TestConsensusHandlerSetup<C>
where
    C: Send + Sync + 'static,
{
    let epoch_store = authority.epoch_store_for_testing();
    let consensus_committee = epoch_store.epoch_start_state().get_consensus_committee();
    let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
    let throughput_calculator = ConsensusThroughputCalculator::new(None, metrics.clone());
    let backpressure_manager = BackpressureManager::new_for_tests();
    let consensus_adapter =
        make_consensus_adapter_for_test(authority.clone(), HashSet::new(), false, vec![]);

    let last_consensus_stats = ExecutionIndicesWithStats {
        stats: crate::authority::authority_per_epoch_store::ConsensusStats::new(
            consensus_committee.size(),
        ),
        ..Default::default()
    };

    // Create a test ExecutionSchedulerSender that captures transactions
    let captured_transactions = Arc::new(Mutex::new(Vec::<(
        Vec<Schedulable>,
        AssignedTxAndVersions,
        SchedulingSource,
    )>::new()));
    let captured_tx_clone = captured_transactions.clone();

    // Create a channel to capture sent transactions
    let (tx_sender, mut receiver) =
        mysten_metrics::monitored_mpsc::unbounded_channel("test_execution_scheduler");

    // Spawn a task to capture transactions from the channel
    tokio::spawn(async move {
        while let Some(item) = receiver.recv().await {
            captured_tx_clone.lock().push(item);
        }
    });

    let execution_scheduler_sender = ExecutionSchedulerSender::new_for_testing(tx_sender);

    let consensus_handler = ConsensusHandler::new_for_testing(
        epoch_store.clone(),
        checkpoint_service,
        execution_scheduler_sender,
        consensus_adapter,
        authority.get_object_cache_reader().clone(),
        Arc::new(ArcSwap::default()),
        consensus_committee,
        metrics,
        Arc::new(throughput_calculator),
        backpressure_manager.subscribe(),
        authority.traffic_controller.clone(),
        last_consensus_stats,
    );

    TestConsensusHandlerSetup {
        consensus_handler,
        captured_transactions,
    }
}

/// Creates a ConsensusHandler for testing with CheckpointServiceNoop
#[cfg(test)]
pub async fn setup_consensus_handler_for_testing(
    authority: &Arc<AuthorityState>,
) -> TestConsensusHandlerSetup<crate::checkpoints::CheckpointServiceNoop> {
    setup_consensus_handler_for_testing_with_checkpoint_service(
        authority,
        Arc::new(crate::checkpoints::CheckpointServiceNoop {}),
    )
    .await
}
