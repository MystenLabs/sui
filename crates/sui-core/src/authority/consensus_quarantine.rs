// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::{
    AuthorityEpochTables, EncG, ExecutionIndicesWithStatsV2, LockDetails, PkG,
};
use crate::authority::transaction_deferral::DeferralKey;
use crate::checkpoints::BuilderCheckpointSummary;
use crate::epoch::randomness::SINGLETON_KEY;
use dashmap::DashMap;
use fastcrypto_tbls::{dkg_v1, nodes::PartyId};
use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId};
use moka::policy::EvictionPolicy;
use moka::sync::SegmentedCache as MokaCache;
use mysten_common::ZipDebugEqIteratorExt;
use mysten_common::fatal;
use mysten_common::random_util::randomize_cache_capacity_in_tests;
use parking_lot::Mutex;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque, hash_map};
use sui_types::authenticator_state::ActiveJwk;
use sui_types::base_types::{AuthorityName, ObjectRef, SequenceNumber};
use sui_types::crypto::RandomnessRound;
use sui_types::error::SuiResult;
use sui_types::executable_transaction::{
    TrustedExecutableTransactionWithAliases, VerifiedExecutableTransactionWithAliases,
};
use sui_types::execution::ExecutionTimeObservationKey;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_consensus::AuthorityIndex;
use sui_types::storage::ObjectStore;
use sui_types::transaction::{InputObjectKind, TransactionDataAPI};
use sui_types::{
    base_types::{ConsensusObjectSequenceKey, ObjectID},
    digests::TransactionDigest,
    messages_consensus::{Round, TimestampMs, VersionedDkgConfirmation},
    signature::GenericSignature,
};
use tracing::debug;
use typed_store::Map;
use typed_store::rocks::DBBatch;

use crate::{
    authority::{
        authority_per_epoch_store::AuthorityPerEpochStore,
        shared_object_congestion_tracker::CongestionPerObjectDebt,
    },
    checkpoints::{CheckpointHeight, PendingCheckpoint},
    consensus_handler::SequencedConsensusTransactionKey,
    epoch::{
        randomness::{VersionedProcessedMessage, VersionedUsedProcessedMessages},
        reconfiguration::ReconfigState,
    },
};

use super::*;

#[derive(Default)]
#[allow(clippy::type_complexity)]
pub(crate) struct ConsensusCommitOutput {
    // Consensus and reconfig state
    consensus_round: Round,
    // Keeps all the processed consensus messages. It also includes transactions that have been dropped and not scheduled
    // for execution after failing to acquire the required locks.
    consensus_messages_processed: BTreeSet<SequencedConsensusTransactionKey>,
    end_of_publish: BTreeSet<AuthorityName>,
    reconfig_state: Option<ReconfigState>,
    consensus_commit_stats: Option<ExecutionIndicesWithStatsV2>,

    // transaction scheduling state
    next_shared_object_versions: Option<HashMap<ConsensusObjectSequenceKey, SequenceNumber>>,

    deferred_txns: Vec<(DeferralKey, Vec<VerifiedExecutableTransactionWithAliases>)>,

    // Previously-deferred transactions reloaded by this commit that did not re-defer
    // (scheduled or cancelled - both execute). Their deferred-locks map entries are
    // removed when this commit flushes: the flush gate guarantees they are executed by
    // then, so the objects table takes over conflict coverage with no gap. (Removing at
    // reload would open a window - reload until flush - where the locks are in no
    // in-memory layer and the transaction may not have executed yet.)
    finalized_reloaded_deferred_txns: Vec<TransactionDigest>,
    deleted_deferred_txns: BTreeSet<DeferralKey>,

    // checkpoint state
    pending_checkpoints: Vec<PendingCheckpoint>,

    // random beacon state
    next_randomness_round: Option<(RandomnessRound, TimestampMs)>,

    dkg_confirmations: BTreeMap<PartyId, VersionedDkgConfirmation>,
    dkg_processed_messages: BTreeMap<PartyId, VersionedProcessedMessage>,
    dkg_used_message: Option<VersionedUsedProcessedMessages>,
    dkg_output: Option<Option<dkg_v1::Output<PkG, EncG>>>,

    // jwk state
    pending_jwks: BTreeSet<(AuthorityName, JwkId, JWK)>,
    active_jwks: BTreeSet<(u64, (JwkId, JWK))>,

    // congestion control state
    congestion_control_object_debts: Vec<(ObjectID, u64)>,
    congestion_control_randomness_object_debts: Vec<(ObjectID, u64)>,
    execution_time_observations: Vec<(
        AuthorityIndex,
        u64, /* generation */
        Vec<(ExecutionTimeObservationKey, Duration)>,
    )>,

    // Owned object locks acquired post-consensus.
    owned_object_locks: HashMap<ObjectRef, LockDetails>,

    // True when the checkpoint queue had no pending roots after this commit's flush.
    // Used by quarantine to determine safe commit boundaries on restart.
    checkpoint_queue_drained: bool,
}

impl ConsensusCommitOutput {
    pub fn new(consensus_round: Round) -> Self {
        Self {
            consensus_round,
            ..Default::default()
        }
    }

    pub fn get_deleted_deferred_txn_keys(&self) -> impl Iterator<Item = DeferralKey> + use<'_> {
        self.deleted_deferred_txns.iter().cloned()
    }

    pub fn has_deferred_transactions(&self) -> bool {
        !self.deferred_txns.is_empty()
    }

    fn get_randomness_last_round_timestamp(&self) -> Option<TimestampMs> {
        self.next_randomness_round.as_ref().map(|(_, ts)| *ts)
    }

    fn get_highest_pending_checkpoint_height(&self) -> Option<CheckpointHeight> {
        self.pending_checkpoints.last().map(|cp| cp.height())
    }

    fn get_pending_checkpoints(
        &self,
        last: Option<CheckpointHeight>,
    ) -> impl Iterator<Item = &PendingCheckpoint> {
        self.pending_checkpoints.iter().filter(move |cp| {
            if let Some(last) = last {
                cp.height() > last
            } else {
                true
            }
        })
    }

    fn pending_checkpoint_exists(&self, index: &CheckpointHeight) -> bool {
        self.pending_checkpoints
            .iter()
            .any(|cp| cp.height() == *index)
    }

    fn get_round(&self) -> Option<u64> {
        self.consensus_commit_stats
            .as_ref()
            .map(|stats| stats.index.last_committed_round)
    }

    pub fn insert_end_of_publish(&mut self, authority: AuthorityName) {
        self.end_of_publish.insert(authority);
    }

    pub fn insert_execution_time_observation(
        &mut self,
        source: AuthorityIndex,
        generation: u64,
        estimates: Vec<(ExecutionTimeObservationKey, Duration)>,
    ) {
        self.execution_time_observations
            .push((source, generation, estimates));
    }

    pub(crate) fn record_consensus_commit_stats(&mut self, stats: ExecutionIndicesWithStatsV2) {
        self.consensus_commit_stats = Some(stats);
    }

    // in testing code we often need to write to the db outside of a consensus commit
    pub(crate) fn set_default_commit_stats_for_testing(&mut self) {
        self.record_consensus_commit_stats(Default::default());
    }

    pub fn store_reconfig_state(&mut self, state: ReconfigState) {
        self.reconfig_state = Some(state);
    }

    pub fn record_consensus_message_processed(&mut self, key: SequencedConsensusTransactionKey) {
        self.consensus_messages_processed.insert(key);
    }

    pub fn get_consensus_messages_processed(
        &self,
    ) -> impl Iterator<Item = &SequencedConsensusTransactionKey> {
        self.consensus_messages_processed.iter()
    }

    pub fn set_next_shared_object_versions(
        &mut self,
        next_versions: HashMap<ConsensusObjectSequenceKey, SequenceNumber>,
    ) {
        assert!(self.next_shared_object_versions.is_none());
        self.next_shared_object_versions = Some(next_versions);
    }

    pub fn defer_transactions(
        &mut self,
        key: DeferralKey,
        transactions: Vec<VerifiedExecutableTransactionWithAliases>,
    ) {
        self.deferred_txns.push((key, transactions));
    }

    pub fn delete_loaded_deferred_transactions(&mut self, deferral_keys: &[DeferralKey]) {
        self.deleted_deferred_txns
            .extend(deferral_keys.iter().cloned());
    }

    pub fn set_finalized_reloaded_deferred_txns(&mut self, digests: Vec<TransactionDigest>) {
        assert!(self.finalized_reloaded_deferred_txns.is_empty());
        self.finalized_reloaded_deferred_txns = digests;
    }

    pub fn insert_pending_checkpoint(&mut self, checkpoint: PendingCheckpoint) {
        self.pending_checkpoints.push(checkpoint);
    }

    pub fn reserve_next_randomness_round(
        &mut self,
        next_randomness_round: RandomnessRound,
        commit_timestamp: TimestampMs,
    ) {
        assert!(self.next_randomness_round.is_none());
        self.next_randomness_round = Some((next_randomness_round, commit_timestamp));
    }

    pub fn insert_dkg_confirmation(&mut self, conf: VersionedDkgConfirmation) {
        self.dkg_confirmations.insert(conf.sender(), conf);
    }

    pub fn insert_dkg_processed_message(&mut self, message: VersionedProcessedMessage) {
        self.dkg_processed_messages
            .insert(message.sender(), message);
    }

    pub fn insert_dkg_used_messages(&mut self, used_messages: VersionedUsedProcessedMessages) {
        self.dkg_used_message = Some(used_messages);
    }

    pub fn set_dkg_output(&mut self, output: Option<dkg_v1::Output<PkG, EncG>>) {
        self.dkg_output = Some(output);
    }

    pub fn insert_pending_jwk(&mut self, authority: AuthorityName, id: JwkId, jwk: JWK) {
        self.pending_jwks.insert((authority, id, jwk));
    }

    pub fn insert_active_jwk(&mut self, round: u64, key: (JwkId, JWK)) {
        self.active_jwks.insert((round, key));
    }

    pub fn set_congestion_control_object_debts(&mut self, object_debts: Vec<(ObjectID, u64)>) {
        self.congestion_control_object_debts = object_debts;
    }

    pub fn set_congestion_control_randomness_object_debts(
        &mut self,
        object_debts: Vec<(ObjectID, u64)>,
    ) {
        self.congestion_control_randomness_object_debts = object_debts;
    }

    pub fn set_checkpoint_queue_drained(&mut self, drained: bool) {
        self.checkpoint_queue_drained = drained;
    }

    pub fn set_owned_object_locks(&mut self, locks: HashMap<ObjectRef, LockDetails>) {
        assert!(self.owned_object_locks.is_empty());
        self.owned_object_locks = locks;
    }

    pub fn write_to_batch(
        self,
        epoch_store: &AuthorityPerEpochStore,
        batch: &mut DBBatch,
    ) -> SuiResult {
        let tables = epoch_store.tables()?;
        batch.insert_batch(
            &tables.consensus_message_processed,
            self.consensus_messages_processed
                .iter()
                .map(|key| (key, true)),
        )?;

        batch.insert_batch(
            &tables.end_of_publish,
            self.end_of_publish.iter().map(|authority| (authority, ())),
        )?;

        if let Some(reconfig_state) = &self.reconfig_state {
            batch.insert_batch(
                &tables.reconfig_state,
                [(RECONFIG_STATE_INDEX, reconfig_state)],
            )?;
        }

        let consensus_commit_stats = self
            .consensus_commit_stats
            .expect("consensus_commit_stats must be set");
        let round = consensus_commit_stats.index.last_committed_round;

        batch.insert_batch(
            &tables.last_consensus_stats_v2,
            [(LAST_CONSENSUS_STATS_ADDR, consensus_commit_stats)],
        )?;

        if let Some(next_versions) = self.next_shared_object_versions {
            batch.insert_batch(&tables.next_shared_object_versions_v2, next_versions)?;
        }

        batch.delete_batch(
            &tables.deferred_transactions_with_aliases_v3,
            &self.deleted_deferred_txns,
        )?;

        batch.insert_batch(
            &tables.deferred_transactions_with_aliases_v3,
            self.deferred_txns.into_iter().map(|(key, txs)| {
                (
                    key,
                    txs.into_iter()
                        .map(|tx| {
                            let tx: TrustedExecutableTransactionWithAliases = tx.serializable();
                            tx
                        })
                        .collect::<Vec<_>>(),
                )
            }),
        )?;

        if let Some((round, commit_timestamp)) = self.next_randomness_round {
            batch.insert_batch(&tables.randomness_next_round, [(SINGLETON_KEY, round)])?;
            batch.insert_batch(
                &tables.randomness_last_round_timestamp,
                [(SINGLETON_KEY, commit_timestamp)],
            )?;
        }

        batch.insert_batch(&tables.dkg_confirmations_v2, self.dkg_confirmations)?;
        batch.insert_batch(
            &tables.dkg_processed_messages_v2,
            self.dkg_processed_messages,
        )?;
        batch.insert_batch(
            &tables.dkg_used_messages_v2,
            // using Option as iter
            self.dkg_used_message
                .into_iter()
                .map(|used_msgs| (SINGLETON_KEY, used_msgs)),
        )?;
        if let Some(output) = self.dkg_output {
            batch.insert_batch(&tables.dkg_output_v2, [(SINGLETON_KEY, output)])?;
        }

        batch.insert_batch(
            &tables.pending_jwks,
            self.pending_jwks.into_iter().map(|j| (j, ())),
        )?;
        batch.insert_batch(
            &tables.active_jwks,
            self.active_jwks.into_iter().map(|j| {
                // TODO: we don't need to store the round in this map if it is invariant
                assert_eq!(j.0, round);
                (j, ())
            }),
        )?;

        batch.insert_batch(
            &tables.congestion_control_object_debts,
            self.congestion_control_object_debts
                .into_iter()
                .map(|(object_id, debt)| {
                    (
                        object_id,
                        CongestionPerObjectDebt::new(self.consensus_round, debt),
                    )
                }),
        )?;
        batch.insert_batch(
            &tables.congestion_control_randomness_object_debts,
            self.congestion_control_randomness_object_debts
                .into_iter()
                .map(|(object_id, debt)| {
                    (
                        object_id,
                        CongestionPerObjectDebt::new(self.consensus_round, debt),
                    )
                }),
        )?;

        batch.insert_batch(
            &tables.execution_time_observations,
            self.execution_time_observations
                .into_iter()
                .map(|(authority, generation, estimates)| ((generation, authority), estimates)),
        )?;

        Ok(())
    }
}

/// Owned-object lock refs held by currently-deferred transactions.
///
/// Deferred transactions are the one class of finalized transactions whose locks the
/// objects table cannot reproduce: they hold locks from their first appearance but have
/// not executed, so their inputs are still at the claimed versions. This map keeps those
/// locks in memory so post-consensus conflict detection does not need a durable lock
/// table for them. Entries are inserted by the consensus handler on deferral and removed
/// when the commit that reloaded the transaction for scheduling *flushes* (by which
/// point the transaction is executed and the objects table takes over coverage) —
/// mirroring the lifetime of the durable deferred-transactions table entry, whose
/// deletion is part of that same flush.
#[derive(Default)]
pub(crate) struct DeferredTransactionLocks {
    by_ref: HashMap<ObjectRef, TransactionDigest>,
    by_tx: HashMap<TransactionDigest, Vec<ObjectRef>>,
}

impl DeferredTransactionLocks {
    pub fn insert(&mut self, digest: TransactionDigest, refs: Vec<ObjectRef>) {
        for obj_ref in &refs {
            self.by_ref.insert(*obj_ref, digest);
        }
        self.by_tx.insert(digest, refs);
    }

    /// Removes and returns the lock refs held by `digest`. The caller decides whether
    /// they re-enter this map (re-deferral) or the current commit's locks (scheduling).
    pub fn remove_tx(&mut self, digest: &TransactionDigest) -> Option<Vec<ObjectRef>> {
        let refs = self.by_tx.remove(digest)?;
        for obj_ref in &refs {
            self.by_ref.remove(obj_ref);
        }
        Some(refs)
    }

    pub fn get(&self, obj_ref: &ObjectRef) -> Option<TransactionDigest> {
        self.by_ref.get(obj_ref).copied()
    }

    pub fn contains_tx(&self, digest: &TransactionDigest) -> bool {
        self.by_tx.contains_key(digest)
    }
}

/// ConsensusOutputCache holds outputs of consensus processing that do not need to be committed to disk.
/// Data quarantining guarantees that all of this data will be used (e.g. for building checkpoints)
/// before the consensus commit from which it originated is marked as processed. Therefore we can rely
/// on replay of consensus commits to recover this data.
pub(crate) struct ConsensusOutputCache {
    // deferred transactions is only used by consensus handler so there should never be lock contention
    // - hence no need for a DashMap.
    pub(crate) deferred_transactions:
        Mutex<BTreeMap<DeferralKey, Vec<VerifiedExecutableTransactionWithAliases>>>,

    // Read by the consensus handler and the transaction submission path; written only by
    // the consensus handler.
    pub(crate) deferred_transaction_locks: Mutex<DeferredTransactionLocks>,

    // user_signatures_for_checkpoints is written to by consensus handler and read from by checkpoint builder
    // The critical sections are small in both cases so a DashMap is probably not helpful.
    #[allow(clippy::type_complexity)]
    pub(crate) user_signatures_for_checkpoints:
        Mutex<HashMap<TransactionDigest, Vec<(GenericSignature, Option<SequenceNumber>)>>>,

    executed_in_epoch: RwLock<DashMap<TransactionDigest, ()>>,
    executed_in_epoch_cache: MokaCache<TransactionDigest, ()>,
}

impl ConsensusOutputCache {
    pub(crate) fn new(tables: &AuthorityEpochTables, object_store: &dyn ObjectStore) -> Self {
        let deferred_transactions = tables
            .get_all_deferred_transactions()
            .expect("load deferred transactions cannot fail");

        // Rebuild the in-memory locks of deferred transactions. The stored transactions do
        // not carry their immutable-object claims, so the lock set is re-derived from live
        // objects: a deferred transaction's owned inputs are still live at their claimed
        // versions (it holds their locks and has not executed - and its commit was flushed,
        // so all producing transactions have been executed locally), which makes
        // immutability decidable per ref. A byzantine under-claim can make this a subset of
        // the originally-acquired set for immutable refs only - quorum-unreachable once
        // strict vote-time claims verification is universal.
        let mut deferred_transaction_locks = DeferredTransactionLocks::default();
        for transactions in deferred_transactions.values() {
            for tx in transactions {
                let digest = *tx.tx().digest();
                let refs = derive_deferred_owned_lock_refs(object_store, tx);
                deferred_transaction_locks.insert(digest, refs);
            }
        }

        let executed_in_epoch_cache_capacity = 50_000;

        Self {
            deferred_transactions: Mutex::new(deferred_transactions),
            deferred_transaction_locks: Mutex::new(deferred_transaction_locks),
            user_signatures_for_checkpoints: Default::default(),
            executed_in_epoch: RwLock::new(DashMap::with_shard_amount(2048)),
            executed_in_epoch_cache: MokaCache::builder(8)
                // most queries should be for recent transactions
                .max_capacity(randomize_cache_capacity_in_tests(
                    executed_in_epoch_cache_capacity,
                ))
                .eviction_policy(EvictionPolicy::lru())
                .build(),
        }
    }

    pub fn get_deferred_transaction_lock(&self, obj_ref: &ObjectRef) -> Option<TransactionDigest> {
        self.deferred_transaction_locks.lock().get(obj_ref)
    }

    pub fn executed_in_current_epoch(&self, digest: &TransactionDigest) -> bool {
        self.executed_in_epoch
            .read()
            .contains_key(digest) ||
            // we use get instead of contains key to mark the entry as read
            self.executed_in_epoch_cache.get(digest).is_some()
    }

    // Called by execution
    pub fn insert_executed_in_epoch(&self, tx_digest: TransactionDigest) {
        assert!(
            self.executed_in_epoch
                .read()
                .insert(tx_digest, ())
                .is_none(),
            "transaction already executed"
        );
        self.executed_in_epoch_cache.insert(tx_digest, ());
    }

    // CheckpointExecutor calls this (indirectly) in order to prune the in-memory cache of executed
    // transactions. By the time this is called, the transaction digests will have been committed to
    // the `executed_transactions_to_checkpoint` table.
    pub fn remove_executed_in_epoch(&self, tx_digests: &[TransactionDigest]) {
        let executed_in_epoch = self.executed_in_epoch.read();
        for tx_digest in tx_digests {
            executed_in_epoch.remove(tx_digest);
        }
    }
}

/// The owned-object lock refs a deferred transaction holds, re-derived from live objects:
/// every non-immutable `ImmOrOwnedMoveObject` input (immutable inputs were claimed at vote
/// time and excluded from lock acquisition). Any ref in an unexpected state (missing, or
/// not live at the claimed version - which the held lock should preclude) is included
/// conservatively: the durable lock table holds every originally-acquired ref, so
/// over-inclusion here can only reproduce a lock that really exists.
fn derive_deferred_owned_lock_refs(
    object_store: &dyn ObjectStore,
    tx: &VerifiedExecutableTransactionWithAliases,
) -> Vec<ObjectRef> {
    let Ok(input_objects) = tx.tx().transaction_data().input_objects() else {
        // Finalized transactions have valid input objects; stay conservative if not.
        return Vec::new();
    };
    input_objects
        .iter()
        .filter_map(|kind| match kind {
            InputObjectKind::ImmOrOwnedMoveObject(obj_ref) => {
                match object_store.get_object(&obj_ref.0) {
                    Some(object) if object.version() == obj_ref.1 && object.is_immutable() => None,
                    _ => Some(*obj_ref),
                }
            }
            _ => None,
        })
        .collect()
}

/// ConsensusOutputQuarantine holds outputs of consensus processing in memory until the checkpoints
/// for the commit have been certified.
pub(crate) struct ConsensusOutputQuarantine {
    // Output from consensus handler
    output_queue: VecDeque<ConsensusCommitOutput>,

    // Highest known certified checkpoint sequence number
    highest_executed_checkpoint: CheckpointSequenceNumber,

    // Checkpoint Builder output
    builder_checkpoint_summary: BTreeMap<CheckpointSequenceNumber, BuilderCheckpointSummary>,

    // Any un-committed next versions are stored here.
    shared_object_next_versions: RefCountedHashMap<ConsensusObjectSequenceKey, SequenceNumber>,

    // The most recent congestion control debts for objects. Uses a ref-count to track
    // which objects still exist in some element of output_queue.
    congestion_control_randomness_object_debts:
        RefCountedHashMap<ObjectID, CongestionPerObjectDebt>,
    congestion_control_object_debts: RefCountedHashMap<ObjectID, CongestionPerObjectDebt>,

    processed_consensus_messages: RefCountedHashMap<SequencedConsensusTransactionKey, ()>,

    // Owned object locks acquired post-consensus.
    owned_object_locks: HashMap<ObjectRef, LockDetails>,

    metrics: Arc<EpochMetrics>,
}

impl ConsensusOutputQuarantine {
    pub(super) fn new(
        highest_executed_checkpoint: CheckpointSequenceNumber,
        authority_metrics: Arc<EpochMetrics>,
    ) -> Self {
        Self {
            highest_executed_checkpoint,

            output_queue: VecDeque::new(),
            builder_checkpoint_summary: BTreeMap::new(),
            shared_object_next_versions: RefCountedHashMap::new(),
            processed_consensus_messages: RefCountedHashMap::new(),
            congestion_control_randomness_object_debts: RefCountedHashMap::new(),
            congestion_control_object_debts: RefCountedHashMap::new(),
            owned_object_locks: HashMap::new(),
            metrics: authority_metrics,
        }
    }
}

// Write methods - all methods in this block insert new data into the quarantine.
// There are only two sources! ConsensusHandler and CheckpointBuilder.
impl ConsensusOutputQuarantine {
    // Push all data gathered from a consensus commit into the quarantine.
    pub(crate) fn push_consensus_output(
        &mut self,
        output: ConsensusCommitOutput,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult {
        self.insert_shared_object_next_versions(&output);
        self.insert_congestion_control_debts(&output);
        self.insert_processed_consensus_messages(&output);
        self.insert_owned_object_locks(&output);
        self.output_queue.push_back(output);

        self.metrics
            .consensus_quarantine_queue_size
            .set(self.output_queue.len() as i64);

        // we may already have observed the certified checkpoint for this round, if state sync is running
        // ahead of consensus, so there may be data to commit right away.
        self.commit(epoch_store)
    }

    // Record a newly built checkpoint.
    pub(super) fn insert_builder_summary(
        &mut self,
        sequence_number: CheckpointSequenceNumber,
        summary: BuilderCheckpointSummary,
    ) {
        debug!(?sequence_number, "inserting builder summary {:?}", summary);
        self.builder_checkpoint_summary
            .insert(sequence_number, summary);
    }
}

// Commit methods.
impl ConsensusOutputQuarantine {
    /// Update the highest executed checkpoint and commit any data which is now
    /// below the watermark.
    pub(super) fn update_highest_executed_checkpoint(
        &mut self,
        checkpoint: CheckpointSequenceNumber,
        epoch_store: &AuthorityPerEpochStore,
        batch: &mut DBBatch,
    ) -> SuiResult {
        self.highest_executed_checkpoint = checkpoint;
        self.commit_with_batch(epoch_store, batch)
    }

    pub(super) fn commit(&mut self, epoch_store: &AuthorityPerEpochStore) -> SuiResult {
        let mut batch = epoch_store.db_batch()?;
        self.commit_with_batch(epoch_store, &mut batch)?;
        batch.write()?;
        Ok(())
    }

    /// Commit all data below the watermark.
    fn commit_with_batch(
        &mut self,
        epoch_store: &AuthorityPerEpochStore,
        batch: &mut DBBatch,
    ) -> SuiResult {
        // The commit algorithm is simple:
        // 1. First commit all checkpoint builder state which is below the watermark.
        // 2. Determine the consensus commit height that corresponds to the highest committed
        //    checkpoint.
        // 3. Commit all consensus output at that height or below.

        let tables = epoch_store.tables()?;

        let mut highest_committed_height = None;

        while self
            .builder_checkpoint_summary
            .first_key_value()
            .map(|(seq, _)| *seq <= self.highest_executed_checkpoint)
            == Some(true)
        {
            let (seq, builder_summary) = self.builder_checkpoint_summary.pop_first().unwrap();

            batch.insert_batch(
                &tables.builder_checkpoint_summary_v2,
                [(seq, &builder_summary)],
            )?;

            let checkpoint_height = builder_summary
                .checkpoint_height
                .expect("non-genesis checkpoint must have height");
            if let Some(highest) = highest_committed_height {
                assert!(
                    checkpoint_height >= highest,
                    "current checkpoint height {} must be no less than highest committed height {}",
                    checkpoint_height,
                    highest
                );
            }

            highest_committed_height = Some(checkpoint_height);
        }

        let Some(highest_committed_height) = highest_committed_height else {
            return Ok(());
        };

        // Only commit outputs up to the last one where the checkpoint queue
        // was fully drained (no pending roots). If the queue is empty after an
        // output, there are no roots that could be lost on restart. Any outputs
        // after the last drain point stay in the quarantine and get full-replayed
        // on restart with correct root reconstruction.
        let mut last_drain_idx = None;
        for (i, output) in self.output_queue.iter().enumerate() {
            let stats = output
                .consensus_commit_stats
                .as_ref()
                .expect("consensus_commit_stats must be set");
            if stats.height > highest_committed_height {
                break;
            }
            if output.checkpoint_queue_drained {
                last_drain_idx = Some(i);
            }
        }
        if let Some(idx) = last_drain_idx {
            for _ in 0..=idx {
                let output = self.output_queue.pop_front().unwrap();
                self.remove_shared_object_next_versions(&output);
                self.remove_processed_consensus_messages(&output);
                self.remove_congestion_control_debts(&output);
                self.remove_owned_object_locks(&output);
                // Reloaded deferred transactions covered by this commit are executed by
                // now (the flush gate requires their checkpoints executed), so the
                // objects table covers their consumed inputs from here on.
                if !output.finalized_reloaded_deferred_txns.is_empty() {
                    let mut deferred_locks = epoch_store
                        .consensus_output_cache
                        .deferred_transaction_locks
                        .lock();
                    for digest in &output.finalized_reloaded_deferred_txns {
                        deferred_locks.remove_tx(digest);
                    }
                }
                output.write_to_batch(epoch_store, batch)?;
            }
        }

        self.metrics
            .consensus_quarantine_queue_size
            .set(self.output_queue.len() as i64);

        Ok(())
    }
}

impl ConsensusOutputQuarantine {
    fn insert_shared_object_next_versions(&mut self, output: &ConsensusCommitOutput) {
        if let Some(next_versions) = output.next_shared_object_versions.as_ref() {
            for (object_id, next_version) in next_versions {
                self.shared_object_next_versions
                    .insert(*object_id, *next_version);
            }
        }
    }

    fn insert_congestion_control_debts(&mut self, output: &ConsensusCommitOutput) {
        let current_round = output.consensus_round;

        for (object_id, debt) in output.congestion_control_object_debts.iter() {
            self.congestion_control_object_debts.insert(
                *object_id,
                CongestionPerObjectDebt::new(current_round, *debt),
            );
        }

        for (object_id, debt) in output.congestion_control_randomness_object_debts.iter() {
            self.congestion_control_randomness_object_debts.insert(
                *object_id,
                CongestionPerObjectDebt::new(current_round, *debt),
            );
        }
    }

    fn remove_congestion_control_debts(&mut self, output: &ConsensusCommitOutput) {
        for (object_id, _) in output.congestion_control_object_debts.iter() {
            self.congestion_control_object_debts.remove(object_id);
        }
        for (object_id, _) in output.congestion_control_randomness_object_debts.iter() {
            self.congestion_control_randomness_object_debts
                .remove(object_id);
        }
    }

    fn insert_processed_consensus_messages(&mut self, output: &ConsensusCommitOutput) {
        for tx_key in output.consensus_messages_processed.iter() {
            self.processed_consensus_messages.insert(tx_key.clone(), ());
        }
    }

    fn remove_processed_consensus_messages(&mut self, output: &ConsensusCommitOutput) {
        for tx_key in output.consensus_messages_processed.iter() {
            self.processed_consensus_messages.remove(tx_key);
        }
    }

    fn remove_shared_object_next_versions(&mut self, output: &ConsensusCommitOutput) {
        if let Some(next_versions) = output.next_shared_object_versions.as_ref() {
            for object_id in next_versions.keys() {
                if !self.shared_object_next_versions.remove(object_id) {
                    fatal!(
                        "Shared object next version not found in quarantine: {:?}",
                        object_id
                    );
                }
            }
        }
    }

    fn insert_owned_object_locks(&mut self, output: &ConsensusCommitOutput) {
        for (obj_ref, lock) in &output.owned_object_locks {
            self.owned_object_locks.insert(*obj_ref, *lock);
        }
    }

    fn remove_owned_object_locks(&mut self, output: &ConsensusCommitOutput) {
        for obj_ref in output.owned_object_locks.keys() {
            self.owned_object_locks.remove(obj_ref);
        }
    }
}

// Read methods - all methods in this block return data from the quarantine which would otherwise
// be found in the database.
impl ConsensusOutputQuarantine {
    pub(super) fn last_built_summary(&self) -> Option<&BuilderCheckpointSummary> {
        self.builder_checkpoint_summary.values().last()
    }

    pub(super) fn get_built_summary(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Option<&BuilderCheckpointSummary> {
        self.builder_checkpoint_summary.get(&sequence)
    }

    pub(super) fn is_consensus_message_processed(
        &self,
        key: &SequencedConsensusTransactionKey,
    ) -> bool {
        self.processed_consensus_messages.contains_key(key)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.output_queue.is_empty()
    }

    pub(super) fn get_next_shared_object_versions(
        &self,
        tables: &AuthorityEpochTables,
        objects_to_init: &[ConsensusObjectSequenceKey],
    ) -> SuiResult<Vec<Option<SequenceNumber>>> {
        Ok(do_fallback_lookup(
            objects_to_init,
            |object_key| {
                if let Some(next_version) = self.shared_object_next_versions.get(object_key) {
                    CacheResult::Hit(Some(*next_version))
                } else {
                    CacheResult::Miss
                }
            },
            |object_keys| {
                tables
                    .next_shared_object_versions_v2
                    .multi_get(object_keys)
                    .expect("db error")
            },
        ))
    }

    /// In-memory-only lock lookup: locks acquired by commits that are still quarantined
    /// (not yet flushed to the epoch DB). Used by the objects-table-based conflict
    /// resolution, which covers flushed commits via the objects table instead of the DB.
    pub(super) fn get_owned_object_lock_in_memory(
        &self,
        obj_ref: &ObjectRef,
    ) -> Option<LockDetails> {
        self.owned_object_locks.get(obj_ref).copied()
    }

    pub(super) fn get_highest_pending_checkpoint_height(&self) -> Option<CheckpointHeight> {
        self.output_queue
            .back()
            .and_then(|output| output.get_highest_pending_checkpoint_height())
    }

    pub(super) fn get_pending_checkpoints(
        &self,
        last: Option<CheckpointHeight>,
    ) -> Vec<(CheckpointHeight, PendingCheckpoint)> {
        let mut checkpoints = Vec::new();
        for output in &self.output_queue {
            checkpoints.extend(
                output
                    .get_pending_checkpoints(last)
                    .map(|cp| (cp.height(), cp.clone())),
            );
        }
        if cfg!(debug_assertions) {
            let mut prev = None;
            for (height, _) in &checkpoints {
                if let Some(prev) = prev {
                    assert!(prev < *height);
                }
                prev = Some(*height);
            }
        }
        checkpoints
    }

    pub(super) fn pending_checkpoint_exists(&self, index: &CheckpointHeight) -> bool {
        self.output_queue
            .iter()
            .any(|output| output.pending_checkpoint_exists(index))
    }

    pub(super) fn get_new_jwks(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        round: u64,
    ) -> SuiResult<Vec<ActiveJwk>> {
        let epoch = epoch_store.epoch();

        // Check if the requested round is in memory
        for output in self.output_queue.iter().rev() {
            // unwrap safe because output will always have last consensus stats set before being added
            // to the quarantine
            let output_round = output.get_round().unwrap();
            if round == output_round {
                return Ok(output
                    .active_jwks
                    .iter()
                    .map(|(_, (jwk_id, jwk))| ActiveJwk {
                        jwk_id: jwk_id.clone(),
                        jwk: jwk.clone(),
                        epoch,
                    })
                    .collect());
            }
        }

        // Fall back to reading from database
        let empty_jwk_id = JwkId::new(String::new(), String::new());
        let empty_jwk = JWK {
            kty: String::new(),
            e: String::new(),
            n: String::new(),
            alg: String::new(),
        };

        let start = (round, (empty_jwk_id.clone(), empty_jwk.clone()));
        let end = (round + 1, (empty_jwk_id, empty_jwk));

        Ok(epoch_store
            .tables()?
            .active_jwks
            .safe_iter_with_bounds(Some(start), Some(end))
            .map_ok(|((r, (jwk_id, jwk)), _)| {
                debug_assert!(round == r);
                ActiveJwk { jwk_id, jwk, epoch }
            })
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub(super) fn get_randomness_last_round_timestamp(&self) -> Option<TimestampMs> {
        self.output_queue
            .iter()
            .rev()
            .filter_map(|output| output.get_randomness_last_round_timestamp())
            .next()
    }

    pub(crate) fn load_initial_object_debts(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        current_round: Round,
        for_randomness: bool,
        transactions: &[VerifiedExecutableTransactionWithAliases],
    ) -> SuiResult<impl IntoIterator<Item = (ObjectID, u64)>> {
        let protocol_config = epoch_store.protocol_config();
        let tables = epoch_store.tables()?;
        let default_per_commit_budget = protocol_config
            .max_accumulated_txn_cost_per_object_in_mysticeti_commit_as_option()
            .unwrap_or(0);
        let (hash_table, db_table, per_commit_budget) = if for_randomness {
            (
                &self.congestion_control_randomness_object_debts,
                &tables.congestion_control_randomness_object_debts,
                protocol_config
                    .max_accumulated_randomness_txn_cost_per_object_in_mysticeti_commit_as_option()
                    .unwrap_or(default_per_commit_budget),
            )
        } else {
            (
                &self.congestion_control_object_debts,
                &tables.congestion_control_object_debts,
                default_per_commit_budget,
            )
        };
        let mut shared_input_object_ids: Vec<_> = transactions
            .iter()
            .flat_map(|tx| tx.tx().shared_input_objects().map(|obj| obj.id))
            .collect();
        shared_input_object_ids.sort();
        shared_input_object_ids.dedup();

        let results = do_fallback_lookup(
            &shared_input_object_ids,
            |object_id| {
                if let Some(debt) = hash_table.get(object_id) {
                    CacheResult::Hit(Some(debt.into_v1()))
                } else {
                    CacheResult::Miss
                }
            },
            |object_ids| {
                db_table
                    .multi_get(object_ids)
                    .expect("db error")
                    .into_iter()
                    .map(|debt| debt.map(|debt| debt.into_v1()))
                    .collect()
            },
        );

        Ok(results
            .into_iter()
            .zip_debug_eq(shared_input_object_ids)
            .filter_map(|(debt, object_id)| debt.map(|debt| (debt, object_id)))
            .map(move |((round, debt), object_id)| {
                // Stored debts already account for the budget of the round in which
                // they were accumulated. Application of budget from future rounds to
                // the debt is handled here.
                assert!(current_round > round);
                let num_rounds = current_round - round - 1;
                let debt = debt.saturating_sub(per_commit_budget * num_rounds);
                (object_id, debt)
            }))
    }
}

// A wrapper around HashMap that uses refcounts to keep entries alive until
// they are no longer needed.
//
// If there are N inserts for the same key, the key will not be removed until
// there are N removes.
//
// It is intended to track the *latest* value for a given key, so duplicate
// inserts are intended to overwrite any prior value.
#[derive(Debug, Default)]
struct RefCountedHashMap<K, V> {
    map: HashMap<K, (usize, V)>,
}

impl<K, V> RefCountedHashMap<K, V>
where
    K: Clone + Eq + std::hash::Hash,
{
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        let entry = self.map.entry(key);
        match entry {
            hash_map::Entry::Occupied(mut entry) => {
                let (ref_count, v) = entry.get_mut();
                *ref_count += 1;
                *v = value;
            }
            hash_map::Entry::Vacant(entry) => {
                entry.insert((1, value));
            }
        }
    }

    // Returns true if the key was present, false otherwise.
    // Note that the key may not be removed if present, as it may have a refcount > 1.
    pub fn remove(&mut self, key: &K) -> bool {
        let entry = self.map.entry(key.clone());
        match entry {
            hash_map::Entry::Occupied(mut entry) => {
                let (ref_count, _) = entry.get_mut();
                *ref_count -= 1;
                if *ref_count == 0 {
                    entry.remove();
                }
                true
            }
            hash_map::Entry::Vacant(_) => false,
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key).map(|(_, v)| v)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }
}

#[cfg(test)]
impl ConsensusOutputQuarantine {
    fn output_queue_len_for_testing(&self) -> usize {
        self.output_queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use sui_types::base_types::ExecutionDigests;
    use sui_types::gas::GasCostSummary;
    use sui_types::messages_checkpoint::CheckpointContents;

    fn make_output(height: u64, round: u64, drained: bool) -> ConsensusCommitOutput {
        let mut output = ConsensusCommitOutput::new(round);
        output.record_consensus_commit_stats(ExecutionIndicesWithStatsV2 {
            height,
            ..Default::default()
        });
        output.set_checkpoint_queue_drained(drained);
        output
    }

    fn make_builder_summary(
        seq: CheckpointSequenceNumber,
        height: CheckpointHeight,
        protocol_config: &ProtocolConfig,
    ) -> BuilderCheckpointSummary {
        let contents =
            CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::random()]);
        let summary = CheckpointSummary::new(
            protocol_config,
            0,
            seq,
            0,
            &contents,
            None,
            GasCostSummary::default(),
            None,
            0,
            vec![],
            vec![],
        );
        BuilderCheckpointSummary {
            summary,
            checkpoint_height: Some(height),
            position_in_commit: 0,
        }
    }

    #[tokio::test]
    async fn test_drain_boundary_prevents_premature_commit() {
        let state = TestAuthorityBuilder::new().build().await;
        let epoch_store = state.epoch_store_for_testing();

        let metrics = epoch_store.metrics.clone();
        let mut quarantine = ConsensusOutputQuarantine::new(0, metrics);

        // Output C: height=4, not drained
        let c = make_output(4, 1, false);
        quarantine.push_consensus_output(c, &epoch_store).unwrap();

        // Output C2: height=5, drained
        let c2 = make_output(5, 2, true);
        quarantine.push_consensus_output(c2, &epoch_store).unwrap();

        assert_eq!(quarantine.output_queue_len_for_testing(), 2);

        // Insert builder summaries for checkpoints 1-4 with checkpoint_height = seq
        let pc = epoch_store.protocol_config();
        for seq in 1..=4 {
            let summary = make_builder_summary(seq, seq, pc);
            quarantine.insert_builder_summary(seq, summary);
        }

        // Certify up to checkpoint 4
        let mut batch = epoch_store.db_batch_for_test();
        quarantine
            .update_highest_executed_checkpoint(4, &epoch_store, &mut batch)
            .unwrap();
        batch.write().unwrap();

        // C has height=4 which is <= 4 but checkpoint_queue_drained=false.
        // C2 has height=5 which is > 4, so it's skipped.
        // No drain boundary found => nothing drained.
        assert_eq!(quarantine.output_queue_len_for_testing(), 2);
    }

    #[tokio::test]
    async fn test_drain_boundary_commits_at_safe_point() {
        let state = TestAuthorityBuilder::new().build().await;
        let epoch_store = state.epoch_store_for_testing();

        let metrics = epoch_store.metrics.clone();
        let mut quarantine = ConsensusOutputQuarantine::new(0, metrics);

        let c = make_output(4, 1, false);
        quarantine.push_consensus_output(c, &epoch_store).unwrap();

        let c2 = make_output(5, 2, true);
        quarantine.push_consensus_output(c2, &epoch_store).unwrap();

        assert_eq!(quarantine.output_queue_len_for_testing(), 2);

        // Insert builder summaries for checkpoints 1-5 with checkpoint_height = seq
        let pc = epoch_store.protocol_config();
        for seq in 1..=5 {
            let summary = make_builder_summary(seq, seq, pc);
            quarantine.insert_builder_summary(seq, summary);
        }

        // Certify up to checkpoint 5
        let mut batch = epoch_store.db_batch_for_test();
        quarantine
            .update_highest_executed_checkpoint(5, &epoch_store, &mut batch)
            .unwrap();
        batch.write().unwrap();

        // C has height=4 <= 5, drained=false.
        // C2 has height=5 <= 5, drained=true => drain boundary at index 1.
        // Both outputs drained.
        assert_eq!(quarantine.output_queue_len_for_testing(), 0);
    }

    // Regression test: transaction T defers in commit C1 (locks in C1's output and the
    // deferred-locks map) and is reloaded for scheduling in a later commit C2. C1 can
    // flush before C2; T's conflict coverage must survive that window even though the
    // flat quarantine map drops C1's entries at C1's flush — the deferred-locks entry
    // lives until C2 flushes, by which point T is executed and the objects table covers
    // its consumed inputs.
    #[tokio::test]
    async fn test_deferred_lock_coverage_survives_deferring_commit_flush() {
        use sui_types::base_types::ObjectDigest;

        let state = TestAuthorityBuilder::new().build().await;
        let epoch_store = state.epoch_store_for_testing();

        let metrics = epoch_store.metrics.clone();
        let mut quarantine = ConsensusOutputQuarantine::new(0, metrics);

        let t_digest = TransactionDigest::random();
        let lock_ref: ObjectRef = (
            ObjectID::random(),
            SequenceNumber::from_u64(1),
            ObjectDigest::random(),
        );

        // C1: T is finalized and deferred - locks acquired into the output, refs into
        // the deferred-locks map (as the deferral bookkeeping does).
        let mut c1 = make_output(1, 1, true);
        c1.set_owned_object_locks([(lock_ref, t_digest)].into_iter().collect());
        epoch_store
            .consensus_output_cache
            .deferred_transaction_locks
            .lock()
            .insert(t_digest, vec![lock_ref]);
        quarantine.push_consensus_output(c1, &epoch_store).unwrap();

        assert_eq!(
            quarantine.get_owned_object_lock_in_memory(&lock_ref),
            Some(t_digest)
        );

        // C2: reloads T for scheduling (T does not re-defer).
        let mut c2 = make_output(2, 2, true);
        c2.set_finalized_reloaded_deferred_txns(vec![t_digest]);
        quarantine.push_consensus_output(c2, &epoch_store).unwrap();

        // Flush C1 only.
        let pc = epoch_store.protocol_config();
        quarantine.insert_builder_summary(1, make_builder_summary(1, 1, pc));
        let mut batch = epoch_store.db_batch_for_test();
        quarantine
            .update_highest_executed_checkpoint(1, &epoch_store, &mut batch)
            .unwrap();
        batch.write().unwrap();
        assert_eq!(quarantine.output_queue_len_for_testing(), 1);

        // The flat map dropped C1's entry, but the deferred-locks map still covers T.
        assert_eq!(quarantine.get_owned_object_lock_in_memory(&lock_ref), None);
        assert_eq!(
            epoch_store.get_deferred_transaction_lock(&lock_ref),
            Some(t_digest)
        );

        // Flush C2: T is executed by then (flush gate), so the deferred entry is
        // released and the objects table takes over.
        quarantine.insert_builder_summary(2, make_builder_summary(2, 2, pc));
        let mut batch = epoch_store.db_batch_for_test();
        quarantine
            .update_highest_executed_checkpoint(2, &epoch_store, &mut batch)
            .unwrap();
        batch.write().unwrap();
        assert_eq!(quarantine.output_queue_len_for_testing(), 0);
        assert_eq!(epoch_store.get_deferred_transaction_lock(&lock_ref), None);
    }
}

#[cfg(test)]
mod deferred_locks_tests {
    use super::*;
    use sui_types::base_types::{ObjectDigest, SequenceNumber};

    fn obj_ref(version: u64) -> ObjectRef {
        (
            ObjectID::random(),
            SequenceNumber::from_u64(version),
            ObjectDigest::random(),
        )
    }

    #[test]
    fn test_deferred_transaction_locks() {
        let mut locks = DeferredTransactionLocks::default();
        let tx1 = TransactionDigest::random();
        let tx2 = TransactionDigest::random();
        let (a, b, c) = (obj_ref(1), obj_ref(2), obj_ref(3));

        locks.insert(tx1, vec![a, b]);
        locks.insert(tx2, vec![c]);
        assert_eq!(locks.get(&a), Some(tx1));
        assert_eq!(locks.get(&b), Some(tx1));
        assert_eq!(locks.get(&c), Some(tx2));

        // Removal returns the refs and clears both indexes.
        let removed = locks.remove_tx(&tx1).unwrap();
        assert_eq!(removed, vec![a, b]);
        assert_eq!(locks.get(&a), None);
        assert_eq!(locks.get(&b), None);
        assert_eq!(locks.get(&c), Some(tx2));
        assert_eq!(locks.remove_tx(&tx1), None);

        // Re-deferral cycle: insert again after removal.
        locks.insert(tx1, vec![a]);
        assert_eq!(locks.get(&a), Some(tx1));
    }
}
