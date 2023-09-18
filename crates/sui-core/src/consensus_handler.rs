// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::{
    AuthorityPerEpochStore, ExecutionIndicesWithHash,
};
use crate::authority::epoch_start_configuration::EpochStartConfigTrait;
use crate::authority::AuthorityMetrics;
use crate::checkpoints::{CheckpointServiceNotify, PendingCheckpoint, PendingCheckpointInfo};
use std::cmp::Ordering;

use crate::scoring_decision::update_low_scoring_authorities;
use crate::transaction_manager::TransactionManager;
use arc_swap::{ArcSwap, ArcSwapOption};
use async_trait::async_trait;
use fastcrypto::hash::Hash as _Hash;
use fastcrypto::traits::ToFromBytes;
use lru::LruCache;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use narwhal_config::Committee;
use narwhal_executor::{ExecutionIndices, ExecutionState};
use narwhal_types::{BatchAPI, CertificateAPI, ConsensusOutput, HeaderAPI, TimestampMs};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use sui_protocol_config::ConsensusTransactionOrdering;
use sui_types::authenticator_state::ActiveJwk;
use sui_types::base_types::{AuthorityName, EpochId, TransactionDigest};
use sui_types::storage::ObjectStore;
use sui_types::transaction::{SenderSignedData, VerifiedTransaction};

use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::{
    ConsensusTransaction, ConsensusTransactionKey, ConsensusTransactionKind,
};
use tracing::{debug, error, info, instrument, warn};

#[derive(Clone, Copy, Debug, PartialEq)]
enum TrafficProfile {
    Low,
    High,
}

impl TrafficProfile {
    fn as_int(&self) -> usize {
        match self {
            TrafficProfile::Low => 0,
            TrafficProfile::High => 1,
        }
    }
}

pub type TimestampSecs = u64;

#[derive(Debug)]
struct TrafficProfileEntry {
    /// The traffic profile
    profile: TrafficProfile,
    /// The time when this traffic profile was created
    timestamp: TimestampSecs,
    /// The calculated throughput when this profile created
    #[allow(unused)]
    throughput: u64,
}

#[derive(Default)]
struct ConsensusThroughputCalculatorInner {
    observations: VecDeque<(TimestampSecs, u64)>,
    total_transactions: u64,
}

struct ConsensusThroughputCalculator {
    observations_window: u64,
    traffic_profile_update_window: TimestampSecs,
    inner: Mutex<ConsensusThroughputCalculatorInner>,
    last_traffic_profile: ArcSwapOption<TrafficProfileEntry>,
    current_throughput: AtomicU64,
    metrics: Arc<AuthorityMetrics>,
}

impl ConsensusThroughputCalculator {
    pub fn new(
        observations_window: NonZeroU64,
        traffic_profile_update_window: TimestampSecs,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        assert!(
            traffic_profile_update_window > 0,
            "traffic_profile_update_window should be >= 0"
        );

        Self {
            observations_window: observations_window.get(),
            traffic_profile_update_window,
            inner: Mutex::new(ConsensusThroughputCalculatorInner::default()),
            last_traffic_profile: ArcSwapOption::empty(), // assume high traffic so the node is more conservative on bootstrap
            current_throughput: AtomicU64::new(0),
            metrics,
        }
    }

    // Adds an observation of the number of transactions that have been sequenced after deduplication
    // and the corresponding leader timestamp. The observation timestamps should be monotonically
    // incremented otherwise observation will be ignored.
    pub fn add_transactions(&self, timestamp_ms: TimestampMs, num_of_transactions: u64) {
        let mut inner = self.inner.lock();
        let timestamp_secs: TimestampSecs = timestamp_ms / 1_000; // lowest bucket we care is seconds

        // If it's the very first observation we just use it as timestamp and don't count any transactions.
        let num_of_transactions = if !inner.observations.is_empty() {
            num_of_transactions
        } else {
            0
        };

        if let Some((front_ts, transactions)) = inner.observations.pop_front() {
            // First check that the timestamp is monotonically incremented - ignore any observation that is not
            // later from previous one (it shouldn't really happen).
            if timestamp_secs < front_ts {
                warn!("Ignoring observation of transactions:{} as has earlier timestamp than last observation {}s < {}s", num_of_transactions, timestamp_secs, front_ts);
                return;
            }

            // Not very likely, but if transactions refer to same second we add to the last element.
            if timestamp_secs == front_ts {
                inner
                    .observations
                    .push_front((front_ts, transactions + num_of_transactions));
            } else {
                inner.observations.push_front((front_ts, transactions));
                inner
                    .observations
                    .push_front((timestamp_secs, num_of_transactions));
            }
        } else {
            inner
                .observations
                .push_front((timestamp_secs, num_of_transactions));
        }

        // update total number of transactions in the observations list
        inner.total_transactions = inner.total_transactions.saturating_add(num_of_transactions);

        // If we have more values on our window of max values, remove the last one, and update the num of transactions
        // We also update the traffic profile when we have at least observations_window values in our observations.
        if inner.observations.len() as u64 > self.observations_window {
            let (last_element_ts, last_element_transactions) =
                inner.observations.pop_back().unwrap();
            inner.total_transactions = inner
                .total_transactions
                .saturating_sub(last_element_transactions);

            //println!("Total transactions: {}", inner.total_transactions);

            // get the first element's timestamp to calculate the transaction rate
            let (first_element_ts, _first_element_transactions) = inner
                .observations
                .front()
                .expect("There should be at least on element in the list");

            let period = first_element_ts.saturating_sub(last_element_ts);

            if period > 0 {
                let current_throughput = inner.total_transactions / period;

                self.update_traffic_profile(current_throughput, timestamp_secs);
            } else {
                warn!("Skip calculating throughput as time period is {}. This is very unlikely to happen, should investigate.", period);
            }
        }
    }

    // Calculate the traffic profile based on
    fn update_traffic_profile(&self, throughput: u64, timestamp: TimestampSecs) {
        let profile = if throughput < 2_000u64 {
            TrafficProfile::Low
        } else {
            TrafficProfile::High
        };

        let should_update_profile = self.last_traffic_profile.load().as_ref().map_or_else(
            || true,
            |entry| {
                // update only when we have a new profile
                profile != entry.profile
                    && timestamp - entry.timestamp >= self.traffic_profile_update_window
            },
        );

        if should_update_profile {
            let p = TrafficProfileEntry {
                profile,
                timestamp,
                throughput,
            };
            info!("Updating traffic profile to {:?}", p);
            self.last_traffic_profile.store(Some(Arc::new(p)));
        }

        // Also update the current throughput
        self.current_throughput.store(throughput, Relaxed);
        self.metrics
            .consensus_calculated_throughput
            .set(throughput as i64);

        self.metrics
            .consensus_calculated_traffic_profile
            .set(self.traffic_profile().0.as_int() as i64);
    }

    // Return the current traffic profile and the corresponding throughput when this was last updated.
    // If that is not set yet then as default the High profile is returned and the throughput will be None.
    pub fn traffic_profile(&self) -> (TrafficProfile, u64) {
        let profile = self.last_traffic_profile.load();
        profile.as_ref().map_or_else(
            || (TrafficProfile::Low, 0),
            |entry| (entry.profile, entry.throughput),
        )
    }

    // Returns the current (live calculated) throughput. If want to get the current throughput use
    // this method. If want to figure out what was the throughput when the traffic profile was last
    // calculated then use the traffic_profile() method.
    #[allow(unused)]
    pub fn current_throughput(&self) -> u64 {
        self.current_throughput.load(Relaxed)
    }
}

pub struct ConsensusHandler<T, C> {
    /// A store created for each epoch. ConsensusHandler is recreated each epoch, with the
    /// corresponding store. This store is also used to get the current epoch ID.
    epoch_store: Arc<AuthorityPerEpochStore>,
    /// Holds the highest transaction index that has been seen so far. It is used for avoiding replaying
    /// already processed transactions and also act as a chain consistency check by calculating and storing a
    /// hash chain.
    last_seen: ExecutionIndicesWithHash,
    checkpoint_service: Arc<C>,
    /// parent_sync_store is needed when determining the next version to assign for shared objects.
    object_store: T,
    /// Reputation scores used by consensus adapter that we update, forwarded from consensus
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    /// The narwhal committee used to do stake computations for deciding set of low scoring authorities
    committee: Committee,
    // TODO: ConsensusHandler doesn't really share metrics with AuthorityState. We could define
    // a new metrics type here if we want to.
    metrics: Arc<AuthorityMetrics>,
    /// Lru cache to quickly discard transactions processed by consensus
    processed_cache: LruCache<SequencedConsensusTransactionKey, ()>,
    transaction_scheduler: AsyncTransactionScheduler,
    /// Using the throughput calculator to identify the traffic profile
    throughput_calculator: ConsensusThroughputCalculator,
}

const PROCESSED_CACHE_CAP: usize = 1024 * 1024;

impl<T, C> ConsensusHandler<T, C> {
    pub fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint_service: Arc<C>,
        transaction_manager: Arc<TransactionManager>,
        object_store: T,
        low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
        committee: Committee,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        // last_seen is zero at the beginning of epoch, including for hash.
        // It needs to be recovered on restart to ensure consistent consensus hash.
        let last_seen = epoch_store
            .get_last_consensus_index()
            .expect("Should be able to read last consensus index");
        let transaction_scheduler =
            AsyncTransactionScheduler::start(transaction_manager, epoch_store.clone());
        let throughput_calculator =
            ConsensusThroughputCalculator::new(NonZeroU64::new(60).unwrap(), 60, metrics.clone());
        Self {
            epoch_store,
            last_seen,
            checkpoint_service,
            object_store,
            low_scoring_authorities,
            committee,
            metrics,
            processed_cache: LruCache::new(NonZeroUsize::new(PROCESSED_CACHE_CAP).unwrap()),
            transaction_scheduler,
            throughput_calculator,
        }
    }

    /// Updates the execution indexes based on the provided input. Some is returned when the indexes
    /// are updated which means that the transaction has been seen for first time. None is returned
    /// otherwise.
    fn update_hash(
        &mut self,
        index: ExecutionIndices,
        v: &[u8],
    ) -> Option<ExecutionIndicesWithHash> {
        if let Some(execution_indexes) = update_hash(&self.last_seen, index, v) {
            self.last_seen = execution_indexes.clone();
            return Some(execution_indexes);
        }
        None
    }
}

fn update_hash(
    last_seen: &ExecutionIndicesWithHash,
    index: ExecutionIndices,
    v: &[u8],
) -> Option<ExecutionIndicesWithHash> {
    if last_seen.index >= index {
        return None;
    }

    let previous_hash = last_seen.hash;
    let mut hasher = DefaultHasher::new();
    previous_hash.hash(&mut hasher);
    v.hash(&mut hasher);
    let hash = hasher.finish();
    // Log hash every 1000th transaction of the subdag
    if index.transaction_index % 1000 == 0 {
        info!(
            "Integrity hash for consensus output at subdag {} transaction {} is {:016x}",
            index.sub_dag_index, index.transaction_index, hash
        );
    }
    Some(ExecutionIndicesWithHash { index, hash })
}

#[async_trait]
impl<T: ObjectStore + Send + Sync, C: CheckpointServiceNotify + Send + Sync> ExecutionState
    for ConsensusHandler<T, C>
{
    /// This function will be called by Narwhal, after Narwhal sequenced this certificate.
    #[instrument(level = "trace", skip_all)]
    async fn handle_consensus_output(&mut self, consensus_output: ConsensusOutput) {
        let _scope = monitored_scope("HandleConsensusOutput");

        // This code no longer supports old protocol versions.
        assert!(self
            .epoch_store
            .protocol_config()
            .consensus_order_end_of_epoch_last());

        let last_committed_round = self
            .epoch_store
            .get_last_consensus_index()
            .expect("Unrecoverable error in consensus handler")
            .index
            .last_committed_round;

        let round = consensus_output.sub_dag.leader_round();

        assert!(round >= last_committed_round);
        if last_committed_round == round {
            // we can receive the same commit twice after restart
            // It is critical that the writes done by this function are atomic - otherwise we can
            // lose the later parts of a commit if we restart midway through processing it.
            info!(
                "Ignoring consensus output for round {} as it is already committed",
                round
            );
            return;
        }

        let mut sequenced_transactions = Vec::new();
        let mut end_of_publish_transactions = Vec::new();

        let mut bytes = 0usize;

        /* (serialized, transaction, output_cert) */
        let mut transactions = vec![];
        let timestamp = consensus_output.sub_dag.commit_timestamp();
        let leader_author = consensus_output.sub_dag.leader.header().author();

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
            "Received consensus output {:?} at leader round {}, subdag index {}, timestamp {} epoch {}",
            consensus_output.digest(),
            round,
            consensus_output.sub_dag.sub_dag_index,
            timestamp,
            self.epoch_store.epoch(),
        );

        let prologue_transaction = self.consensus_commit_prologue_transaction(round, timestamp);
        transactions.push((
            vec![],
            SequencedConsensusTransactionKind::System(prologue_transaction),
            Arc::new(consensus_output.sub_dag.leader.clone()),
        ));

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
            debug!("adding AuthenticatorStateUpdate tx: {:?}", new_jwks);
            let authenticator_state_update_transaction =
                self.authenticator_state_update_transaction(round, new_jwks);

            transactions.push((
                vec![],
                SequencedConsensusTransactionKind::System(authenticator_state_update_transaction),
                Arc::new(consensus_output.sub_dag.leader.clone()),
            ));
        }

        update_low_scoring_authorities(
            self.low_scoring_authorities.clone(),
            &self.committee,
            consensus_output.sub_dag.reputation_score.clone(),
            &self.metrics,
            self.epoch_store
                .protocol_config()
                .consensus_bad_nodes_stake_threshold(),
        );

        self.metrics
            .consensus_committed_subdags
            .with_label_values(&[&leader_author.to_string()])
            .inc();
        for (cert, batches) in consensus_output
            .sub_dag
            .certificates
            .iter()
            .zip(consensus_output.batches.iter())
        {
            assert_eq!(cert.header().payload().len(), batches.len());
            let author = cert.header().author();
            self.metrics
                .consensus_committed_certificates
                .with_label_values(&[&author.to_string()])
                .inc();
            let output_cert = Arc::new(cert.clone());
            for batch in batches {
                assert!(output_cert.header().payload().contains_key(&batch.digest()));
                self.metrics.consensus_handler_processed_batches.inc();
                for serialized_transaction in batch.transactions() {
                    bytes += serialized_transaction.len();

                    let transaction = match bcs::from_bytes::<ConsensusTransaction>(
                        serialized_transaction,
                    ) {
                        Ok(transaction) => transaction,
                        Err(err) => {
                            // This should have been prevented by Narwhal batch verification.
                            panic!(
                                "Unexpected malformed transaction (failed to deserialize): {}\nCertificate={:?} BatchDigest={:?} Transaction={:?}",
                                err, output_cert, batch.digest(), serialized_transaction
                            );
                        }
                    };
                    self.metrics
                        .consensus_handler_processed
                        .with_label_values(&[classify(&transaction)])
                        .inc();
                    let transaction = SequencedConsensusTransactionKind::External(transaction);
                    transactions.push((
                        serialized_transaction.clone(),
                        transaction,
                        output_cert.clone(),
                    ));
                }
            }
        }

        let mut roots = BTreeSet::new();

        {
            // We need a set here as well, since the processed_cache is a LRU cache and can drop
            // entries while we're iterating over the sequenced transactions.
            let mut processed_set = HashSet::new();

            for (seq, (serialized, transaction, output_cert)) in
                transactions.into_iter().enumerate()
            {
                if let Some(digest) = transaction.executable_transaction_digest() {
                    roots.insert(digest);
                }

                let index = ExecutionIndices {
                    last_committed_round: round,
                    sub_dag_index: consensus_output.sub_dag.sub_dag_index,
                    transaction_index: seq as u64,
                };

                let index_with_hash = match self.update_hash(index, &serialized) {
                    Some(i) => i,
                    None => {
                        debug!(
                            "Ignore consensus transaction at index {:?} as it appear to be already processed",
                            index
                        );
                        continue;
                    }
                };

                let certificate_author = AuthorityName::from_bytes(
                    self.committee
                        .authority_safe(&output_cert.header().author())
                        .protocol_key_bytes()
                        .0
                        .as_ref(),
                )
                .unwrap();

                let sequenced_transaction = SequencedConsensusTransaction {
                    certificate: output_cert.clone(),
                    certificate_author,
                    consensus_index: index_with_hash,
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

                let Ok(verified_transaction) = self.epoch_store.verify_consensus_transaction(
                    sequenced_transaction,
                    &self.metrics.skipped_consensus_txns,
                ) else {
                    continue;
                };

                if verified_transaction.0.is_end_of_publish() {
                    end_of_publish_transactions.push(verified_transaction);
                } else {
                    sequenced_transactions.push(verified_transaction);
                }
            }
        }

        // TODO: make the reordering algorithm richer and depend on object hotness as well.
        // Order transactions based on their gas prices. System transactions without gas price
        // are put to the beginning of the sequenced_transactions vector.
        if matches!(
            self.epoch_store
                .protocol_config()
                .consensus_transaction_ordering(),
            ConsensusTransactionOrdering::ByGasPrice
        ) {
            let _scope = monitored_scope("HandleConsensusOutput::order_by_gas_price");
            order_by_gas_price(&mut sequenced_transactions);
        }

        // (!) Should not add new transactions to sequenced_transactions beyond this point

        self.metrics
            .consensus_handler_processed_bytes
            .inc_by(bytes as u64);

        let transactions_to_schedule = {
            let (transactions_to_schedule, consensus_commit_batch) = self
                .epoch_store
                .process_consensus_transactions_and_commit_boundary(
                    &sequenced_transactions,
                    end_of_publish_transactions,
                    &self.checkpoint_service,
                    &self.object_store,
                )
                .await
                .expect("Unrecoverable error in consensus handler");

            // The last block in this function notifies about new checkpoint if needed
            let final_checkpoint_round = consensus_commit_batch.final_checkpoint_round();
            let final_checkpoint = match final_checkpoint_round.map(|r| r.cmp(&round)) {
                Some(Ordering::Less) => {
                    debug!(
                        "Not forming checkpoint for round {} above final checkpoint round {:?}",
                        round, final_checkpoint_round
                    );
                    return;
                }
                Some(Ordering::Equal) => true,
                Some(Ordering::Greater) => false,
                None => false,
            };
            let checkpoint = PendingCheckpoint {
                roots: roots.into_iter().collect(),
                details: PendingCheckpointInfo {
                    timestamp_ms: timestamp,
                    last_of_epoch: final_checkpoint,
                    commit_height: round,
                },
            };

            consensus_commit_batch
                .commit(checkpoint)
                .expect("Failed to commit consensus commit batch");

            if final_checkpoint {
                info!(epoch=?self.epoch(), "Received 2f+1 EndOfPublish messages, notifying last checkpoint");
                self.epoch_store.record_end_of_message_quorum_time_metric();
            }

            transactions_to_schedule
        };

        // update the calculated throughput
        self.throughput_calculator
            .add_transactions(timestamp, transactions_to_schedule.len() as u64);

        self.transaction_scheduler
            .schedule(transactions_to_schedule)
            .await;
    }

    async fn last_executed_sub_dag_index(&self) -> u64 {
        let index_with_hash = self
            .epoch_store
            .get_last_consensus_index()
            .expect("Failed to load consensus indices");

        index_with_hash.index.sub_dag_index
    }
}

fn order_by_gas_price(sequenced_transactions: &mut [VerifiedSequencedConsensusTransaction]) {
    sequenced_transactions.sort_by_key(|txn| {
        // Reverse order, so that transactions with higher gas price are put to the beginning.
        std::cmp::Reverse({
            match &txn.0.transaction {
                SequencedConsensusTransactionKind::External(ConsensusTransaction {
                    tracking_id: _,
                    kind: ConsensusTransactionKind::UserTransaction(cert),
                }) => cert.gas_price(),
                // Non-user transactions are considered to have gas price of MAX u64 and are put to the beginning.
                // This way consensus commit prologue transactions will stay at the beginning.
                _ => u64::MAX,
            }
        })
    });
}

struct AsyncTransactionScheduler {
    sender: tokio::sync::mpsc::Sender<Vec<VerifiedExecutableTransaction>>,
}

impl AsyncTransactionScheduler {
    pub fn start(
        transaction_manager: Arc<TransactionManager>,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> Self {
        let (sender, recv) = tokio::sync::mpsc::channel(16);
        spawn_monitored_task!(Self::run(recv, transaction_manager, epoch_store));
        Self { sender }
    }

    pub async fn schedule(&self, transactions: Vec<VerifiedExecutableTransaction>) {
        self.sender.send(transactions).await.ok();
    }

    pub async fn run(
        mut recv: tokio::sync::mpsc::Receiver<Vec<VerifiedExecutableTransaction>>,
        transaction_manager: Arc<TransactionManager>,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) {
        while let Some(transactions) = recv.recv().await {
            let _guard = monitored_scope("ConsensusHandler::enqueue");
            transaction_manager
                .enqueue(transactions, &epoch_store)
                .expect("transaction_manager::enqueue should not fail");
        }
    }
}

impl<T, C> ConsensusHandler<T, C> {
    fn consensus_commit_prologue_transaction(
        &self,
        round: u64,
        commit_timestamp_ms: u64,
    ) -> VerifiedExecutableTransaction {
        let transaction = VerifiedTransaction::new_consensus_commit_prologue(
            self.epoch(),
            round,
            commit_timestamp_ms,
        );
        VerifiedExecutableTransaction::new_system(transaction, self.epoch())
    }

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
        ConsensusTransactionKind::UserTransaction(certificate) => {
            if certificate.contains_shared_object() {
                "shared_certificate"
            } else {
                "owned_certificate"
            }
        }
        ConsensusTransactionKind::CheckpointSignature(_) => "checkpoint_signature",
        ConsensusTransactionKind::EndOfPublish(_) => "end_of_publish",
        ConsensusTransactionKind::CapabilityNotification(_) => "capability_notification",
        ConsensusTransactionKind::NewJWKFetched(_, _, _) => "new_jwk_fetched",
    }
}

pub struct SequencedConsensusTransaction {
    pub certificate: Arc<narwhal_types::Certificate>,
    pub certificate_author: AuthorityName,
    pub consensus_index: ExecutionIndicesWithHash,
    pub transaction: SequencedConsensusTransactionKind,
}

pub enum SequencedConsensusTransactionKind {
    External(ConsensusTransaction),
    System(VerifiedExecutableTransaction),
}

#[derive(Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Debug)]
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
            SequencedConsensusTransactionKind::External(ext) => ext.is_user_certificate(),
            SequencedConsensusTransactionKind::System(_) => true,
        }
    }

    pub fn executable_transaction_digest(&self) -> Option<TransactionDigest> {
        match self {
            SequencedConsensusTransactionKind::External(ext) => {
                if let ConsensusTransactionKind::UserTransaction(txn) = &ext.kind {
                    Some(*txn.digest())
                } else {
                    None
                }
            }
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

    pub fn as_shared_object_txn(&self) -> Option<&SenderSignedData> {
        match &self.transaction {
            SequencedConsensusTransactionKind::External(ConsensusTransaction {
                kind: ConsensusTransactionKind::UserTransaction(certificate),
                ..
            }) if certificate.contains_shared_object() => Some(certificate.data()),
            SequencedConsensusTransactionKind::System(txn) if txn.contains_shared_object() => {
                Some(txn.data())
            }
            _ => None,
        }
    }
}

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
            transaction: SequencedConsensusTransactionKind::External(transaction),
            certificate: Default::default(),
            certificate_author: AuthorityName::ZERO,
            consensus_index: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use crate::checkpoints::CheckpointServiceNoop;
    use crate::consensus_adapter::consensus_tests::{test_certificates, test_gas_objects};
    use crate::consensus_handler::TrafficProfile::{High, Low};
    use narwhal_config::AuthorityIdentifier;
    use narwhal_test_utils::latest_protocol_version;
    use narwhal_types::{
        Batch, Certificate, CommittedSubDag, Header, HeaderV1Builder, ReputationScores,
    };
    use prometheus::Registry;
    use shared_crypto::intent::Intent;
    use sui_protocol_config::SupportedProtocolVersions;
    use sui_types::base_types::{random_object_ref, AuthorityName, SuiAddress};
    use sui_types::committee::Committee;
    use sui_types::messages_consensus::{
        AuthorityCapabilities, ConsensusTransaction, ConsensusTransactionKind,
    };
    use sui_types::object::Object;
    use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
    use sui_types::transaction::{
        CertifiedTransaction, SenderSignedData, TransactionData, TransactionDataAPI,
    };

    #[tokio::test]
    pub async fn test_consensus_throughput_calculate() {
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let traffic_profile_update_window: TimestampSecs = 5;
        let max_observation_points: NonZeroU64 = NonZeroU64::new(3).unwrap();

        let calculator = ConsensusThroughputCalculator::new(
            max_observation_points,
            traffic_profile_update_window,
            metrics,
        );

        // When no transactions exists, the calculator will return by default "High" to err on the
        // assumption that there is lots of load.
        assert_eq!(calculator.traffic_profile(), (Low, 0));

        calculator.add_transactions(1000 as TimestampMs, 1_000);
        calculator.add_transactions(2000 as TimestampMs, 1_000);
        calculator.add_transactions(3000 as TimestampMs, 1_000);
        calculator.add_transactions(4000 as TimestampMs, 1_000);

        // We expect to have a rate of 1K tx/sec, that's < 2K limit , so traffic profile is set to "low"
        assert_eq!(calculator.traffic_profile(), (Low, 1_000));

        // We add more transactions to go over 2K tx/sec, but time window threshold not satisfied yet,
        // and the profile is not updated yet
        calculator.add_transactions(5_000 as TimestampMs, 2_500);
        calculator.add_transactions(6_000 as TimestampMs, 2_800);
        calculator.add_transactions(7_000 as TimestampMs, 2_500);

        assert_eq!(calculator.traffic_profile(), (Low, 1000));

        // We are adding more transactions to get over 2K tx/sec, so traffic profile should now be categorised
        // as "high"
        calculator.add_transactions(8_000 as TimestampMs, 2_500);
        calculator.add_transactions(9_000 as TimestampMs, 3_000);

        assert_eq!(calculator.traffic_profile(), (High, 2666));
        assert_eq!(calculator.current_throughput(), 2666);

        // Let's now add 0 transactions after 5 seconds. Since 5 seconds have passed since the last
        // update and now the transactions are 0 we expect the traffic to be calculate as:
        // 3000 + 2500 + 0 = 5500 / 15 - 7sec = 5500 / 8sec = 785 tx/sec
        calculator.add_transactions(15_000 as TimestampMs, 0);

        assert_eq!(calculator.traffic_profile(), (Low, 687));
        assert_eq!(calculator.current_throughput(), 687);

        // Adding zero transactions for the next 5 seconds will make throughput zero
        // Traffic profile will remain as Low as it won't get updated.
        calculator.add_transactions(17_000 as TimestampMs, 0);
        assert_eq!(calculator.current_throughput(), 333);

        calculator.add_transactions(19_000 as TimestampMs, 0);
        assert_eq!(calculator.current_throughput(), 0);

        calculator.add_transactions(20_000 as TimestampMs, 0);

        assert_eq!(calculator.traffic_profile(), (Low, 687));
        assert_eq!(calculator.current_throughput(), 0);

        // By adding now a few entries with lots of transactions will trigger a traffic profile update
        // since the last one happened on timestamp 15_000ms.
        calculator.add_transactions(21_000 as TimestampMs, 1_000);
        calculator.add_transactions(22_000 as TimestampMs, 2_000);
        calculator.add_transactions(23_000 as TimestampMs, 3_000);
        assert_eq!(calculator.traffic_profile(), (High, 2000));
        assert_eq!(calculator.current_throughput(), 2000);
    }

    #[tokio::test]
    pub async fn test_consensus_handler() {
        // GIVEN
        let mut objects = test_gas_objects();
        objects.push(Object::shared_for_testing());

        let latest_protocol_config = &latest_protocol_version();

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .with_objects(objects.clone())
                .build();

        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config)
            .build()
            .await;

        let epoch_store = state.epoch_store_for_testing().clone();
        let new_epoch_start_state = epoch_store.epoch_start_state();
        let committee = new_epoch_start_state.get_narwhal_committee();

        let mut consensus_handler = ConsensusHandler::new(
            epoch_store,
            Arc::new(CheckpointServiceNoop {}),
            state.transaction_manager().clone(),
            state.db(),
            Arc::new(ArcSwap::default()),
            committee.clone(),
            Arc::new(AuthorityMetrics::new(&Registry::new())),
        );

        // AND
        // Create a transaction
        let transactions = test_certificates(&state).await;
        let mut certificates = Vec::new();
        let mut batches = Vec::new();

        for transaction in transactions.iter() {
            let transaction_bytes: Vec<u8> = bcs::to_bytes(
                &ConsensusTransaction::new_certificate_message(&state.name, transaction.clone()),
            )
            .unwrap();

            let batch = Batch::new(vec![transaction_bytes], latest_protocol_config);

            batches.push(vec![batch.clone()]);

            // AND make batch as part of a commit
            let header = HeaderV1Builder::default()
                .author(AuthorityIdentifier(0))
                .round(5)
                .epoch(0)
                .parents(BTreeSet::new())
                .with_payload_batch(batch.clone(), 0, 0)
                .build()
                .unwrap();

            let certificate =
                Certificate::new_unsigned(&committee, Header::V1(header), vec![]).unwrap();

            certificates.push(certificate);
        }

        // AND create the consensus output
        let consensus_output = ConsensusOutput {
            sub_dag: Arc::new(CommittedSubDag::new(
                certificates.clone(),
                certificates[0].clone(),
                10,
                ReputationScores::default(),
                None,
            )),
            batches,
        };

        // AND processing the consensus output once
        consensus_handler
            .handle_consensus_output(consensus_output.clone())
            .await;

        // AND capturing the index
        let last_seen_1 = consensus_handler.last_seen.clone();
        assert_eq!(
            last_seen_1.index.transaction_index,
            transactions.len() as u64
        );
        assert_eq!(last_seen_1.index.sub_dag_index, 10_u64);
        assert_eq!(last_seen_1.index.last_committed_round, 5_u64);

        // WHEN processing the same output multiple times
        // THEN the execution indices do not update
        for _ in 0..2 {
            consensus_handler
                .handle_consensus_output(consensus_output.clone())
                .await;
            let last_seen_2 = consensus_handler.last_seen.clone();
            assert_eq!(last_seen_1, last_seen_2);
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

        let tx = &[0];
        assert!(update_hash(&last_seen, index0, tx).is_none());
        assert!(update_hash(&last_seen, index1, tx).is_none());
        assert!(update_hash(&last_seen, index2, tx).is_some());
    }

    #[test]
    fn test_order_by_gas_price() {
        let mut v = vec![cap_txn(10), user_txn(42), user_txn(100), cap_txn(1)];
        order_by_gas_price(&mut v);
        assert_eq!(
            extract(v),
            vec![
                "cap(10)".to_string(),
                "cap(1)".to_string(),
                "user(100)".to_string(),
                "user(42)".to_string(),
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
        order_by_gas_price(&mut v);
        assert_eq!(
            extract(v),
            vec![
                "cap(10)".to_string(),
                "cap(1)".to_string(),
                "user(1200)".to_string(),
                "user(1000)".to_string(),
                "user(1000)".to_string(),
                "user(100)".to_string(),
                "user(42)".to_string(),
                "user(12)".to_string(),
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
        order_by_gas_price(&mut v);
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
            AuthorityCapabilities {
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
            Intent::sui_transaction(),
            vec![],
        );
        txn(ConsensusTransactionKind::UserTransaction(Box::new(
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
