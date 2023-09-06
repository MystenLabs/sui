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
use fastcrypto::hash::Hash as _Hash;
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
use sui_protocol_config::ConsensusTransactionOrdering;
use sui_types::base_types::{AuthorityName, EpochId, TransactionDigest};
use sui_types::storage::ParentSync;
use sui_types::transaction::{SenderSignedData, VerifiedTransaction};

use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::{
    ConsensusTransaction, ConsensusTransactionKey, ConsensusTransactionKind,
};
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
        // last_consensus_index is zero at the beginning of epoch, including for hash.
        // It needs to be recovered on restart to ensure consistent consensus hash.
        let last_consensus_index = epoch_store
            .get_last_consensus_index()
            .expect("Should be able to read last consensus index");
        let last_seen = Mutex::new(last_consensus_index);
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
        info!(
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

        // This code no longer supports old protocol versions.
        assert!(self
            .epoch_store
            .protocol_config()
            .consensus_order_end_of_epoch_last());

        let mut sequenced_transactions = Vec::new();
        let mut end_of_publish_transactions = Vec::new();

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

        info!(
            "Received consensus output {:?} at leader round {}, subdag index {}, timestamp {} ",
            consensus_output.digest(),
            round,
            consensus_output.sub_dag.sub_dag_index,
            timestamp,
        );

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
            let mut processed_cache = self.processed_cache.lock();
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

                let sequenced_transaction = SequencedConsensusTransaction {
                    certificate: output_cert.clone(),
                    certificate_author,
                    consensus_index: index_with_hash,
                    transaction,
                };

                let key = sequenced_transaction.key();
                let in_set = !processed_set.insert(key);
                let in_cache = processed_cache
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

        let transactions_to_schedule = self
            .epoch_store
            .process_consensus_transactions_and_commit_boundary(
                &sequenced_transactions,
                &end_of_publish_transactions,
                &self.checkpoint_service,
                &self.parent_sync_store,
            )
            .await
            .expect("Unrecoverable error in consensus handler");

        self.transaction_scheduler
            .schedule(transactions_to_schedule)
            .await;
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
    use shared_crypto::intent::Intent;
    use sui_protocol_config::SupportedProtocolVersions;
    use sui_types::base_types::{random_object_ref, AuthorityName, SuiAddress};
    use sui_types::committee::Committee;
    use sui_types::messages_consensus::{
        AuthorityCapabilities, ConsensusTransaction, ConsensusTransactionKind,
    };
    use sui_types::transaction::{
        CertifiedTransaction, SenderSignedData, TransactionData, TransactionDataAPI,
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
