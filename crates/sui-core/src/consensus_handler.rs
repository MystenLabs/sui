// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::{
    AuthorityPerEpochStore, ExecutionIndicesWithHash,
};
use crate::authority::AuthorityMetrics;
use crate::checkpoints::{
    CheckpointService, CheckpointServiceNotify, PendingCheckpoint, PendingCheckpointInfo,
};
use std::cmp::Ordering;

use crate::scoring_decision::update_low_scoring_authorities;
use crate::transaction_manager::TransactionManager;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use fastcrypto::traits::ToFromBytes;
use lru::LruCache;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use narwhal_config::Committee;
use narwhal_executor::{ExecutionIndices, ExecutionState};
use narwhal_types::{BatchAPI, CertificateAPI, ConsensusOutput, HeaderAPI};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Arc;
use sui_types::base_types::{AuthorityName, EpochId, TransactionDigest};
use sui_types::messages::{
    ConsensusTransaction, ConsensusTransactionKey, ConsensusTransactionKind,
    VerifiedExecutableTransaction, VerifiedTransaction,
};
use sui_types::storage::ParentSync;

use tracing::{debug, error, info, instrument};

pub struct ConsensusHandler<T> {
    /// A store created for each epoch. ConsensusHandler is recreated each epoch, with the
    /// corresponding store. This store is also used to get the current epoch ID.
    epoch_store: Arc<AuthorityPerEpochStore>,
    last_seen: Mutex<ExecutionIndicesWithHash>,
    checkpoint_service: Arc<CheckpointService>,
    /// parent_sync_store is needed when determining the next version to assign for shared objects.
    parent_sync_store: T,
    /// Reputation scores used by consensus adapter that we update, forwarded from consensus
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    /// The narwhal committee used to do stake computations for deciding set of low scoring authorities
    committee: Committee,
    /// Mappings used for logging and metrics
    authority_names_to_hostnames: HashMap<AuthorityName, String>,
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
        low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
        authority_names_to_hostnames: HashMap<AuthorityName, String>,
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
            low_scoring_authorities,
            committee,
            authority_names_to_hostnames,
            metrics,
            processed_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(PROCESSED_CACHE_CAP).unwrap(),
            )),
            transaction_scheduler,
        }
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

        let prologue_transaction = self.consensus_commit_prologue_transaction(round, timestamp);
        transactions.push((
            vec![],
            SequencedConsensusTransactionKind::System(prologue_transaction),
            Arc::new(consensus_output.sub_dag.leader.clone()),
        ));

        // TODO: spawn a separate task for this as an optimization
        update_low_scoring_authorities(
            self.low_scoring_authorities.clone(),
            &self.committee,
            consensus_output.sub_dag.reputation_score.clone(),
            self.authority_names_to_hostnames.clone(),
            &self.metrics,
            self.epoch_store.protocol_config(),
        );

        self.metrics
            .consensus_committed_subdags
            .with_label_values(&[&leader_author.to_string()])
            .inc();
        for (cert, batches) in consensus_output.batches {
            let author = cert.header().author();
            self.metrics
                .consensus_committed_certificates
                .with_label_values(&[&author.to_string()])
                .inc();
            let output_cert = Arc::new(cert);
            for batch in batches {
                self.metrics.consensus_handler_processed_batches.inc();
                for serialized_transaction in batch.transactions() {
                    bytes += serialized_transaction.len();

                    let transaction = match bcs::from_bytes::<ConsensusTransaction>(
                        serialized_transaction,
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
                    transactions.push((
                        serialized_transaction.clone(),
                        transaction,
                        output_cert.clone(),
                    ));
                }
            }
        }

        let mut roots = BTreeSet::new();
        for (seq, (serialized, transaction, output_cert)) in transactions.into_iter().enumerate() {
            if let Some(digest) = transaction.executable_transaction_digest() {
                roots.insert(digest);
            }

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

            let certificate_author = AuthorityName::from_bytes(
                self.committee
                    .authority_safe(&output_cert.header().author())
                    .protocol_key_bytes()
                    .0
                    .as_ref(),
            )
            .unwrap();

            sequenced_transactions.push(SequencedConsensusTransaction {
                certificate: output_cert.clone(),
                certificate_author,
                consensus_index: index_with_hash,
                transaction,
            });
        }

        // (!) Should not add new transactions to sequenced_transactions beyond this point

        if self
            .epoch_store
            .protocol_config()
            .consensus_order_end_of_epoch_last()
        {
            reorder_end_of_publish(&mut sequenced_transactions);
        }

        self.metrics
            .consensus_handler_processed_bytes
            .inc_by(bytes as u64);

        let verified_transactions = {
            let mut processed_cache = self.processed_cache.lock();
            // We need a set here as well, since the processed_cache is a LRU cache and can drop
            // entries while we're iterating over the sequenced transactions.
            let mut processed_set = HashSet::new();

            sequenced_transactions
                .into_iter()
                .filter_map(|sequenced_transaction| {
                    let key = sequenced_transaction.key();
                    let in_set = !processed_set.insert(key);
                    let in_cache = processed_cache
                        .put(sequenced_transaction.key(), ())
                        .is_some();

                    if in_set || in_cache {
                        self.metrics.skipped_consensus_txns_cache_hit.inc();
                        return None;
                    }

                    match self.epoch_store.verify_consensus_transaction(
                        sequenced_transaction,
                        &self.metrics.skipped_consensus_txns,
                    ) {
                        Ok(verified_transaction) => Some(verified_transaction),
                        Err(()) => None,
                    }
                })
                .collect()
        };

        let transactions_to_schedule = self
            .epoch_store
            .process_consensus_transactions(
                verified_transactions,
                &self.checkpoint_service,
                &self.parent_sync_store,
            )
            .await
            .expect("Unrecoverable error in consensus handler");

        self.transaction_scheduler
            .schedule(transactions_to_schedule)
            .await;
        if self.epoch_store.in_memory_checkpoint_roots {
            // The last block in this function notifies about new checkpoint if needed
            let final_checkpoint_round = self
                .epoch_store
                .final_epoch_checkpoint()
                .expect("final_epoch_checkpoint failed");
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
            self.checkpoint_service
                .notify_checkpoint(&self.epoch_store, checkpoint)
                .expect("notify_checkpoint has failed");
            if final_checkpoint {
                info!(epoch=?self.epoch(), "Received 2f+1 EndOfPublish messages, notifying last checkpoint");
                self.epoch_store.record_end_of_message_quorum_time_metric();
            }
        } else {
            self.epoch_store
                .handle_commit_boundary(round, timestamp, &self.checkpoint_service)
                .expect("Unrecoverable error in consensus handler when processing commit boundary")
        }
    }

    async fn last_executed_sub_dag_index(&self) -> u64 {
        let index_with_hash = self
            .epoch_store
            .get_last_consensus_index()
            .expect("Failed to load consensus indices");

        index_with_hash.index.sub_dag_index
    }
}

fn reorder_end_of_publish(sequenced_transactions: &mut [SequencedConsensusTransaction]) {
    sequenced_transactions.sort_by_key(SequencedConsensusTransaction::is_end_of_publish);
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
    pub certificate_author: AuthorityName,
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
    use narwhal_types::Certificate;
    use sui_protocol_config::SupportedProtocolVersions;
    use sui_types::base_types::AuthorityName;
    use sui_types::messages::{
        AuthorityCapabilities, ConsensusTransaction, ConsensusTransactionKind,
    };

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
    fn test_reorder_end_of_publish() {
        let mut v = vec![cap_txn(10), cap_txn(1)];
        reorder_end_of_publish(&mut v);
        assert_eq!(
            extract(v),
            vec!["cap(10)".to_string(), "cap(1)".to_string()]
        );
        let mut v = vec![cap_txn(1), cap_txn(10)];
        reorder_end_of_publish(&mut v);
        assert_eq!(
            extract(v),
            vec!["cap(1)".to_string(), "cap(10)".to_string()]
        );
        let mut v = vec![cap_txn(1), eop_txn(1), cap_txn(10), eop_txn(10)];
        reorder_end_of_publish(&mut v);
        assert_eq!(
            extract(v),
            vec![
                "cap(1)".to_string(),
                "cap(10)".to_string(),
                "eop(1)".to_string(),
                "eop(10)".to_string()
            ]
        );
        let mut v = vec![cap_txn(1), eop_txn(10), cap_txn(10), eop_txn(1)];
        reorder_end_of_publish(&mut v);
        assert_eq!(
            extract(v),
            vec![
                "cap(1)".to_string(),
                "cap(10)".to_string(),
                "eop(10)".to_string(),
                "eop(1)".to_string()
            ]
        );
    }

    fn extract(v: Vec<SequencedConsensusTransaction>) -> Vec<String> {
        v.into_iter().map(extract_one).collect()
    }

    fn extract_one(t: SequencedConsensusTransaction) -> String {
        match t.transaction {
            SequencedConsensusTransactionKind::External(ext) => match ext.kind {
                ConsensusTransactionKind::EndOfPublish(authority) => {
                    format!("eop({})", authority.0[0])
                }
                ConsensusTransactionKind::CapabilityNotification(cap) => {
                    format!("cap({})", cap.generation)
                }
                _ => unreachable!(),
            },
            SequencedConsensusTransactionKind::System(_) => unreachable!(),
        }
    }

    fn eop_txn(a: u8) -> SequencedConsensusTransaction {
        let mut authority = AuthorityName::default();
        authority.0[0] = a;
        txn(ConsensusTransactionKind::EndOfPublish(authority))
    }

    fn cap_txn(generation: u64) -> SequencedConsensusTransaction {
        txn(ConsensusTransactionKind::CapabilityNotification(
            AuthorityCapabilities {
                authority: Default::default(),
                generation,
                supported_protocol_versions: SupportedProtocolVersions::SYSTEM_DEFAULT,
                available_system_packages: vec![],
            },
        ))
    }

    fn txn(kind: ConsensusTransactionKind) -> SequencedConsensusTransaction {
        let c = ConsensusTransaction {
            kind,
            tracking_id: Default::default(),
        };
        let transaction = SequencedConsensusTransactionKind::External(c);
        let certificate = Arc::new(Certificate::default());
        let certificate_author = Default::default();
        let consensus_index = Default::default();
        SequencedConsensusTransaction {
            certificate,
            certificate_author,
            consensus_index,
            transaction,
        }
    }
}
