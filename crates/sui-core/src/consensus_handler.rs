// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    num::NonZeroUsize,
    sync::Arc,
};

use arc_swap::ArcSwap;
use consensus_config::Committee as ConsensusCommittee;
use consensus_core::{CommitConsumerMonitor, TransactionIndex, VerifiedBlock};
use lru::LruCache;
use mysten_common::debug_fatal;
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
    digests::ConsensusCommitDigest,
    executable_transaction::{TrustedExecutableTransaction, VerifiedExecutableTransaction},
    messages_consensus::{
        AuthorityIndex, ConsensusDeterminedVersionAssignments, ConsensusTransaction,
        ConsensusTransactionKey, ConsensusTransactionKind, ExecutionTimeObservation,
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
        epoch_start_configuration::EpochStartConfigTrait,
        AuthorityMetrics, AuthorityState,
    },
    checkpoints::{CheckpointService, CheckpointServiceNotify},
    consensus_throughput_calculator::ConsensusThroughputCalculator,
    consensus_types::consensus_output_api::{parse_block_transactions, ConsensusCommitAPI},
    execution_cache::{ObjectCacheRead, TransactionCacheRead},
    scoring_decision::update_low_scoring_authorities,
    transaction_manager::TransactionManager,
};

pub struct ConsensusHandlerInitializer {
    state: Arc<AuthorityState>,
    checkpoint_service: Arc<CheckpointService>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    throughput_calculator: Arc<ConsensusThroughputCalculator>,
    backpressure_manager: Arc<BackpressureManager>,
}

impl ConsensusHandlerInitializer {
    pub fn new(
        state: Arc<AuthorityState>,
        checkpoint_service: Arc<CheckpointService>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
        throughput_calculator: Arc<ConsensusThroughputCalculator>,
        backpressure_manager: Arc<BackpressureManager>,
    ) -> Self {
        Self {
            state,
            checkpoint_service,
            epoch_store,
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
        let backpressure_manager = BackpressureManager::new_for_tests();
        Self {
            state: state.clone(),
            checkpoint_service,
            epoch_store: state.epoch_store_for_testing().clone(),
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
            self.state.transaction_manager().clone(),
            self.state.get_object_cache_reader().clone(),
            self.state.get_transaction_cache_reader().clone(),
            self.low_scoring_authorities.clone(),
            consensus_committee,
            self.state.metrics.clone(),
            self.throughput_calculator.clone(),
            self.backpressure_manager.subscribe(),
        )
    }

    pub(crate) fn metrics(&self) -> &Arc<AuthorityMetrics> {
        &self.state.metrics
    }

    pub(crate) fn backpressure_subscriber(&self) -> BackpressureSubscriber {
        self.backpressure_manager.subscribe()
    }
}

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
    /// used to read randomness transactions during crash recovery
    tx_reader: Arc<dyn TransactionCacheRead>,
    /// Reputation scores used by consensus adapter that we update, forwarded from consensus
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    /// The consensus committee used to do stake computations for deciding set of low scoring authorities
    committee: ConsensusCommittee,
    // TODO: ConsensusHandler doesn't really share metrics with AuthorityState. We could define
    // a new metrics type here if we want to.
    metrics: Arc<AuthorityMetrics>,
    /// Lru cache to quickly discard transactions processed by consensus
    processed_cache: LruCache<SequencedConsensusTransactionKey, ()>,
    /// Enqueues transactions to the transaction manager via a separate task.
    transaction_manager_sender: TransactionManagerSender,
    /// Using the throughput calculator to record the current consensus throughput
    throughput_calculator: Arc<ConsensusThroughputCalculator>,

    backpressure_subscriber: BackpressureSubscriber,
}

const PROCESSED_CACHE_CAP: usize = 1024 * 1024;

impl<C> ConsensusHandler<C> {
    pub fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint_service: Arc<C>,
        transaction_manager: Arc<TransactionManager>,
        cache_reader: Arc<dyn ObjectCacheRead>,
        tx_reader: Arc<dyn TransactionCacheRead>,
        low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
        committee: ConsensusCommittee,
        metrics: Arc<AuthorityMetrics>,
        throughput_calculator: Arc<ConsensusThroughputCalculator>,
        backpressure_subscriber: BackpressureSubscriber,
    ) -> Self {
        // Recover last_consensus_stats so it is consistent across validators.
        let mut last_consensus_stats = epoch_store
            .get_last_consensus_stats()
            .expect("Should be able to read last consensus index");
        // stats is empty at the beginning of epoch.
        if !last_consensus_stats.stats.is_initialized() {
            last_consensus_stats.stats = ConsensusStats::new(committee.size());
        }
        let transaction_manager_sender =
            TransactionManagerSender::start(transaction_manager, epoch_store.clone());
        Self {
            epoch_store,
            last_consensus_stats,
            checkpoint_service,
            cache_reader,
            tx_reader,
            low_scoring_authorities,
            committee,
            metrics,
            processed_cache: LruCache::new(NonZeroUsize::new(PROCESSED_CACHE_CAP).unwrap()),
            transaction_manager_sender,
            throughput_calculator,
            backpressure_subscriber,
        }
    }

    /// Returns the last subdag index processed by the handler.
    pub(crate) fn last_processed_subdag_index(&self) -> u64 {
        self.last_consensus_stats.index.sub_dag_index
    }

    pub(crate) fn transaction_manager_sender(&self) -> &TransactionManagerSender {
        &self.transaction_manager_sender
    }
}

impl<C: CheckpointServiceNotify + Send + Sync> ConsensusHandler<C> {
    #[instrument(level = "debug", skip_all)]
    async fn handle_consensus_commit(&mut self, consensus_commit: impl ConsensusCommitAPI) {
        // This may block until one of two conditions happens:
        // - Number of uncommitted transactions in the writeback cache goes below the
        //   backpressure threshold.
        // - The highest executed checkpoint catches up to the highest certified checkpoint.
        self.backpressure_subscriber.await_no_backpressure().await;

        let _scope = monitored_scope("ConsensusCommitHandler::handle_consensus_commit");

        let last_committed_round = self.last_consensus_stats.index.last_committed_round;

        let round = consensus_commit.leader_round();

        // TODO: Remove this once narwhal is deprecated. For now mysticeti will not return
        // more than one leader per round so we are not in danger of ignoring any commits.
        assert!(round >= last_committed_round);
        if last_committed_round == round {
            // we can receive the same commit twice after restart
            // It is critical that the writes done by this function are atomic - otherwise we can
            // lose the later parts of a commit if we restart midway through processing it.
            warn!(
                "Ignoring consensus output for round {} as it is already committed. NOTE: This is only expected if consensus is running.",
                round
            );
            return;
        }

        /* (transaction, serialized length) */
        let mut transactions = vec![];
        let timestamp = consensus_commit.commit_timestamp_ms();
        let leader_author = consensus_commit.leader_author_index();
        let commit_sub_dag_index = consensus_commit.commit_sub_dag_index();

        let epoch_start = self
            .epoch_store
            .epoch_start_config()
            .epoch_start_timestamp_ms();
        let timestamp = if timestamp < epoch_start {
            error!(
                "Unexpected commit timestamp {timestamp} less then epoch start time {epoch_start}, author {leader_author}, round {round}",
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
            last_committed_round: round,
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
                self.authenticator_state_update_transaction(round, new_jwks);
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

        {
            let span = trace_span!("ConsensusHandler::HandleCommit::process_consensus_txns");
            let _guard = span.enter();
            for (authority_index, parsed_transactions) in consensus_commit.transactions() {
                // TODO: consider only messages within 1~3 rounds of the leader?
                self.last_consensus_stats
                    .stats
                    .inc_num_messages(authority_index as usize);
                for parsed in parsed_transactions {
                    // Skip executing rejected transactions. Unlocking is the responsibility of the
                    // consensus transaction handler.
                    if parsed.rejected {
                        continue;
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
                            .inc_num_user_transactions(authority_index as usize);
                    }
                    if let ConsensusTransactionKind::RandomnessStateUpdate(randomness_round, _) =
                        &parsed.transaction.kind
                    {
                        // These are deprecated and we should never see them. Log an error and eat the tx if one appears.
                        debug_fatal!(
                            "BUG: saw deprecated RandomnessStateUpdate tx for commit round {round:?}, randomness round {randomness_round:?}"
                        );
                    } else {
                        let transaction =
                            SequencedConsensusTransactionKind::External(parsed.transaction);
                        transactions.push((transaction, authority_index));
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
                    last_committed_round: round,
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

        let executable_transactions = self
            .epoch_store
            .process_consensus_transactions_and_commit_boundary(
                all_transactions,
                &self.last_consensus_stats,
                &self.checkpoint_service,
                self.cache_reader.as_ref(),
                self.tx_reader.as_ref(),
                &ConsensusCommitInfo::new(self.epoch_store.protocol_config(), &consensus_commit),
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

        self.transaction_manager_sender
            .send(executable_transactions);
    }
}

/// Sends transactions to the transaction manager in a separate task,
/// to avoid blocking consensus handler.
#[derive(Clone)]
pub(crate) struct TransactionManagerSender {
    // Using unbounded channel to avoid blocking consensus commit and transaction handler.
    sender: monitored_mpsc::UnboundedSender<Vec<VerifiedExecutableTransaction>>,
}

impl TransactionManagerSender {
    fn start(
        transaction_manager: Arc<TransactionManager>,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Self {
        let (sender, recv) = monitored_mpsc::unbounded_channel("transaction_manager_sender");
        spawn_monitored_task!(Self::run(recv, transaction_manager, epoch_store));
        Self { sender }
    }

    fn send(&self, transactions: Vec<VerifiedExecutableTransaction>) {
        let _ = self.sender.send(transactions);
    }

    async fn run(
        mut recv: monitored_mpsc::UnboundedReceiver<Vec<VerifiedExecutableTransaction>>,
        transaction_manager: Arc<TransactionManager>,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) {
        while let Some(transactions) = recv.recv().await {
            let _guard = monitored_scope("ConsensusHandler::enqueue");
            transaction_manager.enqueue(transactions, &epoch_store);
        }
    }
}

/// Manages the lifetime of tasks handling the commits and transactions output by consensus.
pub(crate) struct MysticetiConsensusHandler {
    tasks: JoinSet<()>,
}

impl MysticetiConsensusHandler {
    pub(crate) fn new(
        mut consensus_handler: ConsensusHandler<CheckpointService>,
        consensus_transaction_handler: ConsensusTransactionHandler,
        mut commit_receiver: UnboundedReceiver<consensus_core::CommittedSubDag>,
        mut transaction_receiver: UnboundedReceiver<Vec<(VerifiedBlock, Vec<TransactionIndex>)>>,
        commit_consumer_monitor: Arc<CommitConsumerMonitor>,
    ) -> Self {
        let mut tasks = JoinSet::new();
        tasks.spawn(monitored_future!(async move {
            // TODO: pause when execution is overloaded, so consensus can detect the backpressure.
            while let Some(consensus_commit) = commit_receiver.recv().await {
                let commit_index = consensus_commit.commit_ref.index;
                consensus_handler
                    .handle_consensus_commit(consensus_commit)
                    .await;
                commit_consumer_monitor.set_highest_handled_commit(commit_index);
            }
        }));
        if consensus_transaction_handler.enabled() {
            tasks.spawn(monitored_future!(async move {
                while let Some(blocks_and_rejected_transactions) = transaction_receiver.recv().await
                {
                    consensus_transaction_handler
                        .handle_consensus_transactions(blocks_and_rejected_transactions)
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
            if certificate.contains_shared_object() {
                "shared_certificate"
            } else {
                "owned_certificate"
            }
        }
        ConsensusTransactionKind::CheckpointSignature(_) => "checkpoint_signature",
        ConsensusTransactionKind::EndOfPublish(_) => "end_of_publish",
        ConsensusTransactionKind::CapabilityNotification(_) => "capability_notification",
        ConsensusTransactionKind::CapabilityNotificationV2(_) => "capability_notification_v2",
        ConsensusTransactionKind::NewJWKFetched(_, _, _) => "new_jwk_fetched",
        ConsensusTransactionKind::RandomnessStateUpdate(_, _) => "randomness_state_update",
        ConsensusTransactionKind::RandomnessDkgMessage(_, _) => "randomness_dkg_message",
        ConsensusTransactionKind::RandomnessDkgConfirmation(_, _) => "randomness_dkg_confirmation",
        ConsensusTransactionKind::UserTransaction(tx) => {
            if tx.contains_shared_object() {
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

    pub fn as_shared_object_txn(&self) -> Option<&SenderSignedData> {
        match &self.transaction {
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::CertifiedTransaction(certificate),
                ..
            }) if certificate.contains_shared_object() => Some(certificate.data()),
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::UserTransaction(txn),
                ..
            }) if txn.contains_shared_object() => Some(txn.data()),
            SequencedConsensusTransactionKind::System(txn) if txn.contains_shared_object() => {
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

/// Represents the information from the current consensus commit.
pub struct ConsensusCommitInfo {
    pub round: u64,
    pub timestamp: u64,
    pub consensus_commit_digest: ConsensusCommitDigest,

    skip_consensus_commit_prologue_in_test: bool,
}

impl ConsensusCommitInfo {
    fn new(protocol_config: &ProtocolConfig, consensus_commit: &impl ConsensusCommitAPI) -> Self {
        Self {
            round: consensus_commit.leader_round(),
            timestamp: consensus_commit.commit_timestamp_ms(),
            consensus_commit_digest: consensus_commit.consensus_digest(protocol_config),

            skip_consensus_commit_prologue_in_test: false,
        }
    }

    pub fn new_for_test(
        commit_round: u64,
        commit_timestamp: u64,
        skip_consensus_commit_prologue_in_test: bool,
    ) -> Self {
        Self {
            round: commit_round,
            timestamp: commit_timestamp,
            consensus_commit_digest: ConsensusCommitDigest::default(),
            skip_consensus_commit_prologue_in_test,
        }
    }

    pub fn skip_consensus_commit_prologue_in_test(&self) -> bool {
        self.skip_consensus_commit_prologue_in_test
    }

    fn consensus_commit_prologue_transaction(&self, epoch: u64) -> VerifiedExecutableTransaction {
        let transaction =
            VerifiedTransaction::new_consensus_commit_prologue(epoch, self.round, self.timestamp);
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

    pub fn create_consensus_commit_prologue_transaction(
        &self,
        epoch: u64,
        protocol_config: &ProtocolConfig,
        cancelled_txn_version_assignment: Vec<(
            TransactionDigest,
            Vec<(ConsensusObjectSequenceKey, SequenceNumber)>,
        )>,
    ) -> VerifiedExecutableTransaction {
        if protocol_config.record_consensus_determined_version_assignments_in_prologue_v2() {
            self.consensus_commit_prologue_v3_transaction(
                epoch,
                ConsensusDeterminedVersionAssignments::CancelledTransactionsV2(
                    cancelled_txn_version_assignment,
                ),
            )
        } else if protocol_config.record_consensus_determined_version_assignments_in_prologue() {
            self.consensus_commit_prologue_v3_transaction(
                epoch,
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
        } else if protocol_config.include_consensus_digest_in_prologue() {
            self.consensus_commit_prologue_v2_transaction(epoch)
        } else {
            self.consensus_commit_prologue_transaction(epoch)
        }
    }
}

/// Handles certified and rejected transactions output by consensus.
pub(crate) struct ConsensusTransactionHandler {
    /// Whether to enable handling certified transactions.
    enabled: bool,
    /// Per-epoch store.
    epoch_store: Arc<AuthorityPerEpochStore>,
    /// Enqueues transactions to the transaction manager via a separate task.
    transaction_manager_sender: TransactionManagerSender,
    /// Backpressure subscriber to wait for backpressure to be resolved.
    backpressure_subscriber: BackpressureSubscriber,
    /// Metrics for consensus transaction handling.
    metrics: Arc<AuthorityMetrics>,
}

impl ConsensusTransactionHandler {
    pub fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        transaction_manager_sender: TransactionManagerSender,
        backpressure_subscriber: BackpressureSubscriber,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        Self {
            enabled: epoch_store.protocol_config().mysticeti_fastpath(),
            epoch_store,
            transaction_manager_sender,
            backpressure_subscriber,
            metrics,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    #[instrument(level = "debug", skip_all)]
    pub async fn handle_consensus_transactions(
        &self,
        blocks_and_rejected_transactions: Vec<(VerifiedBlock, Vec<TransactionIndex>)>,
    ) {
        self.backpressure_subscriber.await_no_backpressure().await;

        let _scope = monitored_scope("ConsensusTransactionHandler::handle_consensus_transactions");

        let parsed_transactions = blocks_and_rejected_transactions
            .into_iter()
            .flat_map(|(block, rejected_transactions)| {
                parse_block_transactions(&block, &rejected_transactions)
            })
            .collect::<Vec<_>>();
        let mut pending_consensus_transactions = vec![];
        let executable_transactions: Vec<_> = parsed_transactions
            .into_iter()
            .filter_map(|parsed| {
                // TODO(fastpath): unlock rejected transactions.
                // TODO(fastpath): maybe avoid parsing blocks twice between commit and transaction handling?
                if parsed.rejected {
                    self.metrics
                        .consensus_transaction_handler_processed
                        .with_label_values(&["rejected"])
                        .inc();
                    return None;
                }
                self.metrics
                    .consensus_transaction_handler_processed
                    .with_label_values(&["certified"])
                    .inc();
                match &parsed.transaction.kind {
                    ConsensusTransactionKind::UserTransaction(tx) => {
                        // TODO(fastpath): use a separate function to check if a transaction should be executed in fastpath.
                        if tx.contains_shared_object() {
                            return None;
                        }
                        pending_consensus_transactions.push(parsed.transaction.clone());
                        let tx = VerifiedTransaction::new_unchecked(*tx.clone());
                        Some(VerifiedExecutableTransaction::new_from_consensus(
                            tx,
                            self.epoch_store.epoch(),
                        ))
                    }
                    _ => None,
                }
            })
            .collect();

        if pending_consensus_transactions.is_empty() {
            return;
        }
        {
            let reconfig_state = self.epoch_store.get_reconfig_state_read_lock_guard();
            // Stop executing fastpath transactions when epoch change starts.
            if !reconfig_state.should_accept_user_certs() {
                return;
            }
            // Otherwise, try to ensure the certified transactions get into consensus before epoch change.
            // TODO(fastpath): avoid race with removals in consensus adapter, by waiting to handle commit after
            // all blocks in the commit are processed via the transaction handler. Other kinds of races need to be
            // avoided as well. Or we can track pending consensus transactions inside consensus instead.
            self.epoch_store
                .insert_pending_consensus_transactions(
                    &pending_consensus_transactions,
                    Some(&reconfig_state),
                )
                .unwrap_or_else(|e| {
                    panic!("Failed to insert pending consensus transactions: {}", e)
                });
        }
        self.metrics
            .consensus_transaction_handler_fastpath_executions
            .inc_by(executable_transactions.len() as u64);
        self.transaction_manager_sender
            .send(executable_transactions);
    }
}

#[cfg(test)]
mod tests {
    use consensus_core::{
        BlockAPI, CommitDigest, CommitRef, CommittedSubDag, TestBlock, Transaction, VerifiedBlock,
    };
    use futures::pin_mut;
    use prometheus::Registry;
    use sui_protocol_config::ConsensusTransactionOrdering;
    use sui_types::{
        base_types::{random_object_ref, AuthorityName, ObjectID, SuiAddress},
        committee::Committee,
        crypto::deterministic_random_account_key,
        messages_consensus::{
            AuthorityCapabilitiesV1, ConsensusTransaction, ConsensusTransactionKind,
            TransactionIndex,
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
            test_certificates_with_gas_objects, test_user_transaction,
        },
        post_consensus_tx_reorder::PostConsensusTxReorder,
    };

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    pub async fn test_consensus_commit_handler() {
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

        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .build()
            .await;

        let epoch_store = state.epoch_store_for_testing().clone();
        let new_epoch_start_state = epoch_store.epoch_start_state();
        let consensus_committee = new_epoch_start_state.get_consensus_committee();

        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));

        let throughput_calculator = ConsensusThroughputCalculator::new(None, metrics.clone());

        let backpressure_manager = BackpressureManager::new_for_tests();
        let mut consensus_handler = ConsensusHandler::new(
            epoch_store,
            Arc::new(CheckpointServiceNoop {}),
            state.transaction_manager().clone(),
            state.get_object_cache_reader().clone(),
            state.get_transaction_cache_reader().clone(),
            Arc::new(ArcSwap::default()),
            consensus_committee.clone(),
            metrics,
            Arc::new(throughput_calculator),
            backpressure_manager.subscribe(),
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
            vec![vec![]; blocks.len()],
            leader_block.timestamp_ms(),
            CommitRef::new(10, CommitDigest::MIN),
            vec![],
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
                state.notify_read_effects(*digest),
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
                state.notify_read_effects(*digest),
            )
            .await
            {
                // Effects exist as expected.
            } else {
                panic!("Certified transaction {} {} did not execute", i, digest);
            }
        }

        // THEN check for no inflight or suspended transactions.
        state.transaction_manager().check_empty_for_testing();

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
    pub async fn test_consensus_transaction_handler() {
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
        let transaction_manager_sender = TransactionManagerSender::start(
            state.transaction_manager().clone(),
            epoch_store.clone(),
        );

        let backpressure_manager = BackpressureManager::new_for_tests();
        let transaction_handler = ConsensusTransactionHandler::new(
            epoch_store,
            transaction_manager_sender,
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
        transaction_handler
            .handle_consensus_transactions(vec![(block.clone(), rejected_transactions.clone())])
            .await;

        // THEN check for status of transactions that should have been executed.
        for (i, t) in transactions.iter().enumerate() {
            // Do not expect shared transactions or rejected transactions to be executed.
            if i % 2 == 1 || rejected_transactions.contains(&(i as TransactionIndex)) {
                continue;
            }
            let digest = t.digest();
            if let Ok(Ok(_)) = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                state.notify_read_effects(*digest),
            )
            .await
            {
                // Effects exist as expected.
            } else {
                panic!("Transaction {} {} did not execute", i, digest);
            }
        }

        // THEN check for no inflight or suspended transactions.
        state.transaction_manager().check_empty_for_testing();

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
                random_object_ref(),
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
