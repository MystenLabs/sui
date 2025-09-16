// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use arc_swap::ArcSwap;
use consensus_config::Committee as ConsensusCommittee;
use consensus_core::{CertifiedBlocksOutput, CommitConsumerMonitor, CommitIndex};
use consensus_types::block::TransactionIndex;
use itertools::Itertools as _;
use lru::LruCache;
use mysten_common::{debug_fatal, random_util::randomize_cache_capacity_in_tests};
use mysten_metrics::{
    monitored_future,
    monitored_mpsc::{self, UnboundedReceiver},
    monitored_scope, spawn_monitored_task,
};
use serde::{Deserialize, Serialize};
use sui_macros::{fail_point, fail_point_if};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    authenticator_state::ActiveJwk,
    base_types::{
        AuthorityName, ConsensusObjectSequenceKey, EpochId, SequenceNumber, TransactionDigest,
    },
    digests::{AdditionalConsensusStateDigest, ConsensusCommitDigest},
    executable_transaction::{TrustedExecutableTransaction, VerifiedExecutableTransaction},
    messages_consensus::{
        AuthorityIndex, ConsensusDeterminedVersionAssignments, ConsensusPosition,
        ConsensusTransaction, ConsensusTransactionKey, ConsensusTransactionKind,
        ExecutionTimeObservation,
    },
    sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait,
    transaction::{SenderSignedData, VerifiedTransaction},
};
use tokio::task::JoinSet;
use tracing::{debug, error, info, instrument, trace_span, warn};

use crate::{
    authority::{
        authority_per_epoch_store::{
            AuthorityPerEpochStore, ConsensusStats, ConsensusStatsAPI, ExecutionIndices,
            ExecutionIndicesWithStats,
        },
        backpressure::{BackpressureManager, BackpressureSubscriber},
        consensus_tx_status_cache::ConsensusTxStatus,
        epoch_start_configuration::EpochStartConfigTrait,
        shared_object_version_manager::{AssignedTxAndVersions, Schedulable},
        AuthorityMetrics, AuthorityState, ExecutionEnv,
    },
    checkpoints::{CheckpointService, CheckpointServiceNotify},
    consensus_adapter::ConsensusAdapter,
    consensus_throughput_calculator::ConsensusThroughputCalculator,
    consensus_types::consensus_output_api::{parse_block_transactions, ConsensusCommitAPI},
    execution_cache::ObjectCacheRead,
    execution_scheduler::{ExecutionScheduler, SchedulingSource},
    scoring_decision::update_low_scoring_authorities,
    traffic_controller::{policies::TrafficTally, TrafficController},
};

pub struct ConsensusHandlerInitializer {
    state: Arc<AuthorityState>,
    checkpoint_service: Arc<CheckpointService>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    consensus_adapter: Arc<ConsensusAdapter>,
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    throughput_calculator: Arc<ConsensusThroughputCalculator>,
    backpressure_manager: Arc<BackpressureManager>,
}

impl ConsensusHandlerInitializer {
    pub fn new(
        state: Arc<AuthorityState>,
        checkpoint_service: Arc<CheckpointService>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_adapter: Arc<ConsensusAdapter>,
        low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
        throughput_calculator: Arc<ConsensusThroughputCalculator>,
        backpressure_manager: Arc<BackpressureManager>,
    ) -> Self {
        Self {
            state,
            checkpoint_service,
            epoch_store,
            consensus_adapter,
            low_scoring_authorities,
            throughput_calculator,
            backpressure_manager,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_testing(
        state: Arc<AuthorityState>,
        checkpoint_service: Arc<CheckpointService>,
    ) -> Self {
        use crate::consensus_adapter::consensus_tests::make_consensus_adapter_for_test;
        use std::collections::HashSet;

        let backpressure_manager = BackpressureManager::new_for_tests();
        let consensus_adapter =
            make_consensus_adapter_for_test(state.clone(), HashSet::new(), false, vec![]);
        Self {
            state: state.clone(),
            checkpoint_service,
            epoch_store: state.epoch_store_for_testing().clone(),
            consensus_adapter,
            low_scoring_authorities: Arc::new(Default::default()),
            throughput_calculator: Arc::new(ConsensusThroughputCalculator::new(
                None,
                state.metrics.clone(),
            )),
            backpressure_manager,
        }
    }

    pub(crate) fn new_consensus_handler(&self) -> ConsensusHandler<CheckpointService> {
        let new_epoch_start_state = self.epoch_store.epoch_start_state();
        let consensus_committee = new_epoch_start_state.get_consensus_committee();

        ConsensusHandler::new(
            self.epoch_store.clone(),
            self.checkpoint_service.clone(),
            self.state.execution_scheduler().clone(),
            self.consensus_adapter.clone(),
            self.state.get_object_cache_reader().clone(),
            self.low_scoring_authorities.clone(),
            consensus_committee,
            self.state.metrics.clone(),
            self.throughput_calculator.clone(),
            self.backpressure_manager.subscribe(),
            self.state.traffic_controller.clone(),
        )
    }

    pub(crate) fn metrics(&self) -> &Arc<AuthorityMetrics> {
        &self.state.metrics
    }

    pub(crate) fn backpressure_subscriber(&self) -> BackpressureSubscriber {
        self.backpressure_manager.subscribe()
    }
}

mod additional_consensus_state {
    use std::marker::PhantomData;

    use fastcrypto::hash::HashFunction as _;
    use sui_types::crypto::DefaultHash;

    use super::*;
    /// AdditionalConsensusState tracks any in-memory state that is retained by ConsensusHandler
    /// between consensus commits. Because of crash recovery, using such data is inherently risky.
    /// In order to do this safely, we must store data from a fixed number of previous commits.
    /// Then, at start-up, that same fixed number of already processed commits is replayed to
    /// reconstruct the state.
    ///
    /// To make sure that bugs in this process appear immediately, we record the digest of this
    /// state in ConsensusCommitPrologue, so that any deviation causes an immediate fork.
    #[derive(Serialize, Deserialize)]
    pub(super) struct AdditionalConsensusState {
        commit_interval_observer: CommitIntervalObserver,
    }

    impl AdditionalConsensusState {
        pub fn new(additional_consensus_state_window_size: u32) -> Self {
            Self {
                commit_interval_observer: CommitIntervalObserver::new(
                    additional_consensus_state_window_size,
                ),
            }
        }

        /// Update all internal state based on the new commit
        pub(crate) fn observe_commit(
            &mut self,
            protocol_config: &ProtocolConfig,
            consensus_commit: &impl ConsensusCommitAPI,
        ) -> ConsensusCommitInfo {
            self.commit_interval_observer
                .observe_commit_time(consensus_commit);

            let estimated_commit_period = self
                .commit_interval_observer
                .commit_interval_estimate()
                .unwrap_or(Duration::from_millis(
                    protocol_config.min_checkpoint_interval_ms(),
                ));

            ConsensusCommitInfo {
                _phantom: PhantomData,
                round: consensus_commit.leader_round(),
                timestamp: consensus_commit.commit_timestamp_ms(),
                consensus_commit_digest: consensus_commit.consensus_digest(protocol_config),
                additional_state_digest: Some(self.digest()),
                estimated_commit_period: Some(estimated_commit_period),
                skip_consensus_commit_prologue_in_test: false,
            }
        }

        pub(crate) fn stateless_commit_info(
            &self,
            protocol_config: &ProtocolConfig,
            consensus_commit: &impl ConsensusCommitAPI,
        ) -> ConsensusCommitInfo {
            ConsensusCommitInfo {
                _phantom: PhantomData,
                round: consensus_commit.leader_round(),
                timestamp: consensus_commit.commit_timestamp_ms(),
                consensus_commit_digest: consensus_commit.consensus_digest(protocol_config),
                additional_state_digest: None,
                estimated_commit_period: None,
                skip_consensus_commit_prologue_in_test: false,
            }
        }

        /// Get the digest of the current state.
        fn digest(&self) -> AdditionalConsensusStateDigest {
            let mut hash = DefaultHash::new();
            bcs::serialize_into(&mut hash, self).unwrap();
            AdditionalConsensusStateDigest::new(hash.finalize().into())
        }
    }

    pub struct ConsensusCommitInfo {
        // prevent public construction
        _phantom: PhantomData<()>,

        pub round: u64,
        pub timestamp: u64,
        pub consensus_commit_digest: ConsensusCommitDigest,

        additional_state_digest: Option<AdditionalConsensusStateDigest>,
        estimated_commit_period: Option<Duration>,

        pub skip_consensus_commit_prologue_in_test: bool,
    }

    impl ConsensusCommitInfo {
        pub fn new_for_test(
            commit_round: u64,
            commit_timestamp: u64,
            estimated_commit_period: Option<Duration>,
            skip_consensus_commit_prologue_in_test: bool,
        ) -> Self {
            Self {
                _phantom: PhantomData,
                round: commit_round,
                timestamp: commit_timestamp,
                consensus_commit_digest: ConsensusCommitDigest::default(),
                additional_state_digest: Some(AdditionalConsensusStateDigest::ZERO),
                estimated_commit_period,
                skip_consensus_commit_prologue_in_test,
            }
        }

        pub fn new_for_congestion_test(
            commit_round: u64,
            commit_timestamp: u64,
            estimated_commit_period: Duration,
        ) -> Self {
            Self::new_for_test(
                commit_round,
                commit_timestamp,
                Some(estimated_commit_period),
                true,
            )
        }

        pub fn additional_state_digest(&self) -> AdditionalConsensusStateDigest {
            // this method cannot be called if stateless_commit_info is used
            self.additional_state_digest
                .expect("additional_state_digest is not available")
        }

        pub fn estimated_commit_period(&self) -> Duration {
            // this method cannot be called if stateless_commit_info is used
            self.estimated_commit_period
                .expect("estimated commit period is not available")
        }

        fn consensus_commit_prologue_transaction(
            &self,
            epoch: u64,
        ) -> VerifiedExecutableTransaction {
            let transaction = VerifiedTransaction::new_consensus_commit_prologue(
                epoch,
                self.round,
                self.timestamp,
            );
            VerifiedExecutableTransaction::new_system(transaction, epoch)
        }

        fn consensus_commit_prologue_v2_transaction(
            &self,
            epoch: u64,
        ) -> VerifiedExecutableTransaction {
            let transaction = VerifiedTransaction::new_consensus_commit_prologue_v2(
                epoch,
                self.round,
                self.timestamp,
                self.consensus_commit_digest,
            );
            VerifiedExecutableTransaction::new_system(transaction, epoch)
        }

        fn consensus_commit_prologue_v3_transaction(
            &self,
            epoch: u64,
            consensus_determined_version_assignments: ConsensusDeterminedVersionAssignments,
        ) -> VerifiedExecutableTransaction {
            let transaction = VerifiedTransaction::new_consensus_commit_prologue_v3(
                epoch,
                self.round,
                self.timestamp,
                self.consensus_commit_digest,
                consensus_determined_version_assignments,
            );
            VerifiedExecutableTransaction::new_system(transaction, epoch)
        }

        fn consensus_commit_prologue_v4_transaction(
            &self,
            epoch: u64,
            consensus_determined_version_assignments: ConsensusDeterminedVersionAssignments,
            additional_state_digest: AdditionalConsensusStateDigest,
        ) -> VerifiedExecutableTransaction {
            let transaction = VerifiedTransaction::new_consensus_commit_prologue_v4(
                epoch,
                self.round,
                self.timestamp,
                self.consensus_commit_digest,
                consensus_determined_version_assignments,
                additional_state_digest,
            );
            VerifiedExecutableTransaction::new_system(transaction, epoch)
        }

        pub fn create_consensus_commit_prologue_transaction(
            &self,
            epoch: u64,
            protocol_config: &ProtocolConfig,
            cancelled_txn_version_assignment: Vec<(
                TransactionDigest,
                Vec<(ConsensusObjectSequenceKey, SequenceNumber)>,
            )>,
            commit_info: &ConsensusCommitInfo,
            indirect_state_observer: IndirectStateObserver,
        ) -> VerifiedExecutableTransaction {
            let version_assignments = if protocol_config
                .record_consensus_determined_version_assignments_in_prologue_v2()
            {
                Some(
                    ConsensusDeterminedVersionAssignments::CancelledTransactionsV2(
                        cancelled_txn_version_assignment,
                    ),
                )
            } else if protocol_config.record_consensus_determined_version_assignments_in_prologue()
            {
                Some(
                    ConsensusDeterminedVersionAssignments::CancelledTransactions(
                        cancelled_txn_version_assignment
                            .into_iter()
                            .map(|(tx_digest, versions)| {
                                (
                                    tx_digest,
                                    versions.into_iter().map(|(id, v)| (id.0, v)).collect(),
                                )
                            })
                            .collect(),
                    ),
                )
            } else {
                None
            };

            if protocol_config.record_additional_state_digest_in_prologue() {
                let additional_state_digest =
                    if protocol_config.additional_consensus_digest_indirect_state() {
                        let d1 = commit_info.additional_state_digest();
                        indirect_state_observer.fold_with(d1)
                    } else {
                        commit_info.additional_state_digest()
                    };

                self.consensus_commit_prologue_v4_transaction(
                    epoch,
                    version_assignments.unwrap(),
                    additional_state_digest,
                )
            } else if let Some(version_assignments) = version_assignments {
                self.consensus_commit_prologue_v3_transaction(epoch, version_assignments)
            } else if protocol_config.include_consensus_digest_in_prologue() {
                self.consensus_commit_prologue_v2_transaction(epoch)
            } else {
                self.consensus_commit_prologue_transaction(epoch)
            }
        }
    }

    #[derive(Default)]
    pub struct IndirectStateObserver {
        hash: DefaultHash,
    }

    impl IndirectStateObserver {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn observe_indirect_state<T: Serialize>(&mut self, state: &T) {
            bcs::serialize_into(&mut self.hash, state).unwrap();
        }

        pub fn fold_with(
            self,
            d1: AdditionalConsensusStateDigest,
        ) -> AdditionalConsensusStateDigest {
            let hash = self.hash.finalize();
            let d2 = AdditionalConsensusStateDigest::new(hash.into());

            let mut hasher = DefaultHash::new();
            bcs::serialize_into(&mut hasher, &d1).unwrap();
            bcs::serialize_into(&mut hasher, &d2).unwrap();
            AdditionalConsensusStateDigest::new(hasher.finalize().into())
        }
    }

    #[test]
    fn test_additional_consensus_state() {
        use crate::consensus_types::consensus_output_api::ParsedTransaction;

        #[derive(Debug)]
        struct TestConsensusCommit {
            round: u64,
            timestamp: u64,
        }

        impl std::fmt::Display for TestConsensusCommit {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "TestConsensusCommitAPI(round={}, timestamp={})",
                    self.round, self.timestamp
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

            /// Returns epoch UNIX timestamp in milliseconds
            fn commit_timestamp_ms(&self) -> u64 {
                self.timestamp
            }

            /// Returns a unique global index for each committed sub-dag.
            fn commit_sub_dag_index(&self) -> u64 {
                self.round
            }

            /// Returns all accepted and rejected transactions per block in the commit in deterministic order.
            fn transactions(
                &self,
            ) -> Vec<(consensus_types::block::BlockRef, Vec<ParsedTransaction>)> {
                vec![]
            }

            /// Returns the digest of consensus output.
            fn consensus_digest(&self, _: &ProtocolConfig) -> ConsensusCommitDigest {
                ConsensusCommitDigest::ZERO
            }
        }

        fn observe(state: &mut AdditionalConsensusState, round: u64, timestamp: u64) {
            let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
            state.observe_commit(&protocol_config, &TestConsensusCommit { round, timestamp });
        }

        let mut s1 = AdditionalConsensusState::new(3);
        observe(&mut s1, 1, 1000);
        observe(&mut s1, 2, 2000);
        observe(&mut s1, 3, 3000);
        observe(&mut s1, 4, 4000);

        let mut s2 = AdditionalConsensusState::new(3);
        // Because state uses a ring buffer, we should get the same digest
        // even though we only added the 3 latest observations.
        observe(&mut s2, 2, 2000);
        observe(&mut s2, 3, 3000);
        observe(&mut s2, 4, 4000);

        assert_eq!(s1.digest(), s2.digest());

        observe(&mut s1, 5, 5000);
        observe(&mut s2, 5, 5000);

        assert_eq!(s1.digest(), s2.digest());
    }
}
use additional_consensus_state::AdditionalConsensusState;
pub(crate) use additional_consensus_state::{ConsensusCommitInfo, IndirectStateObserver};

pub struct ConsensusHandler<C> {
    /// A store created for each epoch. ConsensusHandler is recreated each epoch, with the
    /// corresponding store. This store is also used to get the current epoch ID.
    epoch_store: Arc<AuthorityPerEpochStore>,
    /// Holds the indices, hash and stats after the last consensus commit
    /// It is used for avoiding replaying already processed transactions,
    /// checking chain consistency, and accumulating per-epoch consensus output stats.
    last_consensus_stats: ExecutionIndicesWithStats,
    checkpoint_service: Arc<C>,
    /// cache reader is needed when determining the next version to assign for shared objects.
    cache_reader: Arc<dyn ObjectCacheRead>,
    /// Reputation scores used by consensus adapter that we update, forwarded from consensus
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    /// The consensus committee used to do stake computations for deciding set of low scoring authorities
    committee: ConsensusCommittee,
    // TODO: ConsensusHandler doesn't really share metrics with AuthorityState. We could define
    // a new metrics type here if we want to.
    metrics: Arc<AuthorityMetrics>,
    /// Lru cache to quickly discard transactions processed by consensus
    processed_cache: LruCache<SequencedConsensusTransactionKey, ()>,
    /// Enqueues transactions to the execution scheduler via a separate task.
    execution_scheduler_sender: ExecutionSchedulerSender,
    /// Consensus adapter for submitting transactions to consensus
    consensus_adapter: Arc<ConsensusAdapter>,

    /// Using the throughput calculator to record the current consensus throughput
    throughput_calculator: Arc<ConsensusThroughputCalculator>,

    additional_consensus_state: AdditionalConsensusState,

    backpressure_subscriber: BackpressureSubscriber,

    traffic_controller: Option<Arc<TrafficController>>,
}

const PROCESSED_CACHE_CAP: usize = 1024 * 1024;

impl<C> ConsensusHandler<C> {
    pub(crate) fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint_service: Arc<C>,
        execution_scheduler: Arc<ExecutionScheduler>,
        consensus_adapter: Arc<ConsensusAdapter>,
        cache_reader: Arc<dyn ObjectCacheRead>,
        low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
        committee: ConsensusCommittee,
        metrics: Arc<AuthorityMetrics>,
        throughput_calculator: Arc<ConsensusThroughputCalculator>,
        backpressure_subscriber: BackpressureSubscriber,
        traffic_controller: Option<Arc<TrafficController>>,
    ) -> Self {
        // Recover last_consensus_stats so it is consistent across validators.
        let mut last_consensus_stats = epoch_store
            .get_last_consensus_stats()
            .expect("Should be able to read last consensus index");
        // stats is empty at the beginning of epoch.
        if !last_consensus_stats.stats.is_initialized() {
            last_consensus_stats.stats = ConsensusStats::new(committee.size());
        }
        let execution_scheduler_sender =
            ExecutionSchedulerSender::start(execution_scheduler, epoch_store.clone());
        let commit_rate_estimate_window_size = epoch_store
            .protocol_config()
            .get_consensus_commit_rate_estimation_window_size();
        Self {
            epoch_store,
            last_consensus_stats,
            checkpoint_service,
            cache_reader,
            low_scoring_authorities,
            committee,
            metrics,
            processed_cache: LruCache::new(
                NonZeroUsize::new(randomize_cache_capacity_in_tests(PROCESSED_CACHE_CAP)).unwrap(),
            ),
            execution_scheduler_sender,
            consensus_adapter,
            throughput_calculator,
            additional_consensus_state: AdditionalConsensusState::new(
                commit_rate_estimate_window_size,
            ),
            backpressure_subscriber,
            traffic_controller,
        }
    }

    /// Returns the last subdag index processed by the handler.
    pub(crate) fn last_processed_subdag_index(&self) -> u64 {
        self.last_consensus_stats.index.sub_dag_index
    }

    pub(crate) fn execution_scheduler_sender(&self) -> &ExecutionSchedulerSender {
        &self.execution_scheduler_sender
    }
}

impl<C: CheckpointServiceNotify + Send + Sync> ConsensusHandler<C> {
    /// Called during startup to allow us to observe commits we previously processed, for crash recovery.
    /// Any state computed here must be a pure function of the commits observed, it cannot depend on any
    /// state recorded in the epoch db.
    fn handle_prior_consensus_commit(&mut self, consensus_commit: impl ConsensusCommitAPI) {
        assert!(self
            .epoch_store
            .protocol_config()
            .record_additional_state_digest_in_prologue());
        self.additional_consensus_state
            .observe_commit(self.epoch_store.protocol_config(), &consensus_commit);
    }

    #[instrument(level = "debug", skip_all)]
    async fn handle_consensus_commit(&mut self, consensus_commit: impl ConsensusCommitAPI) {
        // This may block until one of two conditions happens:
        // - Number of uncommitted transactions in the writeback cache goes below the
        //   backpressure threshold.
        // - The highest executed checkpoint catches up to the highest certified checkpoint.
        self.backpressure_subscriber.await_no_backpressure().await;

        let _scope = monitored_scope("ConsensusCommitHandler::handle_consensus_commit");

        let last_committed_round = self.last_consensus_stats.index.last_committed_round;

        if let Some(consensus_tx_status_cache) = self.epoch_store.consensus_tx_status_cache.as_ref()
        {
            consensus_tx_status_cache
                .update_last_committed_leader_round(last_committed_round as u32)
                .await;
        }
        if let Some(tx_reject_reason_cache) = self.epoch_store.tx_reject_reason_cache.as_ref() {
            tx_reject_reason_cache.set_last_committed_leader_round(last_committed_round as u32);
        }

        let commit_info = if self
            .epoch_store
            .protocol_config()
            .record_additional_state_digest_in_prologue()
        {
            let commit_info = self
                .additional_consensus_state
                .observe_commit(self.epoch_store.protocol_config(), &consensus_commit);
            info!(
                "estimated commit rate: {:?}",
                commit_info.estimated_commit_period()
            );
            commit_info
        } else {
            self.additional_consensus_state
                .stateless_commit_info(self.epoch_store.protocol_config(), &consensus_commit)
        };

        // TODO: Remove this once narwhal is deprecated. For now mysticeti will not return
        // more than one leader per round so we are not in danger of ignoring any commits.
        assert!(commit_info.round >= last_committed_round);
        if last_committed_round == commit_info.round {
            // we can receive the same commit twice after restart
            // It is critical that the writes done by this function are atomic - otherwise we can
            // lose the later parts of a commit if we restart midway through processing it.
            warn!(
                "Ignoring consensus output for round {} as it is already committed. NOTE: This is only expected if consensus is running.",
                commit_info.round
            );
            return;
        }

        /* (transaction, serialized length) */
        let epoch = self.epoch_store.epoch();
        let mut transactions = vec![];
        let timestamp = consensus_commit.commit_timestamp_ms();
        let leader_author = consensus_commit.leader_author_index();
        let commit_sub_dag_index = consensus_commit.commit_sub_dag_index();

        let system_time_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let consensus_timestamp_bias_ms = system_time_ms - (timestamp as i64);
        let consensus_timestamp_bias_seconds = consensus_timestamp_bias_ms as f64 / 1000.0;
        self.metrics
            .consensus_timestamp_bias
            .observe(consensus_timestamp_bias_seconds);

        let epoch_start = self
            .epoch_store
            .epoch_start_config()
            .epoch_start_timestamp_ms();
        let timestamp = if timestamp < epoch_start {
            error!(
                "Unexpected commit timestamp {timestamp} less then epoch start time {epoch_start}, author {leader_author}, round {}",
                commit_info.round
            );
            epoch_start
        } else {
            timestamp
        };

        info!(
            %consensus_commit,
            epoch = ?self.epoch_store.epoch(),
            "Received consensus output"
        );

        let execution_index = ExecutionIndices {
            last_committed_round: commit_info.round,
            sub_dag_index: commit_sub_dag_index,
            transaction_index: 0_u64,
        };
        // This function has filtered out any already processed consensus output.
        // So we can safely assume that the index is always increasing.
        assert!(self.last_consensus_stats.index < execution_index);

        // TODO: test empty commit explicitly.
        // Note that consensus commit batch may contain no transactions, but we still need to record the current
        // round and subdag index in the last_consensus_stats, so that it won't be re-executed in the future.
        self.last_consensus_stats.index = execution_index;

        // Load all jwks that became active in the previous round, and commit them in this round.
        // We want to delay one round because none of the transactions in the previous round could
        // have been authenticated with the jwks that became active in that round.
        //
        // Because of this delay, jwks that become active in the last round of the epoch will
        // never be committed. That is ok, because in the new epoch, the validators should
        // immediately re-submit these jwks, and they can become active then.
        let new_jwks = self
            .epoch_store
            .get_new_jwks(last_committed_round)
            .expect("Unrecoverable error in consensus handler");

        if !new_jwks.is_empty() {
            let authenticator_state_update_transaction =
                self.authenticator_state_update_transaction(commit_info.round, new_jwks);
            debug!(
                "adding AuthenticatorStateUpdate({:?}) tx: {:?}",
                authenticator_state_update_transaction.digest(),
                authenticator_state_update_transaction,
            );

            transactions.push((
                SequencedConsensusTransactionKind::System(authenticator_state_update_transaction),
                leader_author,
            ));
        }

        update_low_scoring_authorities(
            self.low_scoring_authorities.clone(),
            self.epoch_store.committee(),
            &self.committee,
            consensus_commit.reputation_score_sorted_desc(),
            &self.metrics,
            self.epoch_store
                .protocol_config()
                .consensus_bad_nodes_stake_threshold(),
        );

        self.metrics
            .consensus_committed_subdags
            .with_label_values(&[&leader_author.to_string()])
            .inc();

        let mut num_finalized_user_transactions = vec![0; self.committee.size()];
        let mut num_rejected_user_transactions = vec![0; self.committee.size()];
        {
            let span = trace_span!("ConsensusHandler::HandleCommit::process_consensus_txns");
            let _guard = span.enter();
            for (block, parsed_transactions) in consensus_commit.transactions() {
                let author = block.author.value();
                // TODO: consider only messages within 1~3 rounds of the leader?
                self.last_consensus_stats.stats.inc_num_messages(author);
                for (tx_index, parsed) in parsed_transactions.into_iter().enumerate() {
                    let position = ConsensusPosition {
                        epoch,
                        block,
                        index: tx_index as TransactionIndex,
                    };

                    // Transaction has appeared in consensus output, we can increment the submission count
                    // for this tx for DoS protection.
                    if self.epoch_store.protocol_config().mysticeti_fastpath() {
                        if let ConsensusTransactionKind::UserTransaction(tx) =
                            &parsed.transaction.kind
                        {
                            let digest = tx.digest();
                            if let Some((spam_weight, submitter_client_addrs)) = self
                                .epoch_store
                                .submitted_transaction_cache
                                .increment_submission_count(digest)
                            {
                                if let Some(ref traffic_controller) = self.traffic_controller {
                                    debug!(
                                        "Transaction {digest} exceeded submission limits, spam_weight: {spam_weight:?} applied to {} client addresses",
                                        submitter_client_addrs.len()
                                    );

                                    // Apply spam weight to all client addresses that submitted this transaction
                                    for addr in submitter_client_addrs {
                                        traffic_controller.tally(TrafficTally::new(
                                            Some(addr),
                                            None,
                                            None,
                                            spam_weight.clone(),
                                        ));
                                    }
                                } else {
                                    warn!(
                                        "Transaction {digest} exceeded submission limits, spam_weight: {spam_weight:?} for {} client addresses (traffic controller not configured)",
                                        submitter_client_addrs.len()
                                    );
                                }
                            }
                        }
                    }

                    if parsed.rejected {
                        if parsed.transaction.kind.is_user_transaction() {
                            self.epoch_store
                                .set_consensus_tx_status(position, ConsensusTxStatus::Rejected);
                            num_rejected_user_transactions[author] += 1;
                        }
                        // Skip processing rejected transactions.
                        continue;
                    }
                    if parsed.transaction.kind.is_user_transaction() {
                        self.epoch_store
                            .set_consensus_tx_status(position, ConsensusTxStatus::Finalized);
                        num_finalized_user_transactions[author] += 1;
                    }
                    let kind = classify(&parsed.transaction);
                    self.metrics
                        .consensus_handler_processed
                        .with_label_values(&[kind])
                        .inc();
                    self.metrics
                        .consensus_handler_transaction_sizes
                        .with_label_values(&[kind])
                        .observe(parsed.serialized_len as f64);
                    // UserTransaction exists only when mysticeti_fastpath is enabled in protocol config.
                    if matches!(
                        &parsed.transaction.kind,
                        ConsensusTransactionKind::CertifiedTransaction(_)
                            | ConsensusTransactionKind::UserTransaction(_)
                    ) {
                        self.last_consensus_stats
                            .stats
                            .inc_num_user_transactions(author);
                    }
                    if let ConsensusTransactionKind::RandomnessStateUpdate(randomness_round, _) =
                        &parsed.transaction.kind
                    {
                        // These are deprecated and we should never see them. Log an error and eat the tx if one appears.
                        debug_fatal!(
                            "BUG: saw deprecated RandomnessStateUpdate tx for commit round {}, randomness round {}",
                            commit_info.round,
                            randomness_round
                        );
                    } else {
                        let transaction =
                            SequencedConsensusTransactionKind::External(parsed.transaction);
                        transactions.push((transaction, author as u32));
                    }
                }
            }
        }

        for (i, authority) in self.committee.authorities() {
            let hostname = &authority.hostname;
            self.metrics
                .consensus_committed_messages
                .with_label_values(&[hostname])
                .set(self.last_consensus_stats.stats.get_num_messages(i.value()) as i64);
            self.metrics
                .consensus_committed_user_transactions
                .with_label_values(&[hostname])
                .set(
                    self.last_consensus_stats
                        .stats
                        .get_num_user_transactions(i.value()) as i64,
                );
            self.metrics
                .consensus_finalized_user_transactions
                .with_label_values(&[hostname])
                .set(num_finalized_user_transactions[i.value()] as i64);
            self.metrics
                .consensus_rejected_user_transactions
                .with_label_values(&[hostname])
                .set(num_rejected_user_transactions[i.value()] as i64);
        }

        let mut all_transactions = Vec::new();
        {
            // We need a set here as well, since the processed_cache is a LRU cache and can drop
            // entries while we're iterating over the sequenced transactions.
            let mut processed_set = HashSet::new();

            for (seq, (transaction, cert_origin)) in transactions.into_iter().enumerate() {
                // In process_consensus_transactions_and_commit_boundary(), we will add a system consensus commit
                // prologue transaction, which will be the first transaction in this consensus commit batch.
                // Therefore, the transaction sequence number starts from 1 here.
                let current_tx_index = ExecutionIndices {
                    last_committed_round: commit_info.round,
                    sub_dag_index: commit_sub_dag_index,
                    transaction_index: (seq + 1) as u64,
                };

                self.last_consensus_stats.index = current_tx_index;

                let certificate_author = *self
                    .epoch_store
                    .committee()
                    .authority_by_index(cert_origin)
                    .unwrap();

                let sequenced_transaction = SequencedConsensusTransaction {
                    certificate_author_index: cert_origin,
                    certificate_author,
                    consensus_index: current_tx_index,
                    transaction,
                };

                let key = sequenced_transaction.key();
                let in_set = !processed_set.insert(key);
                let in_cache = self
                    .processed_cache
                    .put(sequenced_transaction.key(), ())
                    .is_some();

                if in_set || in_cache {
                    self.metrics.skipped_consensus_txns_cache_hit.inc();
                    continue;
                }

                all_transactions.push(sequenced_transaction);
            }
        }

        let indirect_state_observer = IndirectStateObserver::new();

        let (executable_transactions, assigned_versions) = self
            .epoch_store
            .process_consensus_transactions_and_commit_boundary(
                all_transactions,
                &self.last_consensus_stats,
                &self.checkpoint_service,
                self.cache_reader.as_ref(),
                &commit_info,
                indirect_state_observer,
                &self.metrics,
            )
            .await
            .expect("Unrecoverable error in consensus handler");

        // update the calculated throughput
        self.throughput_calculator
            .add_transactions(timestamp, executable_transactions.len() as u64);

        fail_point_if!("correlated-crash-after-consensus-commit-boundary", || {
            let key = [commit_sub_dag_index, self.epoch_store.epoch()];
            if sui_simulator::random::deterministic_probability_once(&key, 0.01) {
                sui_simulator::task::kill_current_node(None);
            }
        });

        fail_point!("crash"); // for tests that produce random crashes

        self.execution_scheduler_sender.send(
            executable_transactions,
            assigned_versions,
            SchedulingSource::NonFastPath,
        );

        // Check if we should send EndOfPublish after processing consensus commit
        self.send_end_of_publish_if_needed().await;
    }

    async fn send_end_of_publish_if_needed(&self) {
        if !self.epoch_store.should_send_end_of_publish() {
            return;
        }

        let end_of_publish = ConsensusTransaction::new_end_of_publish(self.epoch_store.name);
        if let Err(err) =
            self.consensus_adapter
                .submit(end_of_publish, None, &self.epoch_store, None, None)
        {
            warn!(
                "Error when sending EndOfPublish message from ConsensusHandler: {:?}",
                err
            );
        } else {
            info!(epoch=?self.epoch_store.epoch(), "Sending EndOfPublish message to consensus");
        }
    }
}

/// Sends transactions to the execution scheduler in a separate task,
/// to avoid blocking consensus handler.
#[derive(Clone)]
pub(crate) struct ExecutionSchedulerSender {
    // Using unbounded channel to avoid blocking consensus commit and transaction handler.
    sender: monitored_mpsc::UnboundedSender<(
        Vec<Schedulable>,
        AssignedTxAndVersions,
        SchedulingSource,
    )>,
}

impl ExecutionSchedulerSender {
    fn start(
        execution_scheduler: Arc<ExecutionScheduler>,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Self {
        let (sender, recv) = monitored_mpsc::unbounded_channel("execution_scheduler_sender");
        spawn_monitored_task!(Self::run(recv, execution_scheduler, epoch_store));
        Self { sender }
    }

    fn send(
        &self,
        transactions: Vec<Schedulable>,
        assigned_versions: AssignedTxAndVersions,
        scheduling_source: SchedulingSource,
    ) {
        let _ = self
            .sender
            .send((transactions, assigned_versions, scheduling_source));
    }

    async fn run(
        mut recv: monitored_mpsc::UnboundedReceiver<(
            Vec<Schedulable>,
            AssignedTxAndVersions,
            SchedulingSource,
        )>,
        execution_scheduler: Arc<ExecutionScheduler>,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) {
        while let Some((transactions, assigned_versions, scheduling_source)) = recv.recv().await {
            let _guard = monitored_scope("ConsensusHandler::enqueue");
            let assigned_versions = assigned_versions.into_map();
            let txns = transactions
                .into_iter()
                .map(|txn| {
                    let key = txn.key();
                    (
                        txn,
                        ExecutionEnv::new()
                            .with_scheduling_source(scheduling_source)
                            .with_assigned_versions(
                                assigned_versions.get(&key).cloned().unwrap_or_default(),
                            ),
                    )
                })
                .collect();
            execution_scheduler.enqueue(txns, &epoch_store);
        }
    }
}

/// Manages the lifetime of tasks handling the commits and transactions output by consensus.
pub(crate) struct MysticetiConsensusHandler {
    tasks: JoinSet<()>,
}

impl MysticetiConsensusHandler {
    pub(crate) fn new(
        last_processed_commit_at_startup: CommitIndex,
        mut consensus_handler: ConsensusHandler<CheckpointService>,
        consensus_block_handler: ConsensusBlockHandler,
        mut commit_receiver: UnboundedReceiver<consensus_core::CommittedSubDag>,
        mut block_receiver: UnboundedReceiver<consensus_core::CertifiedBlocksOutput>,
        commit_consumer_monitor: Arc<CommitConsumerMonitor>,
    ) -> Self {
        let mut tasks = JoinSet::new();
        tasks.spawn(monitored_future!(async move {
            // TODO: pause when execution is overloaded, so consensus can detect the backpressure.
            while let Some(consensus_commit) = commit_receiver.recv().await {
                let commit_index = consensus_commit.commit_ref.index;
                if commit_index <= last_processed_commit_at_startup {
                    consensus_handler.handle_prior_consensus_commit(consensus_commit);
                } else {
                    consensus_handler
                        .handle_consensus_commit(consensus_commit)
                        .await;
                }
                commit_consumer_monitor.set_highest_handled_commit(commit_index);
            }
        }));
        if consensus_block_handler.enabled() {
            tasks.spawn(monitored_future!(async move {
                while let Some(blocks) = block_receiver.recv().await {
                    consensus_block_handler
                        .handle_certified_blocks(blocks)
                        .await;
                }
            }));
        }
        Self { tasks }
    }

    pub(crate) async fn abort(&mut self) {
        self.tasks.shutdown().await;
    }
}

impl<C> ConsensusHandler<C> {
    fn authenticator_state_update_transaction(
        &self,
        round: u64,
        mut new_active_jwks: Vec<ActiveJwk>,
    ) -> VerifiedExecutableTransaction {
        new_active_jwks.sort();

        info!("creating authenticator state update transaction");
        assert!(self.epoch_store.authenticator_state_enabled());
        let transaction = VerifiedTransaction::new_authenticator_state_update(
            self.epoch(),
            round,
            new_active_jwks,
            self.epoch_store
                .epoch_start_config()
                .authenticator_obj_initial_shared_version()
                .expect("authenticator state obj must exist"),
        );
        VerifiedExecutableTransaction::new_system(transaction, self.epoch())
    }

    fn epoch(&self) -> EpochId {
        self.epoch_store.epoch()
    }
}

pub(crate) fn classify(transaction: &ConsensusTransaction) -> &'static str {
    match &transaction.kind {
        ConsensusTransactionKind::CertifiedTransaction(certificate) => {
            if certificate.is_consensus_tx() {
                "shared_certificate"
            } else {
                "owned_certificate"
            }
        }
        ConsensusTransactionKind::CheckpointSignature(_) => "checkpoint_signature",
        ConsensusTransactionKind::CheckpointSignatureV2(_) => "checkpoint_signature",
        ConsensusTransactionKind::EndOfPublish(_) => "end_of_publish",
        ConsensusTransactionKind::CapabilityNotification(_) => "capability_notification",
        ConsensusTransactionKind::CapabilityNotificationV2(_) => "capability_notification_v2",
        ConsensusTransactionKind::NewJWKFetched(_, _, _) => "new_jwk_fetched",
        ConsensusTransactionKind::RandomnessStateUpdate(_, _) => "randomness_state_update",
        ConsensusTransactionKind::RandomnessDkgMessage(_, _) => "randomness_dkg_message",
        ConsensusTransactionKind::RandomnessDkgConfirmation(_, _) => "randomness_dkg_confirmation",
        ConsensusTransactionKind::UserTransaction(tx) => {
            if tx.is_consensus_tx() {
                "shared_user_transaction"
            } else {
                "owned_user_transaction"
            }
        }
        ConsensusTransactionKind::ExecutionTimeObservation(_) => "execution_time_observation",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencedConsensusTransaction {
    pub certificate_author_index: AuthorityIndex,
    pub certificate_author: AuthorityName,
    pub consensus_index: ExecutionIndices,
    pub transaction: SequencedConsensusTransactionKind,
}

#[derive(Debug, Clone)]
pub enum SequencedConsensusTransactionKind {
    External(ConsensusTransaction),
    System(VerifiedExecutableTransaction),
}

impl Serialize for SequencedConsensusTransactionKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let serializable = SerializableSequencedConsensusTransactionKind::from(self);
        serializable.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SequencedConsensusTransactionKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let serializable =
            SerializableSequencedConsensusTransactionKind::deserialize(deserializer)?;
        Ok(serializable.into())
    }
}

// We can't serialize SequencedConsensusTransactionKind directly because it contains a
// VerifiedExecutableTransaction, which is not serializable (by design). This wrapper allows us to
// convert to a serializable format easily.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum SerializableSequencedConsensusTransactionKind {
    External(ConsensusTransaction),
    System(TrustedExecutableTransaction),
}

impl From<&SequencedConsensusTransactionKind> for SerializableSequencedConsensusTransactionKind {
    fn from(kind: &SequencedConsensusTransactionKind) -> Self {
        match kind {
            SequencedConsensusTransactionKind::External(ext) => {
                SerializableSequencedConsensusTransactionKind::External(ext.clone())
            }
            SequencedConsensusTransactionKind::System(txn) => {
                SerializableSequencedConsensusTransactionKind::System(txn.clone().serializable())
            }
        }
    }
}

impl From<SerializableSequencedConsensusTransactionKind> for SequencedConsensusTransactionKind {
    fn from(kind: SerializableSequencedConsensusTransactionKind) -> Self {
        match kind {
            SerializableSequencedConsensusTransactionKind::External(ext) => {
                SequencedConsensusTransactionKind::External(ext)
            }
            SerializableSequencedConsensusTransactionKind::System(txn) => {
                SequencedConsensusTransactionKind::System(txn.into())
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Debug, Ord, PartialOrd)]
pub enum SequencedConsensusTransactionKey {
    External(ConsensusTransactionKey),
    System(TransactionDigest),
}

impl SequencedConsensusTransactionKind {
    pub fn key(&self) -> SequencedConsensusTransactionKey {
        match self {
            SequencedConsensusTransactionKind::External(ext) => {
                SequencedConsensusTransactionKey::External(ext.key())
            }
            SequencedConsensusTransactionKind::System(txn) => {
                SequencedConsensusTransactionKey::System(*txn.digest())
            }
        }
    }

    pub fn get_tracking_id(&self) -> u64 {
        match self {
            SequencedConsensusTransactionKind::External(ext) => ext.get_tracking_id(),
            SequencedConsensusTransactionKind::System(_txn) => 0,
        }
    }

    pub fn is_executable_transaction(&self) -> bool {
        match self {
            SequencedConsensusTransactionKind::External(ext) => ext.is_executable_transaction(),
            SequencedConsensusTransactionKind::System(_) => true,
        }
    }

    pub fn executable_transaction_digest(&self) -> Option<TransactionDigest> {
        match self {
            SequencedConsensusTransactionKind::External(ext) => match &ext.kind {
                ConsensusTransactionKind::CertifiedTransaction(txn) => Some(*txn.digest()),
                ConsensusTransactionKind::UserTransaction(txn) => Some(*txn.digest()),
                _ => None,
            },
            SequencedConsensusTransactionKind::System(txn) => Some(*txn.digest()),
        }
    }

    pub fn is_end_of_publish(&self) -> bool {
        match self {
            SequencedConsensusTransactionKind::External(ext) => {
                matches!(ext.kind, ConsensusTransactionKind::EndOfPublish(..))
            }
            SequencedConsensusTransactionKind::System(_) => false,
        }
    }
}

impl SequencedConsensusTransaction {
    pub fn sender_authority(&self) -> AuthorityName {
        self.certificate_author
    }

    pub fn key(&self) -> SequencedConsensusTransactionKey {
        self.transaction.key()
    }

    pub fn is_end_of_publish(&self) -> bool {
        if let SequencedConsensusTransactionKind::External(ref transaction) = self.transaction {
            matches!(transaction.kind, ConsensusTransactionKind::EndOfPublish(..))
        } else {
            false
        }
    }

    pub fn try_take_execution_time_observation(&mut self) -> Option<ExecutionTimeObservation> {
        if let SequencedConsensusTransactionKind::External(ConsensusTransaction {
            kind: ConsensusTransactionKind::ExecutionTimeObservation(observation),
            ..
        }) = &mut self.transaction
        {
            Some(std::mem::take(observation))
        } else {
            None
        }
    }

    pub fn is_system(&self) -> bool {
        matches!(
            self.transaction,
            SequencedConsensusTransactionKind::System(_)
        )
    }

    pub fn is_user_tx_with_randomness(&self, randomness_state_enabled: bool) -> bool {
        if !randomness_state_enabled {
            // If randomness is disabled, these should be processed same as a tx without randomness,
            // which will eventually fail when the randomness state object is not found.
            return false;
        }
        match &self.transaction {
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::CertifiedTransaction(cert),
                ..
            }) => cert.transaction_data().uses_randomness(),
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::UserTransaction(txn),
                ..
            }) => txn.transaction_data().uses_randomness(),
            _ => false,
        }
    }

    pub fn as_consensus_txn(&self) -> Option<&SenderSignedData> {
        match &self.transaction {
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::CertifiedTransaction(certificate),
                ..
            }) if certificate.is_consensus_tx() => Some(certificate.data()),
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::UserTransaction(txn),
                ..
            }) if txn.is_consensus_tx() => Some(txn.data()),
            SequencedConsensusTransactionKind::System(txn) if txn.is_consensus_tx() => {
                Some(txn.data())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedSequencedConsensusTransaction(pub SequencedConsensusTransaction);

#[cfg(test)]
impl VerifiedSequencedConsensusTransaction {
    pub fn new_test(transaction: ConsensusTransaction) -> Self {
        Self(SequencedConsensusTransaction::new_test(transaction))
    }
}

impl SequencedConsensusTransaction {
    pub fn new_test(transaction: ConsensusTransaction) -> Self {
        Self {
            certificate_author_index: 0,
            certificate_author: AuthorityName::ZERO,
            consensus_index: Default::default(),
            transaction: SequencedConsensusTransactionKind::External(transaction),
        }
    }
}

/// Handles certified and rejected transactions output by consensus.
pub(crate) struct ConsensusBlockHandler {
    /// Whether to enable handling certified transactions.
    enabled: bool,
    /// Per-epoch store.
    epoch_store: Arc<AuthorityPerEpochStore>,
    /// Enqueues transactions to the execution scheduler via a separate task.
    execution_scheduler_sender: ExecutionSchedulerSender,
    /// Backpressure subscriber to wait for backpressure to be resolved.
    backpressure_subscriber: BackpressureSubscriber,
    /// Metrics for consensus transaction handling.
    metrics: Arc<AuthorityMetrics>,
}

impl ConsensusBlockHandler {
    pub fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        execution_scheduler_sender: ExecutionSchedulerSender,
        backpressure_subscriber: BackpressureSubscriber,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        Self {
            enabled: epoch_store.protocol_config().mysticeti_fastpath(),
            epoch_store,
            execution_scheduler_sender,
            backpressure_subscriber,
            metrics,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    #[instrument(level = "debug", skip_all)]
    async fn handle_certified_blocks(&self, blocks_output: CertifiedBlocksOutput) {
        self.backpressure_subscriber.await_no_backpressure().await;

        let _scope = monitored_scope("ConsensusBlockHandler::handle_certified_blocks");

        // Avoid triggering fastpath execution or setting transaction status to fastpath certified, during reconfiguration.
        let reconfiguration_lock = self.epoch_store.get_reconfig_state_read_lock_guard();
        if !reconfiguration_lock.should_accept_user_certs() {
            debug!(
                "Skipping fastpath execution because epoch {} is closing user transactions: {}",
                self.epoch_store.epoch(),
                blocks_output
                    .blocks
                    .iter()
                    .map(|b| b.block.reference().to_string())
                    .join(", "),
            );
            return;
        }

        self.metrics.consensus_block_handler_block_processed.inc();
        let epoch = self.epoch_store.epoch();
        let parsed_transactions = blocks_output
            .blocks
            .into_iter()
            .map(|certified_block| {
                let block_ref = certified_block.block.reference();
                let transactions =
                    parse_block_transactions(&certified_block.block, &certified_block.rejected);
                (block_ref, transactions)
            })
            .collect::<Vec<_>>();
        let mut executable_transactions = vec![];
        for (block, transactions) in parsed_transactions.into_iter() {
            for (txn_idx, parsed) in transactions.into_iter().enumerate() {
                let position = ConsensusPosition {
                    epoch,
                    block,
                    index: txn_idx as TransactionIndex,
                };

                let status_str = if parsed.rejected {
                    "rejected"
                } else {
                    "certified"
                };
                if let ConsensusTransactionKind::UserTransaction(tx) = &parsed.transaction.kind {
                    debug!(
                        "User Transaction in position: {:} with digest {:} is {:}",
                        position,
                        tx.digest(),
                        status_str
                    );
                } else {
                    debug!(
                        "System Transaction in position: {:} is {:}",
                        position, status_str
                    );
                }

                if parsed.rejected {
                    // TODO(fastpath): avoid parsing blocks twice between handling commit and fastpath transactions?
                    self.epoch_store
                        .set_consensus_tx_status(position, ConsensusTxStatus::Rejected);
                    self.metrics
                        .consensus_block_handler_txn_processed
                        .with_label_values(&["rejected"])
                        .inc();
                    continue;
                }

                self.metrics
                    .consensus_block_handler_txn_processed
                    .with_label_values(&["certified"])
                    .inc();

                if let ConsensusTransactionKind::UserTransaction(tx) = parsed.transaction.kind {
                    if tx.is_consensus_tx() {
                        continue;
                    }
                    // Only set fastpath certified status on transactions intended for fastpath execution.
                    self.epoch_store
                        .set_consensus_tx_status(position, ConsensusTxStatus::FastpathCertified);
                    let tx = VerifiedTransaction::new_unchecked(*tx);
                    executable_transactions.push(Schedulable::Transaction(
                        VerifiedExecutableTransaction::new_from_consensus(
                            tx,
                            self.epoch_store.epoch(),
                        ),
                    ));
                }
            }
        }

        if executable_transactions.is_empty() {
            return;
        }
        self.metrics
            .consensus_block_handler_fastpath_executions
            .inc_by(executable_transactions.len() as u64);

        self.execution_scheduler_sender.send(
            executable_transactions,
            Default::default(),
            SchedulingSource::MysticetiFastPath,
        );
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct CommitIntervalObserver {
    ring_buffer: VecDeque<u64>,
}

impl CommitIntervalObserver {
    pub fn new(window_size: u32) -> Self {
        Self {
            ring_buffer: VecDeque::with_capacity(window_size as usize),
        }
    }

    pub fn observe_commit_time(&mut self, consensus_commit: &impl ConsensusCommitAPI) {
        let commit_time = consensus_commit.commit_timestamp_ms();
        if self.ring_buffer.len() == self.ring_buffer.capacity() {
            self.ring_buffer.pop_front();
        }
        self.ring_buffer.push_back(commit_time);
    }

    pub fn commit_interval_estimate(&self) -> Option<Duration> {
        if self.ring_buffer.len() <= 1 {
            None
        } else {
            let first = self.ring_buffer.front().unwrap();
            let last = self.ring_buffer.back().unwrap();
            let duration = last.saturating_sub(*first);
            let num_commits = self.ring_buffer.len() as u64;
            Some(Duration::from_millis(duration.div_ceil(num_commits)))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use consensus_core::{
        BlockAPI, CertifiedBlock, CommitDigest, CommitRef, CommittedSubDag, TestBlock, Transaction,
        VerifiedBlock,
    };
    use consensus_types::block::TransactionIndex;
    use futures::pin_mut;
    use prometheus::Registry;
    use sui_protocol_config::{
        Chain, ConsensusTransactionOrdering, PerObjectCongestionControlMode, ProtocolConfig,
        ProtocolVersion,
    };
    use sui_types::{
        base_types::ExecutionDigests,
        base_types::{random_object_ref, AuthorityName, FullObjectRef, ObjectID, SuiAddress},
        committee::Committee,
        crypto::deterministic_random_account_key,
        gas::GasCostSummary,
        message_envelope::Message,
        messages_checkpoint::{
            CheckpointContents, CheckpointSignatureMessage, CheckpointSummary,
            SignedCheckpointSummary,
        },
        messages_consensus::{
            AuthorityCapabilitiesV1, ConsensusTransaction, ConsensusTransactionKind,
        },
        object::Object,
        supported_protocol_versions::SupportedProtocolVersions,
        transaction::{
            CertifiedTransaction, SenderSignedData, TransactionData, TransactionDataAPI,
        },
    };

    use super::*;
    use crate::{
        authority::{
            authority_per_epoch_store::ConsensusStatsAPI,
            test_authority_builder::TestAuthorityBuilder,
        },
        checkpoints::CheckpointServiceNoop,
        consensus_adapter::consensus_tests::{
            make_consensus_adapter_for_test, test_certificates_with_gas_objects,
            test_user_transaction,
        },
        post_consensus_tx_reorder::PostConsensusTxReorder,
    };

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_consensus_commit_handler() {
        telemetry_subscribers::init_for_testing();

        // GIVEN
        // 1 account keypair
        let (sender, keypair) = deterministic_random_account_key();
        // 12 gas objects.
        let gas_objects: Vec<Object> = (0..12)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();
        // 4 owned objects.
        let owned_objects: Vec<Object> = (0..4)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();
        // 6 shared objects.
        let shared_objects: Vec<Object> = (0..6)
            .map(|_| Object::shared_for_testing())
            .collect::<Vec<_>>();
        let mut all_objects = gas_objects.clone();
        all_objects.extend(owned_objects.clone());
        all_objects.extend(shared_objects.clone());

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .with_objects(all_objects.clone())
                .build();

        let mut protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
        protocol_config.set_per_object_congestion_control_mode_for_testing(
            PerObjectCongestionControlMode::None,
        );

        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .with_protocol_config(protocol_config)
            .build()
            .await;

        let epoch_store = state.epoch_store_for_testing().clone();
        let new_epoch_start_state = epoch_store.epoch_start_state();
        let consensus_committee = new_epoch_start_state.get_consensus_committee();

        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));

        let throughput_calculator = ConsensusThroughputCalculator::new(None, metrics.clone());

        let backpressure_manager = BackpressureManager::new_for_tests();
        let consensus_adapter =
            make_consensus_adapter_for_test(state.clone(), HashSet::new(), false, vec![]);
        let mut consensus_handler = ConsensusHandler::new(
            epoch_store,
            Arc::new(CheckpointServiceNoop {}),
            state.execution_scheduler().clone(),
            consensus_adapter,
            state.get_object_cache_reader().clone(),
            Arc::new(ArcSwap::default()),
            consensus_committee.clone(),
            metrics,
            Arc::new(throughput_calculator),
            backpressure_manager.subscribe(),
            state.traffic_controller.clone(),
        );

        // AND create test user transactions alternating between owned and shared input.
        let mut user_transactions = vec![];
        for (i, gas_object) in gas_objects[0..8].iter().enumerate() {
            let input_object = if i % 2 == 0 {
                owned_objects.get(i / 2).unwrap().clone()
            } else {
                shared_objects.get(i / 2).unwrap().clone()
            };
            let transaction = test_user_transaction(
                &state,
                sender,
                &keypair,
                gas_object.clone(),
                vec![input_object],
            )
            .await;
            user_transactions.push(transaction);
        }

        // AND create 4 certified transactions with remaining gas objects and 2 shared objects.
        // Having more txns on the same shared object may get deferred.
        let certified_transactions = [
            test_certificates_with_gas_objects(
                &state,
                &gas_objects[8..10],
                shared_objects[4].clone(),
            )
            .await,
            test_certificates_with_gas_objects(
                &state,
                &gas_objects[10..12],
                shared_objects[5].clone(),
            )
            .await,
        ]
        .concat();

        // AND create block for each user and certified transaction
        let mut blocks = Vec::new();
        for (i, consensus_transaction) in user_transactions
            .iter()
            .map(|t| {
                ConsensusTransaction::new_user_transaction_message(&state.name, t.inner().clone())
            })
            .chain(
                certified_transactions
                    .iter()
                    .map(|t| ConsensusTransaction::new_certificate_message(&state.name, t.clone())),
            )
            .enumerate()
        {
            let transaction_bytes = bcs::to_bytes(&consensus_transaction).unwrap();
            let block = VerifiedBlock::new_for_test(
                TestBlock::new(100 + i as u32, (i % consensus_committee.size()) as u32)
                    .set_transactions(vec![Transaction::new(transaction_bytes)])
                    .build(),
            );

            blocks.push(block);
        }

        // AND create the consensus commit
        let leader_block = blocks[0].clone();
        let committed_sub_dag = CommittedSubDag::new(
            leader_block.reference(),
            blocks.clone(),
            leader_block.timestamp_ms(),
            CommitRef::new(10, CommitDigest::MIN),
        );

        // Test that the consensus handler respects backpressure.
        backpressure_manager.set_backpressure(true);
        // Default watermarks are 0,0 which will suppress the backpressure.
        backpressure_manager.update_highest_certified_checkpoint(1);

        // AND process the consensus commit once
        {
            let waiter = consensus_handler.handle_consensus_commit(committed_sub_dag.clone());
            pin_mut!(waiter);

            // waiter should not complete within 5 seconds
            tokio::time::timeout(std::time::Duration::from_secs(5), &mut waiter)
                .await
                .unwrap_err();

            // lift backpressure
            backpressure_manager.set_backpressure(false);

            // waiter completes now.
            tokio::time::timeout(std::time::Duration::from_secs(100), waiter)
                .await
                .unwrap();
        }

        // THEN check the consensus stats
        let num_blocks = blocks.len();
        let num_transactions = user_transactions.len() + certified_transactions.len();
        let last_consensus_stats_1 = consensus_handler.last_consensus_stats.clone();
        assert_eq!(
            last_consensus_stats_1.index.transaction_index,
            num_transactions as u64
        );
        assert_eq!(last_consensus_stats_1.index.sub_dag_index, 10_u64);
        assert_eq!(last_consensus_stats_1.index.last_committed_round, 100_u64);
        assert_eq!(last_consensus_stats_1.hash, 0);
        assert_eq!(
            last_consensus_stats_1.stats.get_num_messages(0),
            num_blocks as u64
        );
        assert_eq!(
            last_consensus_stats_1.stats.get_num_user_transactions(0),
            num_transactions as u64
        );

        // THEN check for execution status of user transactions.
        for (i, t) in user_transactions.iter().enumerate() {
            let digest = t.digest();
            if let Ok(Ok(_)) = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                state.notify_read_effects("", *digest),
            )
            .await
            {
                // Effects exist as expected.
            } else {
                panic!("User transaction {} {} did not execute", i, digest);
            }
        }

        // THEN check for execution status of certified transactions.
        for (i, t) in certified_transactions.iter().enumerate() {
            let digest = t.digest();
            if let Ok(Ok(_)) = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                state.notify_read_effects("", *digest),
            )
            .await
            {
                // Effects exist as expected.
            } else {
                panic!("Certified transaction {} {} did not execute", i, digest);
            }
        }

        // THEN check for no inflight or suspended transactions.
        state.execution_scheduler().check_empty_for_testing();

        // WHEN processing the same output multiple times
        // THEN the consensus stats do not update
        for _ in 0..2 {
            consensus_handler
                .handle_consensus_commit(committed_sub_dag.clone())
                .await;
            let last_consensus_stats_2 = consensus_handler.last_consensus_stats.clone();
            assert_eq!(last_consensus_stats_1, last_consensus_stats_2);
        }
    }

    #[tokio::test]
    async fn test_consensus_block_handler() {
        // GIVEN
        // 1 account keypair
        let (sender, keypair) = deterministic_random_account_key();
        // 8 gas objects.
        let gas_objects: Vec<Object> = (0..8)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();
        // 4 owned objects.
        let owned_objects: Vec<Object> = (0..4)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();
        // 4 shared objects.
        let shared_objects: Vec<Object> = (0..4)
            .map(|_| Object::shared_for_testing())
            .collect::<Vec<_>>();
        let mut all_objects = gas_objects.clone();
        all_objects.extend(owned_objects.clone());
        all_objects.extend(shared_objects.clone());

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .with_objects(all_objects.clone())
                .build();

        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .build()
            .await;
        let epoch_store = state.epoch_store_for_testing().clone();
        let execution_scheduler_sender = ExecutionSchedulerSender::start(
            state.execution_scheduler().clone(),
            epoch_store.clone(),
        );

        let backpressure_manager = BackpressureManager::new_for_tests();
        let block_handler = ConsensusBlockHandler::new(
            epoch_store.clone(),
            execution_scheduler_sender,
            backpressure_manager.subscribe(),
            state.metrics.clone(),
        );

        // AND create test transactions alternating between owned and shared input.
        let mut transactions = vec![];
        for (i, gas_object) in gas_objects.iter().enumerate() {
            let input_object = if i % 2 == 0 {
                owned_objects.get(i / 2).unwrap().clone()
            } else {
                shared_objects.get(i / 2).unwrap().clone()
            };
            let transaction = test_user_transaction(
                &state,
                sender,
                &keypair,
                gas_object.clone(),
                vec![input_object],
            )
            .await;
            transactions.push(transaction);
        }

        let serialized_transactions: Vec<_> = transactions
            .iter()
            .map(|t| {
                Transaction::new(
                    bcs::to_bytes(&ConsensusTransaction::new_user_transaction_message(
                        &state.name,
                        t.inner().clone(),
                    ))
                    .unwrap(),
                )
            })
            .collect();

        // AND create block for all transactions
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(100, 1)
                .set_transactions(serialized_transactions.clone())
                .build(),
        );

        // AND set rejected transactions.
        let rejected_transactions = vec![0, 3, 4];

        // AND process the transactions from consensus output.
        block_handler
            .handle_certified_blocks(CertifiedBlocksOutput {
                blocks: vec![CertifiedBlock {
                    block: block.clone(),
                    rejected: rejected_transactions.clone(),
                }],
            })
            .await;

        // Ensure the correct consensus status is set for the correct consensus position
        let consensus_tx_status_cache = epoch_store.consensus_tx_status_cache.as_ref().unwrap();
        for txn_idx in 0..transactions.len() {
            let position = ConsensusPosition {
                epoch: epoch_store.epoch(),
                block: block.reference(),
                index: txn_idx as TransactionIndex,
            };
            if rejected_transactions.contains(&(txn_idx as TransactionIndex)) {
                // Expect rejected transactions to be marked as such.
                assert_eq!(
                    consensus_tx_status_cache.get_transaction_status(&position),
                    Some(ConsensusTxStatus::Rejected)
                );
            } else if txn_idx % 2 == 0 {
                // Expect owned object transactions to be marked as fastpath certified.
                assert_eq!(
                    consensus_tx_status_cache.get_transaction_status(&position),
                    Some(ConsensusTxStatus::FastpathCertified),
                );
            } else {
                // Expect shared object transactions to be marked as fastpath certified.
                assert_eq!(
                    consensus_tx_status_cache.get_transaction_status(&position),
                    None,
                );
            }
        }

        // THEN check for status of transactions that should have been executed.
        for (i, t) in transactions.iter().enumerate() {
            // Do not expect shared transactions or rejected transactions to be executed.
            if i % 2 == 1 || rejected_transactions.contains(&(i as TransactionIndex)) {
                continue;
            }
            let digest = t.digest();
            if tokio::time::timeout(
                std::time::Duration::from_secs(10),
                state
                    .get_transaction_cache_reader()
                    .notify_read_fastpath_transaction_outputs(&[*digest]),
            )
            .await
            .is_err()
            {
                panic!("Transaction {} {} did not execute", i, digest);
            }
        }

        // THEN check for no inflight or suspended transactions.
        state.execution_scheduler().check_empty_for_testing();

        // THEN check that rejected transactions are not executed.
        for (i, t) in transactions.iter().enumerate() {
            // Expect shared transactions or rejected transactions to not have executed.
            if i % 2 == 0 && !rejected_transactions.contains(&(i as TransactionIndex)) {
                continue;
            }
            let digest = t.digest();
            assert!(
                !state.is_tx_already_executed(digest),
                "Rejected transaction {} {} should not have been executed",
                i,
                digest
            );
        }
    }

    #[test]
    fn test_order_by_gas_price() {
        let mut v = vec![cap_txn(10), user_txn(42), user_txn(100), cap_txn(1)];
        PostConsensusTxReorder::reorder(&mut v, ConsensusTransactionOrdering::ByGasPrice);
        assert_eq!(
            extract(v),
            vec![
                "cap(10)".to_string(),
                "cap(1)".to_string(),
                "certified(100)".to_string(),
                "certified(42)".to_string(),
            ]
        );

        let mut v = vec![
            user_txn(1200),
            cap_txn(10),
            user_txn(12),
            user_txn(1000),
            user_txn(42),
            user_txn(100),
            cap_txn(1),
            user_txn(1000),
        ];
        PostConsensusTxReorder::reorder(&mut v, ConsensusTransactionOrdering::ByGasPrice);
        assert_eq!(
            extract(v),
            vec![
                "cap(10)".to_string(),
                "cap(1)".to_string(),
                "certified(1200)".to_string(),
                "certified(1000)".to_string(),
                "certified(1000)".to_string(),
                "certified(100)".to_string(),
                "certified(42)".to_string(),
                "certified(12)".to_string(),
            ]
        );

        // If there are no user transactions, the order should be preserved.
        let mut v = vec![
            cap_txn(10),
            eop_txn(12),
            eop_txn(10),
            cap_txn(1),
            eop_txn(11),
        ];
        PostConsensusTxReorder::reorder(&mut v, ConsensusTransactionOrdering::ByGasPrice);
        assert_eq!(
            extract(v),
            vec![
                "cap(10)".to_string(),
                "eop(12)".to_string(),
                "eop(10)".to_string(),
                "cap(1)".to_string(),
                "eop(11)".to_string(),
            ]
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_checkpoint_signature_dedup_v1_vs_v2() {
        telemetry_subscribers::init_for_testing();

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir().build();
        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .build()
            .await;

        let epoch_store = state.epoch_store_for_testing().clone();
        let consensus_committee = epoch_store.epoch_start_state().get_consensus_committee();

        let make_signed = || {
            let epoch = epoch_store.epoch();
            let contents =
                CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::random()]);
            let summary = CheckpointSummary::new(
                &ProtocolConfig::get_for_max_version_UNSAFE(),
                epoch,
                42, // sequence number
                10, // network_total_transactions
                &contents,
                None, // previous_digest
                GasCostSummary::default(),
                None,       // end_of_epoch_data
                0,          // timestamp
                Vec::new(), // randomness_rounds
            );
            SignedCheckpointSummary::new(epoch, summary, &*state.secret, state.name)
        };

        // Prepare V1 pair: same (authority, seq), different digests => same key
        let v1_s1 = make_signed();
        let v1_s2 = make_signed();
        // Validate assumption: digests differ
        assert_ne!(v1_s1.data().digest(), v1_s2.data().digest());
        let v1_a =
            ConsensusTransaction::new_checkpoint_signature_message(CheckpointSignatureMessage {
                summary: v1_s1,
            });
        let v1_b =
            ConsensusTransaction::new_checkpoint_signature_message(CheckpointSignatureMessage {
                summary: v1_s2,
            });

        // Prepare V2 pair: same (authority, seq), different digests => different keys
        let v2_s1 = make_signed();
        let v2_s1_clone = v2_s1.clone();
        let v2_digest_a = v2_s1.data().digest();
        let v2_a =
            ConsensusTransaction::new_checkpoint_signature_message_v2(CheckpointSignatureMessage {
                summary: v2_s1,
            });

        let v2_s2 = make_signed();
        let v2_digest_b = v2_s2.data().digest();
        let v2_b =
            ConsensusTransaction::new_checkpoint_signature_message_v2(CheckpointSignatureMessage {
                summary: v2_s2,
            });

        assert_ne!(v2_digest_a, v2_digest_b);

        // Create an exact duplicate with same digest to exercise valid dedup
        assert_eq!(v2_s1_clone.data().digest(), v2_digest_a);
        let v2_dup =
            ConsensusTransaction::new_checkpoint_signature_message_v2(CheckpointSignatureMessage {
                summary: v2_s1_clone,
            });

        let to_tx = |ct: &ConsensusTransaction| Transaction::new(bcs::to_bytes(ct).unwrap());
        let block = VerifiedBlock::new_for_test(
            TestBlock::new(100, 0)
                .set_transactions(vec![
                    to_tx(&v1_a),
                    to_tx(&v1_b),
                    to_tx(&v2_a),
                    to_tx(&v2_b),
                    to_tx(&v2_dup),
                ])
                .build(),
        );
        let commit = CommittedSubDag::new(
            block.reference(),
            vec![block.clone()],
            block.timestamp_ms(),
            CommitRef::new(10, CommitDigest::MIN),
        );

        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let throughput = ConsensusThroughputCalculator::new(None, metrics.clone());
        let backpressure = BackpressureManager::new_for_tests();
        let consensus_adapter =
            make_consensus_adapter_for_test(state.clone(), HashSet::new(), false, vec![]);
        let mut handler = ConsensusHandler::new(
            epoch_store.clone(),
            Arc::new(CheckpointServiceNoop {}),
            state.execution_scheduler().clone(),
            consensus_adapter,
            state.get_object_cache_reader().clone(),
            Arc::new(ArcSwap::default()),
            consensus_committee.clone(),
            metrics,
            Arc::new(throughput),
            backpressure.subscribe(),
            state.traffic_controller.clone(),
        );

        handler.handle_consensus_commit(commit).await;

        use crate::consensus_handler::SequencedConsensusTransactionKey as SK;
        use sui_types::messages_consensus::ConsensusTransactionKey as CK;

        // V1 collapses digest: both map to the same key and are processed once.
        let v1_key = SK::External(CK::CheckpointSignature(state.name, 42));
        assert!(epoch_store.is_consensus_message_processed(&v1_key).unwrap());

        // V2 distinct digests: both must be processed. If these were collapsed to one CheckpointSeq num, only one would process.
        let v2_key_a = SK::External(CK::CheckpointSignatureV2(state.name, 42, v2_digest_a));
        let v2_key_b = SK::External(CK::CheckpointSignatureV2(state.name, 42, v2_digest_b));
        assert!(epoch_store
            .is_consensus_message_processed(&v2_key_a)
            .unwrap());
        assert!(epoch_store
            .is_consensus_message_processed(&v2_key_b)
            .unwrap());
    }

    fn extract(v: Vec<VerifiedSequencedConsensusTransaction>) -> Vec<String> {
        v.into_iter().map(extract_one).collect()
    }

    fn extract_one(t: VerifiedSequencedConsensusTransaction) -> String {
        match t.0.transaction {
            SequencedConsensusTransactionKind::External(ext) => match ext.kind {
                ConsensusTransactionKind::EndOfPublish(authority) => {
                    format!("eop({})", authority.0[0])
                }
                ConsensusTransactionKind::CapabilityNotification(cap) => {
                    format!("cap({})", cap.generation)
                }
                ConsensusTransactionKind::CertifiedTransaction(txn) => {
                    format!("certified({})", txn.transaction_data().gas_price())
                }
                ConsensusTransactionKind::UserTransaction(txn) => {
                    format!("user({})", txn.transaction_data().gas_price())
                }
                _ => unreachable!(),
            },
            SequencedConsensusTransactionKind::System(_) => unreachable!(),
        }
    }

    fn eop_txn(a: u8) -> VerifiedSequencedConsensusTransaction {
        let mut authority = AuthorityName::default();
        authority.0[0] = a;
        txn(ConsensusTransactionKind::EndOfPublish(authority))
    }

    fn cap_txn(generation: u64) -> VerifiedSequencedConsensusTransaction {
        txn(ConsensusTransactionKind::CapabilityNotification(
            AuthorityCapabilitiesV1 {
                authority: Default::default(),
                generation,
                supported_protocol_versions: SupportedProtocolVersions::SYSTEM_DEFAULT,
                available_system_packages: vec![],
            },
        ))
    }

    fn user_txn(gas_price: u64) -> VerifiedSequencedConsensusTransaction {
        let (committee, keypairs) = Committee::new_simple_test_committee();
        let data = SenderSignedData::new(
            TransactionData::new_transfer(
                SuiAddress::default(),
                FullObjectRef::from_fastpath_ref(random_object_ref()),
                SuiAddress::default(),
                random_object_ref(),
                1000 * gas_price,
                gas_price,
            ),
            vec![],
        );
        txn(ConsensusTransactionKind::CertifiedTransaction(Box::new(
            CertifiedTransaction::new_from_keypairs_for_testing(data, &keypairs, &committee),
        )))
    }

    fn txn(kind: ConsensusTransactionKind) -> VerifiedSequencedConsensusTransaction {
        VerifiedSequencedConsensusTransaction::new_test(ConsensusTransaction {
            kind,
            tracking_id: Default::default(),
        })
    }
}
