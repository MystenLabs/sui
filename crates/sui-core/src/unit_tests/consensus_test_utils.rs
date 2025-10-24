// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Common test utilities for consensus handler testing

use crate::authority::authority_per_epoch_store::ExecutionIndicesWithStats;
use crate::authority::backpressure::BackpressureManager;
use crate::authority::shared_object_version_manager::{AssignedTxAndVersions, Schedulable};
use crate::authority::{AuthorityMetrics, AuthorityState};
use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_adapter::consensus_tests::make_consensus_adapter_for_test;
use crate::consensus_handler::{ConsensusHandler, ExecutionSchedulerSender};
use crate::consensus_throughput_calculator::ConsensusThroughputCalculator;
use crate::consensus_types::consensus_output_api::{ConsensusCommitAPI, ParsedTransaction};
use crate::execution_scheduler::SchedulingSource;
use arc_swap::ArcSwap;
use consensus_types::block::BlockRef;
use parking_lot::Mutex;
use prometheus::Registry;
use std::collections::HashSet;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::digests::ConsensusCommitDigest;
use sui_types::messages_consensus::{AuthorityIndex, ConsensusTransaction};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;

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

    fn consensus_digest(&self, _protocol_config: &ProtocolConfig) -> ConsensusCommitDigest {
        ConsensusCommitDigest::default()
    }
}

pub struct TestConsensusHandlerSetup<C> {
    pub consensus_handler: ConsensusHandler<C>,
    pub captured_transactions: CapturedTransactions,
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
pub async fn setup_consensus_handler_for_testing(
    authority: &Arc<AuthorityState>,
) -> TestConsensusHandlerSetup<CheckpointServiceNoop> {
    setup_consensus_handler_for_testing_with_checkpoint_service(
        authority,
        Arc::new(CheckpointServiceNoop {}),
    )
    .await
}
