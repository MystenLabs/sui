// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::authority::authority_per_epoch_store::{EncG, ExecutionIndicesWithStats, PkG};
use crate::authority::transaction_deferral::DeferralKey;
use crate::epoch::randomness::SINGLETON_KEY;
use fastcrypto_tbls::{dkg_v1, nodes::PartyId};
use fastcrypto_zkp::bn254::zk_login::{JwkId, JWK};
use sui_types::base_types::{AuthorityName, SequenceNumber};
use sui_types::crypto::RandomnessRound;
use sui_types::error::SuiResult;
use sui_types::{
    base_types::{ConsensusObjectSequenceKey, ObjectID},
    digests::TransactionDigest,
    messages_consensus::{Round, TimestampMs, VersionedDkgConfirmation},
    signature::GenericSignature,
};
use typed_store::rocks::DBBatch;

use crate::{
    authority::{
        authority_per_epoch_store::AuthorityPerEpochStore,
        epoch_start_configuration::EpochStartConfigTrait,
        shared_object_congestion_tracker::CongestionPerObjectDebt,
    },
    checkpoints::PendingCheckpointV2,
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
    pending_execution: Vec<VerifiedExecutableTransaction>,

    // transaction scheduling state
    shared_object_versions: Option<(
        AssignedTxAndVersions,
        HashMap<ConsensusObjectSequenceKey, SequenceNumber>,
    )>,

    deferred_txns: Vec<(DeferralKey, Vec<VerifiedSequencedConsensusTransaction>)>,
    // deferred txns that have been loaded and can be removed
    deleted_deferred_txns: BTreeSet<DeferralKey>,

    // checkpoint state
    user_signatures_for_checkpoints: Vec<(TransactionDigest, Vec<GenericSignature>)>,
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

    pub fn insert_end_of_publish(&mut self, authority: AuthorityName) {
        self.end_of_publish.insert(authority);
    }

    pub fn insert_pending_execution(&mut self, transactions: &[VerifiedExecutableTransaction]) {
        self.pending_execution.reserve(transactions.len());
        self.pending_execution.extend_from_slice(transactions);
    }

    pub fn insert_user_signatures_for_checkpoints(
        &mut self,
        transactions: &[VerifiedExecutableTransaction],
    ) {
        self.user_signatures_for_checkpoints.extend(
            transactions
                .iter()
                .map(|tx| (*tx.digest(), tx.tx_signatures().to_vec())),
        );
    }

    pub fn record_consensus_commit_stats(&mut self, stats: ExecutionIndicesWithStats) {
        self.consensus_commit_stats = Some(stats);
    }

    pub fn store_reconfig_state(&mut self, state: ReconfigState) {
        self.reconfig_state = Some(state);
    }

    pub fn record_consensus_message_processed(&mut self, key: SequencedConsensusTransactionKey) {
        self.consensus_messages_processed.insert(key);
    }

    pub fn set_assigned_shared_object_versions(
        &mut self,
        versions: AssignedTxAndVersions,
        next_versions: HashMap<ConsensusObjectSequenceKey, SequenceNumber>,
    ) {
        assert!(self.shared_object_versions.is_none());
        self.shared_object_versions = Some((versions, next_versions));
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

        if let Some(consensus_commit_stats) = &self.consensus_commit_stats {
            batch.insert_batch(
                &tables.last_consensus_stats,
                [(LAST_CONSENSUS_STATS_ADDR, consensus_commit_stats)],
            )?;
        }

        batch.insert_batch(
            &tables.pending_execution,
            self.pending_execution
                .into_iter()
                .map(|tx| (*tx.inner().digest(), tx.serializable())),
        )?;

        if let Some((assigned_versions, next_versions)) = self.shared_object_versions {
            if epoch_store
                .epoch_start_config()
                .use_version_assignment_tables_v3()
            {
                batch.insert_batch(
                    &tables.assigned_shared_object_versions_v3,
                    assigned_versions,
                )?;
                batch.insert_batch(&tables.next_shared_object_versions_v2, next_versions)?;
            } else {
                batch.insert_batch(
                    &tables.assigned_shared_object_versions_v2,
                    assigned_versions.into_iter().map(|(key, versions)| {
                        (
                            key,
                            versions
                                .into_iter()
                                .map(|(id, v)| (id.0, v))
                                .collect::<Vec<_>>(),
                        )
                    }),
                )?;
                batch.insert_batch(
                    &tables.next_shared_object_versions,
                    next_versions.into_iter().map(|(key, v)| (key.0, v)),
                )?;
            }
        }

        batch.delete_batch(&tables.deferred_transactions, self.deleted_deferred_txns)?;
        batch.insert_batch(&tables.deferred_transactions, self.deferred_txns)?;

        batch.insert_batch(
            &tables.user_signatures_for_checkpoints,
            self.user_signatures_for_checkpoints,
        )?;

        batch.insert_batch(
            &tables.pending_checkpoints_v2,
            self.pending_checkpoints
                .into_iter()
                .map(|cp| (cp.height(), cp)),
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
            batch.insert_batch(&tables.dkg_output, [(SINGLETON_KEY, output)])?;
        }

        batch.insert_batch(
            &tables.pending_jwks,
            self.pending_jwks.into_iter().map(|j| (j, ())),
        )?;
        batch.insert_batch(
            &tables.active_jwks,
            self.active_jwks.into_iter().map(|j| (j, ())),
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
