// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::{
    AuthorityPerEpochStore, ExecutionIndicesWithHash,
};
use crate::authority::AuthorityMetrics;
use crate::checkpoints::CheckpointService;
use crate::transaction_manager::TransactionManager;
use async_trait::async_trait;
use dashmap::DashMap;
use lru::LruCache;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use narwhal_executor::{ExecutionIndices, ExecutionState};
use narwhal_types::{ConsensusOutput, ReputationScores};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Arc;
use sui_types::base_types::{AuthorityName, EpochId, TransactionDigest};
use sui_types::committee::Committee;
use sui_types::messages::{
    ConsensusTransaction, ConsensusTransactionKey, ConsensusTransactionKind,
    VerifiedExecutableTransaction, VerifiedTransaction,
};
use sui_types::storage::ParentSync;

use tracing::{debug, error, instrument};

pub struct ConsensusHandler<T> {
    /// A store created for each epoch. ConsensusHandler is recreated each epoch, with the
    /// corresponding store. This store is also used to get the current epoch ID.
    epoch_store: Arc<AuthorityPerEpochStore>,
    last_seen: Mutex<ExecutionIndicesWithHash>,
    checkpoint_service: Arc<CheckpointService>,
    /// parent_sync_store is needed when determining the next version to assign for shared objects.
    parent_sync_store: T,
    /// Reputation scores used by consensus adapter that we update, forwarded from consensus
    low_scoring_authorities: Arc<DashMap<AuthorityName, u64>>,
    /// The committee used to do stake computations for deciding set of low scoring authorities
    committee: Committee,
    // TODO: ConsensusHandler doesn't really share metrics with AuthorityState. We could define
    // a new metrics type here if we want to.
    metrics: Arc<AuthorityMetrics>,
    /// Lru cache to quickly discard transactions processed by consensus
    processed_cache: Mutex<LruCache<SequencedConsensusTransactionKey, ()>>,
    transaction_scheduler: AsyncTransactionScheduler,
}

const PROCESSED_CACHE_CAP: usize = 1024 * 1024;

impl<T> ConsensusHandler<T> {
    pub fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint_service: Arc<CheckpointService>,
        transaction_manager: Arc<TransactionManager>,
        parent_sync_store: T,
        scores_per_authority: Arc<DashMap<AuthorityName, u64>>,
        committee: Committee,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        let last_seen = Mutex::new(Default::default());
        let transaction_scheduler =
            AsyncTransactionScheduler::start(transaction_manager, epoch_store.clone());
        Self {
            epoch_store,
            last_seen,
            checkpoint_service,
            parent_sync_store,
            low_scoring_authorities: scores_per_authority,
            committee,
            metrics,
            processed_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(PROCESSED_CACHE_CAP).unwrap(),
            )),
            transaction_scheduler,
        }
    }
}

/// Updates list of authorities that are deemed to have low reputation scores by consensus
/// these may be lagging behind the network, byzantine, or not reliably participating for any reason.
/// We want to ensure that the remaining set of validators once we exclude the low scoring authorities
/// is including enough stake for a quorum, at the very least. It is also possible that no authorities
/// are particularly low scoring, in which case this will result in storing an empty list.
fn update_low_scoring_authorities(
    low_scoring_authorities: Arc<DashMap<AuthorityName, u64>>,
    committee: Committee,
    reputation_scores: ReputationScores,
) {
    if !reputation_scores.final_of_schedule {
        return;
    }
    let mut score_list = vec![];
    for val in reputation_scores.scores_per_authority.values() {
        score_list.push(*val as f64);
    }

    let median = statistical::median(&score_list);
    let mut deviations = vec![];
    let mut abs_deviations = vec![];
    for (i, _) in score_list.clone().iter().enumerate() {
        deviations.push((score_list[i] - median) * -1.0);
        abs_deviations.push((score_list[i] - median).abs());
    }

    // adjusted median absolute deviation
    let mad = statistical::median(&abs_deviations) / 0.5;
    let mut low_scoring = vec![];
    let mut rest = vec![];
    for (i, (a, _)) in reputation_scores.scores_per_authority.iter().enumerate() {
        let temp = deviations[i] / mad;
        if temp > 2.5 {
            low_scoring.push(a);
        } else {
            rest.push(AuthorityName::from(a));
        }
    }

    low_scoring_authorities.clear();

    // make sure the rest have at least quorum
    let remaining_stake = rest.iter().map(|a| committee.weight(a)).sum::<u64>();
    let quorum_threshold = committee.threshold::<true>();
    if remaining_stake < quorum_threshold {
        return;
    }

    for authority in low_scoring {
        low_scoring_authorities.insert(
            AuthorityName::from(authority),
            *reputation_scores
                .scores_per_authority
                .get(authority)
                .unwrap_or(&0),
        );
    }
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
    // Log hash every 1000th transaction of the subdag
    if index.transaction_index % 1000 == 0 {
        debug!(
            "Integrity hash for consensus output at subdag {} transaction {} is {:016x}",
            index.sub_dag_index, index.transaction_index, hash
        );
    }
    let last_seen = ExecutionIndicesWithHash { index, hash };
    *last_seen_guard = last_seen.clone();
    Some(last_seen)
}

#[async_trait]
impl<T: ParentSync + Send + Sync> ExecutionState for ConsensusHandler<T> {
    /// This function will be called by Narwhal, after Narwhal sequenced this certificate.
    #[instrument(level = "trace", skip_all)]
    async fn handle_consensus_output(&self, consensus_output: ConsensusOutput) {
        let _scope = monitored_scope("HandleConsensusOutput");
        let mut sequenced_transactions = Vec::new();

        let mut bytes = 0usize;
        let round = consensus_output.sub_dag.leader_round();

        /* (serialized, transaction, output_cert) */
        let mut transactions = vec![];
        // Narwhal enforces some invariants on the header.created_at, so we can use it as a timestamp
        let timestamp = consensus_output.sub_dag.leader.header.created_at;

        let prologue_transaction = self.consensus_commit_prologue_transaction(round, timestamp);
        transactions.push((
            vec![],
            SequencedConsensusTransactionKind::System(prologue_transaction),
            Arc::new(consensus_output.sub_dag.leader.clone()),
        ));

        update_low_scoring_authorities(
            self.low_scoring_authorities.clone(),
            self.committee.clone(),
            consensus_output.sub_dag.reputation_score.clone(),
        );

        for (cert, batches) in consensus_output.batches {
            let author = cert.header.author.clone();
            let output_cert = Arc::new(cert);
            for batch in batches {
                self.metrics.consensus_handler_processed_batches.inc();
                for serialized_transaction in batch.transactions {
                    bytes += serialized_transaction.len();

                    let transaction = match bcs::from_bytes::<ConsensusTransaction>(
                        &serialized_transaction,
                    ) {
                        Ok(transaction) => transaction,
                        Err(err) => {
                            // This should be prevented by batch verification, hence `error` log level
                            error!(
                                    "Ignoring unexpected malformed transaction (failed to deserialize) from {}: {}",
                                    author, err
                                );
                            continue;
                        }
                    };
                    self.metrics
                        .consensus_handler_processed
                        .with_label_values(&[classify(&transaction)])
                        .inc();
                    let transaction = SequencedConsensusTransactionKind::External(transaction);
                    transactions.push((serialized_transaction, transaction, output_cert.clone()));
                }
            }
        }

        for (seq, (serialized, transaction, output_cert)) in transactions.into_iter().enumerate() {
            let index = ExecutionIndices {
                last_committed_round: round,
                sub_dag_index: consensus_output.sub_dag.sub_dag_index,
                transaction_index: seq as u64,
            };

            let index_with_hash = match update_hash(&self.last_seen, index, &serialized) {
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
        }

        self.metrics
            .consensus_handler_processed_bytes
            .inc_by(bytes as u64);

        let mut transactions_to_schedule = vec![];
        for sequenced_transaction in sequenced_transactions {
            // todo if we can make handle_consensus_transaction into sync function,
            // we could acquire mutex once for entire loop
            if self
                .processed_cache
                .lock()
                .put(sequenced_transaction.key(), ())
                .is_some()
            {
                self.metrics.skipped_consensus_txns_cache_hit.inc();
                continue;
            }
            let verified_transaction = match self.epoch_store.verify_consensus_transaction(
                sequenced_transaction,
                &self.metrics.skipped_consensus_txns,
            ) {
                Ok(verified_transaction) => verified_transaction,
                Err(()) => continue,
            };

            if let Some(transaction) = self
                .epoch_store
                .process_consensus_transaction(
                    verified_transaction,
                    &self.checkpoint_service,
                    &self.parent_sync_store,
                )
                .await
                .expect("Unrecoverable error in consensus handler")
            {
                transactions_to_schedule.push(transaction);
            }
        }

        self.transaction_scheduler
            .schedule(transactions_to_schedule)
            .await;

        self.epoch_store
            .handle_commit_boundary(round, timestamp, &self.checkpoint_service)
            .expect("Unrecoverable error in consensus handler when processing commit boundary")
    }

    async fn last_executed_sub_dag_index(&self) -> u64 {
        let index_with_hash = self
            .epoch_store
            .get_last_consensus_index()
            .expect("Failed to load consensus indices");

        index_with_hash.index.sub_dag_index
    }
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

impl<T> ConsensusHandler<T> {
    #[allow(dead_code)]
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

    fn epoch(&self) -> EpochId {
        self.epoch_store.epoch()
    }
}

fn classify(transaction: &ConsensusTransaction) -> &'static str {
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
    }
}

pub struct SequencedConsensusTransaction {
    pub certificate: Arc<narwhal_types::Certificate>,
    pub consensus_index: ExecutionIndicesWithHash,
    pub transaction: SequencedConsensusTransactionKind,
}

pub enum SequencedConsensusTransactionKind {
    External(ConsensusTransaction),
    System(VerifiedExecutableTransaction),
}

#[derive(Serialize, Deserialize, Clone, Copy, Hash, PartialEq, Eq, Debug)]
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
}

impl SequencedConsensusTransaction {
    pub fn sender_authority(&self) -> AuthorityName {
        (&self.certificate.header.author).into()
    }

    pub fn key(&self) -> SequencedConsensusTransactionKey {
        self.transaction.key()
    }
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
            transaction: SequencedConsensusTransactionKind::External(transaction),
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
    let tx = &[0];
    assert!(update_hash(&last_seen, index0, tx).is_none());
    assert!(update_hash(&last_seen, index1, tx).is_none());
    assert!(update_hash(&last_seen, index2, tx).is_some());
}

#[test]
pub fn test_update_low_scoring_authorities() {
    #![allow(clippy::mutable_key_type)]
    use fastcrypto::traits::KeyPair;
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use sui_protocol_config::ProtocolVersion;
    use sui_types::crypto::{get_key_pair, AuthorityKeyPair};

    let (_, sec1): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec2): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec3): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec4): (_, AuthorityKeyPair) = get_key_pair();
    let a1: AuthorityName = sec1.public().into();
    let a2: AuthorityName = sec2.public().into();
    let a3: AuthorityName = sec3.public().into();
    let a4: AuthorityName = sec4.public().into();

    let mut authorities = BTreeMap::new();
    authorities.insert(a1, 1);
    authorities.insert(a2, 1);
    authorities.insert(a3, 1);
    authorities.insert(a4, 1);
    let committee = Committee::new(0, ProtocolVersion::MIN, authorities).unwrap();

    let low_scoring = Arc::new(DashMap::new());
    low_scoring.insert(a1, 50);
    let reputation_scores_1 = ReputationScores {
        scores_per_authority: Default::default(),
        final_of_schedule: false,
    };

    // when final of schedule is false, calling update_low_scoring_authorities will not change the
    // low_scoring_authorities map
    update_low_scoring_authorities(
        low_scoring.clone(),
        committee.clone(),
        reputation_scores_1.clone(),
    );
    assert_eq!(*low_scoring.get(&a1).unwrap().value(), 50_u64);
    assert_eq!(low_scoring.len(), 1);

    // there is a clear low outlier in the scores, exclude it
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 45_u64);
    scores.insert(sec2.public().clone(), 49_u64);
    scores.insert(sec3.public().clone(), 55_u64);
    scores.insert(sec4.public().clone(), 25_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(low_scoring.clone(), committee.clone(), reputation_scores);
    assert_eq!(*low_scoring.get(&a4).unwrap().value(), 25_u64);
    assert_eq!(low_scoring.len(), 1);

    // a4 has score of 30 which is a bit lower, but not an outlier, so it should not be excluded
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 45_u64);
    scores.insert(sec2.public().clone(), 49_u64);
    scores.insert(sec3.public().clone(), 55_u64);
    scores.insert(sec4.public().clone(), 30_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(low_scoring.clone(), committee.clone(), reputation_scores);
    assert_eq!(low_scoring.len(), 0);

    // this set of scores has a high performing outlier, we don't exclude it
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 45_u64);
    scores.insert(sec2.public().clone(), 49_u64);
    scores.insert(sec3.public().clone(), 55_u64);
    scores.insert(sec4.public().clone(), 80_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(low_scoring.clone(), committee.clone(), reputation_scores);
    assert_eq!(low_scoring.len(), 0);

    // if more than the quorum is a low outlier, we don't exclude any authority
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 45_u64);
    scores.insert(sec2.public().clone(), 49_u64);
    scores.insert(sec3.public().clone(), 16_u64);
    scores.insert(sec4.public().clone(), 25_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(low_scoring.clone(), committee.clone(), reputation_scores);
    assert_eq!(low_scoring.len(), 0);

    // the computation can handle score values at any scale
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 2300_u64);
    scores.insert(sec2.public().clone(), 3000_u64);
    scores.insert(sec3.public().clone(), 900_u64);
    scores.insert(sec4.public().clone(), 1900_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(low_scoring.clone(), committee.clone(), reputation_scores);
    assert_eq!(low_scoring.len(), 0);

    // the computation can handle score values scaled up or down
    // (note as we scale up sensitivity to outliers goes slightly down)
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 2300_u64);
    scores.insert(sec2.public().clone(), 3000_u64);
    scores.insert(sec3.public().clone(), 210_u64);
    scores.insert(sec4.public().clone(), 1900_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(low_scoring.clone(), committee.clone(), reputation_scores);
    assert_eq!(*low_scoring.get(&a3).unwrap().value(), 210_u64);
    assert_eq!(low_scoring.len(), 1);
}
