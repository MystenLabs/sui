// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{hash_map, BTreeMap, BTreeSet, HashMap, VecDeque};

use crate::authority::authority_per_epoch_store::{
    AuthorityEpochTables, EncG, ExecutionIndicesWithStats, PkG,
};
use crate::authority::transaction_deferral::DeferralKey;
use crate::checkpoints::BuilderCheckpointSummary;
use crate::consensus_handler::SequencedConsensusTransactionKind;
use crate::epoch::randomness::SINGLETON_KEY;
use dashmap::DashMap;
use fastcrypto_tbls::{dkg_v1, nodes::PartyId};
use fastcrypto_zkp::bn254::zk_login::{JwkId, JWK};
use mysten_common::fatal;
use parking_lot::Mutex;
use sui_types::authenticator_state::ActiveJwk;
use sui_types::base_types::{AuthorityName, SequenceNumber};
use sui_types::crypto::RandomnessRound;
use sui_types::error::SuiResult;
use sui_types::messages_checkpoint::{CheckpointContents, CheckpointSequenceNumber};
use sui_types::messages_consensus::{ConsensusTransaction, ConsensusTransactionKind};
use sui_types::{
    base_types::{ConsensusObjectSequenceKey, ObjectID},
    digests::TransactionDigest,
    messages_consensus::{Round, TimestampMs, VersionedDkgConfirmation},
    signature::GenericSignature,
    transaction::TransactionKey,
};
use tracing::{debug, info};
use typed_store::rocks::DBBatch;
use typed_store::Map;

use crate::{
    authority::{
        authority_per_epoch_store::AuthorityPerEpochStore,
        epoch_start_configuration::{EpochStartConfigTrait, EpochStartConfiguration},
        shared_object_congestion_tracker::CongestionPerObjectDebt,
    },
    checkpoints::{CheckpointHeight, PendingCheckpointV2},
    consensus_handler::{SequencedConsensusTransactionKey, VerifiedSequencedConsensusTransaction},
    epoch::{
        randomness::{VersionedProcessedMessage, VersionedUsedProcessedMessages},
        reconfiguration::ReconfigState,
    },
};

use super::*;

#[derive(Default)]
pub(crate) struct ConsensusCommitOutput {
    // Consensus and reconfig state
    consensus_round: Round,
    consensus_messages_processed: BTreeSet<SequencedConsensusTransactionKey>,
    end_of_publish: BTreeSet<AuthorityName>,
    reconfig_state: Option<ReconfigState>,
    consensus_commit_stats: Option<ExecutionIndicesWithStats>,

    // transaction scheduling state
    next_shared_object_versions: Option<HashMap<ConsensusObjectSequenceKey, SequenceNumber>>,

    // TODO: If we delay committing consensus output until after all deferrals have been loaded,
    // we can move deferred_txns to the ConsensusOutputCache and save disk bandwidth.
    deferred_txns: Vec<(DeferralKey, Vec<VerifiedSequencedConsensusTransaction>)>,
    // deferred txns that have been loaded and can be removed
    deleted_deferred_txns: BTreeSet<DeferralKey>,

    // checkpoint state
    pending_checkpoints: Vec<PendingCheckpointV2>,

    // random beacon state
    next_randomness_round: Option<(RandomnessRound, TimestampMs)>,

    dkg_confirmations: BTreeMap<PartyId, VersionedDkgConfirmation>,
    dkg_processed_messages: BTreeMap<PartyId, VersionedProcessedMessage>,
    dkg_used_message: Option<VersionedUsedProcessedMessages>,
    dkg_output: Option<dkg_v1::Output<PkG, EncG>>,

    // jwk state
    pending_jwks: BTreeSet<(AuthorityName, JwkId, JWK)>,
    active_jwks: BTreeSet<(u64, (JwkId, JWK))>,

    // congestion control state
    congestion_control_object_debts: Vec<(ObjectID, u64)>,
    congestion_control_randomness_object_debts: Vec<(ObjectID, u64)>,
}

impl ConsensusCommitOutput {
    pub fn new(consensus_round: Round) -> Self {
        Self {
            consensus_round,
            ..Default::default()
        }
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
    ) -> impl Iterator<Item = &PendingCheckpointV2> {
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

    pub(crate) fn record_consensus_commit_stats(&mut self, stats: ExecutionIndicesWithStats) {
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
        transactions: Vec<VerifiedSequencedConsensusTransaction>,
    ) {
        self.deferred_txns.push((key, transactions));
    }

    pub fn delete_loaded_deferred_transactions(&mut self, deferral_keys: &[DeferralKey]) {
        self.deleted_deferred_txns
            .extend(deferral_keys.iter().cloned());
    }

    pub fn insert_pending_checkpoint(&mut self, checkpoint: PendingCheckpointV2) {
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

    pub fn set_dkg_output(&mut self, output: dkg_v1::Output<PkG, EncG>) {
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
            &tables.last_consensus_stats,
            [(LAST_CONSENSUS_STATS_ADDR, consensus_commit_stats)],
        )?;

        if let Some(next_versions) = self.next_shared_object_versions {
            if epoch_store
                .epoch_start_config()
                .use_version_assignment_tables_v3()
            {
                batch.insert_batch(&tables.next_shared_object_versions_v2, next_versions)?;
            } else {
                batch.insert_batch(
                    &tables.next_shared_object_versions,
                    next_versions.into_iter().map(|((id, _), v)| (id, v)),
                )?;
            }
        }

        batch.delete_batch(&tables.deferred_transactions, self.deleted_deferred_txns)?;
        batch.insert_batch(&tables.deferred_transactions, self.deferred_txns)?;

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
            batch.insert_batch(&tables.dkg_output, [(SINGLETON_KEY, output)])?;
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

        Ok(())
    }
}

/// ConsensusOutputCache holds outputs of consensus processing that do not need to be committed to disk.
/// Data quarantining guarantees that all of this data will be used (e.g. for building checkpoints)
/// before the consensus commit from which it originated is marked as processed. Therefore we can rely
/// on replay of consensus commits to recover this data.
pub(crate) struct ConsensusOutputCache {
    // shared version assignments is a DashMap because it is read from execution so we don't
    // want contention.
    shared_version_assignments:
        DashMap<TransactionKey, Vec<(ConsensusObjectSequenceKey, SequenceNumber)>>,

    // deferred transactions is only used by consensus handler so there should never be lock contention
    // - hence no need for a DashMap.
    pub(super) deferred_transactions:
        Mutex<BTreeMap<DeferralKey, Vec<VerifiedSequencedConsensusTransaction>>>,
    // user_signatures_for_checkpoints is written to by consensus handler and read from by checkpoint builder
    // The critical sections are small in both cases so a DashMap is probably not helpful.
    pub(super) user_signatures_for_checkpoints:
        Mutex<HashMap<TransactionDigest, Vec<GenericSignature>>>,

    metrics: Arc<EpochMetrics>,
}

impl ConsensusOutputCache {
    pub(crate) fn new(
        epoch_start_configuration: &EpochStartConfiguration,
        tables: &AuthorityEpochTables,
        metrics: Arc<EpochMetrics>,
    ) -> Self {
        let deferred_transactions = tables
            .get_all_deferred_transactions()
            .expect("load deferred transactions cannot fail");

        if !epoch_start_configuration.is_data_quarantine_active_from_beginning_of_epoch() {
            let shared_version_assignments =
                Self::get_all_shared_version_assignments(epoch_start_configuration, tables);

            let user_signatures_for_checkpoints = tables
                .get_all_user_signatures_for_checkpoints()
                .expect("load user signatures for checkpoints cannot fail");

            Self {
                shared_version_assignments: shared_version_assignments.into_iter().collect(),
                deferred_transactions: Mutex::new(deferred_transactions),
                user_signatures_for_checkpoints: Mutex::new(user_signatures_for_checkpoints),
                metrics,
            }
        } else {
            Self {
                shared_version_assignments: Default::default(),
                deferred_transactions: Mutex::new(deferred_transactions),
                user_signatures_for_checkpoints: Default::default(),
                metrics,
            }
        }
    }

    pub fn num_shared_version_assignments(&self) -> usize {
        self.shared_version_assignments.len()
    }

    pub fn get_assigned_shared_object_versions(
        &self,
        key: &TransactionKey,
    ) -> Option<Vec<(ConsensusObjectSequenceKey, SequenceNumber)>> {
        self.shared_version_assignments
            .get(key)
            .map(|locks| locks.clone())
    }

    pub fn insert_shared_object_assignments(&self, versions: &AssignedTxAndVersions) {
        trace!("insert_shared_object_assignments: {:?}", versions);
        let mut inserted_count = 0;
        for (key, value) in versions {
            if self
                .shared_version_assignments
                .insert(*key, value.clone())
                .is_none()
            {
                inserted_count += 1;
            }
        }
        self.metrics
            .shared_object_assignments_size
            .add(inserted_count as i64);
    }

    pub fn set_shared_object_versions_for_testing(
        &self,
        tx_digest: &TransactionDigest,
        assigned_versions: &[(ConsensusObjectSequenceKey, SequenceNumber)],
    ) {
        self.shared_version_assignments.insert(
            TransactionKey::Digest(*tx_digest),
            assigned_versions.to_owned(),
        );
    }

    pub fn remove_shared_object_assignments<'a>(
        &self,
        keys: impl IntoIterator<Item = &'a TransactionKey>,
    ) {
        let mut removed_count = 0;
        for tx_key in keys {
            if self.shared_version_assignments.remove(tx_key).is_some() {
                removed_count += 1;
            }
        }
        self.metrics
            .shared_object_assignments_size
            .sub(removed_count as i64);
    }

    // Used to read pre-existing shared object versions from the database after a crash.
    // TODO: remove this once all nodes have upgraded to data-quarantining.
    fn get_all_shared_version_assignments(
        epoch_start_configuration: &EpochStartConfiguration,
        tables: &AuthorityEpochTables,
    ) -> Vec<(
        TransactionKey,
        Vec<(ConsensusObjectSequenceKey, SequenceNumber)>,
    )> {
        if epoch_start_configuration.use_version_assignment_tables_v3() {
            tables
                .assigned_shared_object_versions_v3
                .safe_iter()
                .collect::<Result<_, _>>()
                .expect("db error")
        } else {
            tables
                .assigned_shared_object_versions_v2
                .safe_iter()
                .collect::<Result<Vec<_>, _>>()
                .expect("db error")
                .into_iter()
                .map(|(key, value)| {
                    (
                        key,
                        value
                            .into_iter()
                            .map(|(id, v)| ((id, SequenceNumber::UNKNOWN), v))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect()
        }
    }
}

/// ConsensusOutputQuarantine holds outputs of consensus processing in memory until the checkpoints
/// for the commit have been certified.
pub(crate) struct ConsensusOutputQuarantine {
    // Output from consensus handler
    output_queue: VecDeque<ConsensusCommitOutput>,

    // Highest known certified checkpoint sequence number
    highest_executed_checkpoint: CheckpointSequenceNumber,

    // Checkpoint Builder output
    builder_checkpoint_summary:
        BTreeMap<CheckpointSequenceNumber, (BuilderCheckpointSummary, CheckpointContents)>,

    builder_digest_to_checkpoint: HashMap<TransactionDigest, CheckpointSequenceNumber>,

    // Any un-committed next versions are stored here.
    shared_object_next_versions: RefCountedHashMap<ConsensusObjectSequenceKey, SequenceNumber>,

    // The most recent congestion control debts for objects. Uses a ref-count to track
    // which objects still exist in some element of output_queue.
    congestion_control_randomness_object_debts:
        RefCountedHashMap<ObjectID, CongestionPerObjectDebt>,
    congestion_control_object_debts: RefCountedHashMap<ObjectID, CongestionPerObjectDebt>,

    processed_consensus_messages: RefCountedHashMap<SequencedConsensusTransactionKey, ()>,

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
            builder_digest_to_checkpoint: HashMap::new(),
            shared_object_next_versions: RefCountedHashMap::new(),
            processed_consensus_messages: RefCountedHashMap::new(),
            congestion_control_randomness_object_debts: RefCountedHashMap::new(),
            congestion_control_object_debts: RefCountedHashMap::new(),
            metrics: authority_metrics,
        }
    }
}

// Write methods - all methods in this block insert new data into the quarantine.
// There are only two sources! ConsensusHandler and CheckpointBuilder.
impl ConsensusOutputQuarantine {
    // Push all data gathered from a consensus commit into the quarantine.
    pub(super) fn push_consensus_output(
        &mut self,
        output: ConsensusCommitOutput,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult {
        self.insert_shared_object_next_versions(&output);
        self.insert_congestion_control_debts(&output);
        self.insert_processed_consensus_messages(&output);
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
        contents: CheckpointContents,
    ) {
        debug!(?sequence_number, "inserting builder summary {:?}", summary);
        for tx in contents.iter() {
            self.builder_digest_to_checkpoint
                .insert(tx.transaction, sequence_number);
        }
        self.builder_checkpoint_summary
            .insert(sequence_number, (summary, contents));
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
            let (seq, (builder_summary, contents)) =
                self.builder_checkpoint_summary.pop_first().unwrap();

            for tx in contents.iter() {
                let digest = &tx.transaction;
                assert_eq!(
                    self.builder_digest_to_checkpoint
                        .remove(digest)
                        .unwrap_or_else(|| {
                            panic!(
                                "transaction {:?} not found in builder_digest_to_checkpoint",
                                digest
                            )
                        }),
                    seq
                );
            }

            batch.insert_batch(
                &tables.builder_digest_to_checkpoint,
                contents.iter().map(|tx| (tx.transaction, seq)),
            )?;

            batch.insert_batch(
                &tables.builder_checkpoint_summary_v2,
                [(seq, &builder_summary)],
            )?;

            let checkpoint_height = builder_summary
                .checkpoint_height
                .expect("non-genesis checkpoint must have height");
            if let Some(highest) = highest_committed_height {
                assert!(checkpoint_height > highest);
            }

            highest_committed_height = Some(checkpoint_height);
        }

        let Some(highest_committed_height) = highest_committed_height else {
            return Ok(());
        };

        while !self.output_queue.is_empty() {
            // A consensus commit can have more than one pending checkpoint (a regular one and a randomnes one).
            // We can only write the consensus commit if the highest pending checkpoint associated with it has
            // been processed by the builder.
            let Some(highest_in_commit) = self
                .output_queue
                .front()
                .unwrap()
                .get_highest_pending_checkpoint_height()
            else {
                // if highest is none, we have already written the pending checkpoint for the final epoch,
                // so there is no more data that needs to be committed.
                break;
            };

            if highest_in_commit <= highest_committed_height {
                info!(
                    "committing output with highest pending checkpoint height {:?}",
                    highest_in_commit
                );
                let output = self.output_queue.pop_front().unwrap();
                self.remove_shared_object_next_versions(&output);
                self.remove_processed_consensus_messages(&output);
                self.remove_congestion_control_debts(&output);
                epoch_store.remove_shared_version_assignments(
                    output
                        .pending_checkpoints
                        .iter()
                        .flat_map(|c| c.roots().iter()),
                );
                output.write_to_batch(epoch_store, batch)?;
            } else {
                break;
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
}

// Read methods - all methods in this block return data from the quarantine which would otherwise
// be found in the database.
impl ConsensusOutputQuarantine {
    pub(super) fn last_built_summary(&self) -> Option<&BuilderCheckpointSummary> {
        self.builder_checkpoint_summary
            .values()
            .last()
            .map(|(summary, _)| summary)
    }

    pub(super) fn get_built_summary(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Option<&BuilderCheckpointSummary> {
        self.builder_checkpoint_summary
            .get(&sequence)
            .map(|(summary, _)| summary)
    }

    pub(super) fn included_transaction_in_checkpoint(&self, digest: &TransactionDigest) -> bool {
        self.builder_digest_to_checkpoint.contains_key(digest)
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
        epoch_start_config: &EpochStartConfiguration,
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
                if epoch_start_config.use_version_assignment_tables_v3() {
                    tables
                        .next_shared_object_versions_v2
                        .multi_get(object_keys)
                        .expect("db error")
                } else {
                    tables
                        .next_shared_object_versions
                        .multi_get(object_keys.iter().map(|(id, _)| *id))
                        .expect("db error")
                }
            },
        ))
    }

    pub(super) fn get_highest_pending_checkpoint_height(&self) -> Option<CheckpointHeight> {
        self.output_queue
            .back()
            .and_then(|output| output.get_highest_pending_checkpoint_height())
    }

    pub(super) fn get_pending_checkpoints(
        &self,
        last: Option<CheckpointHeight>,
    ) -> Vec<(CheckpointHeight, PendingCheckpointV2)> {
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

    pub(super) fn load_initial_object_debts(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        current_round: Round,
        for_randomness: bool,
        transactions: &[VerifiedSequencedConsensusTransaction],
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
            .filter_map(|tx| {
                if let SequencedConsensusTransactionKind::External(ConsensusTransaction {
                    kind: ConsensusTransactionKind::CertifiedTransaction(tx),
                    ..
                }) = &tx.0.transaction
                {
                    Some(tx.shared_input_objects().map(|obj| obj.id))
                } else {
                    None
                }
            })
            .flatten()
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
            .zip(shared_input_object_ids)
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
