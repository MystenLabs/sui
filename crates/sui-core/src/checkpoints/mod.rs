// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod causal_order;
pub mod checkpoint_executor;
mod checkpoint_output;
mod metrics;

use crate::authority::AuthorityState;
use crate::authority_client::{make_network_authority_clients_with_network_config, AuthorityAPI};
use crate::checkpoints::causal_order::CausalOrder;
use crate::checkpoints::checkpoint_output::{CertifiedCheckpointOutput, CheckpointOutput};
pub use crate::checkpoints::checkpoint_output::{
    LogCheckpointOutput, SendCheckpointToStateSync, SubmitCheckpointToConsensus,
};
pub use crate::checkpoints::metrics::CheckpointMetrics;
use crate::execution_cache::TransactionCacheRead;
use crate::stake_aggregator::{InsertResult, MultiStakeAggregator};
use crate::state_accumulator::StateAccumulator;
use diffy::create_patch;
use itertools::Itertools;
use mysten_common::random::get_rng;
use mysten_common::sync::notify_read::NotifyRead;
use mysten_common::{assert_reachable, debug_fatal, fatal};
use mysten_metrics::{monitored_future, monitored_scope, MonitoredFutureExt};
use nonempty::NonEmpty;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sui_macros::fail_point;
use sui_network::default_mysten_network_config;
use sui_types::base_types::ConciseableName;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::execution::ExecutionTimeObservationKey;
use sui_types::messages_checkpoint::CheckpointCommitment;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use tokio::sync::{mpsc, watch};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_handler::SequencedConsensusTransactionKey;
use rand::seq::SliceRandom;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::Weak;
use std::time::{Duration, SystemTime};
use sui_protocol_config::ProtocolVersion;
use sui_types::base_types::{AuthorityName, EpochId, TransactionDigest};
use sui_types::committee::StakeUnit;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::digests::{CheckpointContentsDigest, CheckpointDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::error::{SuiError, SuiResult};
use sui_types::gas::GasCostSummary;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointResponseV2, CheckpointSequenceNumber,
    CheckpointSignatureMessage, CheckpointSummary, CheckpointSummaryResponse, CheckpointTimestamp,
    EndOfEpochData, FullCheckpointContents, TrustedCheckpoint, VerifiedCheckpoint,
    VerifiedCheckpointContents,
};
use sui_types::messages_checkpoint::{CheckpointRequestV2, SignedCheckpointSummary};
use sui_types::messages_consensus::ConsensusTransactionKey;
use sui_types::signature::GenericSignature;
use sui_types::sui_system_state::{SuiSystemState, SuiSystemStateTrait};
use sui_types::transaction::{TransactionDataAPI, TransactionKey, TransactionKind};
use tokio::{sync::Notify, task::JoinSet, time::timeout};
use tracing::{debug, error, info, instrument, trace, warn};
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store::DBMapUtils;
use typed_store::Map;
use typed_store::{
    rocks::{DBMap, MetricConf},
    TypedStoreError,
};

pub type CheckpointHeight = u64;

pub struct EpochStats {
    pub checkpoint_count: u64,
    pub transaction_count: u64,
    pub total_gas_reward: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingCheckpointInfo {
    pub timestamp_ms: CheckpointTimestamp,
    pub last_of_epoch: bool,
    pub checkpoint_height: CheckpointHeight,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingCheckpoint {
    pub roots: Vec<TransactionDigest>,
    pub details: PendingCheckpointInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PendingCheckpointV2 {
    // This is an enum for future upgradability, though at the moment there is only one variant.
    V2(PendingCheckpointV2Contents),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingCheckpointV2Contents {
    pub roots: Vec<TransactionKey>,
    pub details: PendingCheckpointInfo,
}

impl PendingCheckpointV2 {
    pub fn as_v2(&self) -> &PendingCheckpointV2Contents {
        match self {
            PendingCheckpointV2::V2(contents) => contents,
        }
    }

    pub fn into_v2(self) -> PendingCheckpointV2Contents {
        match self {
            PendingCheckpointV2::V2(contents) => contents,
        }
    }

    pub fn expect_v1(self) -> PendingCheckpoint {
        let v2 = self.into_v2();
        PendingCheckpoint {
            roots: v2
                .roots
                .into_iter()
                .map(|root| *root.unwrap_digest())
                .collect(),
            details: v2.details,
        }
    }

    pub fn roots(&self) -> &Vec<TransactionKey> {
        &self.as_v2().roots
    }

    pub fn details(&self) -> &PendingCheckpointInfo {
        &self.as_v2().details
    }

    pub fn height(&self) -> CheckpointHeight {
        self.details().checkpoint_height
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuilderCheckpointSummary {
    pub summary: CheckpointSummary,
    // Height at which this checkpoint summary was built. None for genesis checkpoint
    pub checkpoint_height: Option<CheckpointHeight>,
    pub position_in_commit: usize,
}

#[derive(DBMapUtils)]
pub struct CheckpointStoreTables {
    /// Maps checkpoint contents digest to checkpoint contents
    pub(crate) checkpoint_content: DBMap<CheckpointContentsDigest, CheckpointContents>,

    /// Maps checkpoint contents digest to checkpoint sequence number
    pub(crate) checkpoint_sequence_by_contents_digest:
        DBMap<CheckpointContentsDigest, CheckpointSequenceNumber>,

    /// Stores entire checkpoint contents from state sync, indexed by sequence number, for
    /// efficient reads of full checkpoints. Entries from this table are deleted after state
    /// accumulation has completed.
    full_checkpoint_content: DBMap<CheckpointSequenceNumber, FullCheckpointContents>,

    /// Stores certified checkpoints
    pub(crate) certified_checkpoints: DBMap<CheckpointSequenceNumber, TrustedCheckpoint>,
    /// Map from checkpoint digest to certified checkpoint
    pub(crate) checkpoint_by_digest: DBMap<CheckpointDigest, TrustedCheckpoint>,

    /// Store locally computed checkpoint summaries so that we can detect forks and log useful
    /// information. Can be pruned as soon as we verify that we are in agreement with the latest
    /// certified checkpoint.
    pub(crate) locally_computed_checkpoints: DBMap<CheckpointSequenceNumber, CheckpointSummary>,

    /// A map from epoch ID to the sequence number of the last checkpoint in that epoch.
    epoch_last_checkpoint_map: DBMap<EpochId, CheckpointSequenceNumber>,

    /// Watermarks used to determine the highest verified, fully synced, and
    /// fully executed checkpoints
    pub(crate) watermarks: DBMap<CheckpointWatermark, (CheckpointSequenceNumber, CheckpointDigest)>,
}

impl CheckpointStoreTables {
    pub fn new(path: &Path, metric_name: &'static str) -> Self {
        Self::open_tables_read_write(path.to_path_buf(), MetricConf::new(metric_name), None, None)
    }

    pub fn open_readonly(path: &Path) -> CheckpointStoreTablesReadOnly {
        Self::get_read_only_handle(
            path.to_path_buf(),
            None,
            None,
            MetricConf::new("checkpoint_readonly"),
        )
    }
}

pub struct CheckpointStore {
    pub(crate) tables: CheckpointStoreTables,
    synced_checkpoint_notify_read: NotifyRead<CheckpointSequenceNumber, VerifiedCheckpoint>,
    executed_checkpoint_notify_read: NotifyRead<CheckpointSequenceNumber, VerifiedCheckpoint>,
}

impl CheckpointStore {
    pub fn new(path: &Path) -> Arc<Self> {
        let tables = CheckpointStoreTables::new(path, "checkpoint");
        Arc::new(Self {
            tables,
            synced_checkpoint_notify_read: NotifyRead::new(),
            executed_checkpoint_notify_read: NotifyRead::new(),
        })
    }

    pub fn new_for_tests() -> Arc<Self> {
        let ckpt_dir = mysten_common::tempdir().unwrap();
        CheckpointStore::new(ckpt_dir.path())
    }

    pub fn new_for_db_checkpoint_handler(path: &Path) -> Arc<Self> {
        let tables = CheckpointStoreTables::new(path, "db_checkpoint");
        Arc::new(Self {
            tables,
            synced_checkpoint_notify_read: NotifyRead::new(),
            executed_checkpoint_notify_read: NotifyRead::new(),
        })
    }

    pub fn open_readonly(path: &Path) -> CheckpointStoreTablesReadOnly {
        CheckpointStoreTables::open_readonly(path)
    }

    #[instrument(level = "info", skip_all)]
    pub fn insert_genesis_checkpoint(
        &self,
        checkpoint: VerifiedCheckpoint,
        contents: CheckpointContents,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        assert_eq!(
            checkpoint.epoch(),
            0,
            "can't call insert_genesis_checkpoint with a checkpoint not in epoch 0"
        );
        assert_eq!(
            *checkpoint.sequence_number(),
            0,
            "can't call insert_genesis_checkpoint with a checkpoint that doesn't have a sequence number of 0"
        );

        // Only insert the genesis checkpoint if the DB is empty and doesn't have it already
        if self
            .get_checkpoint_by_digest(checkpoint.digest())
            .unwrap()
            .is_none()
        {
            if epoch_store.epoch() == checkpoint.epoch {
                epoch_store
                    .put_genesis_checkpoint_in_builder(checkpoint.data(), &contents)
                    .unwrap();
            } else {
                debug!(
                    validator_epoch =% epoch_store.epoch(),
                    genesis_epoch =% checkpoint.epoch(),
                    "Not inserting checkpoint builder data for genesis checkpoint",
                );
            }
            self.insert_checkpoint_contents(contents).unwrap();
            self.insert_verified_checkpoint(&checkpoint).unwrap();
            self.update_highest_synced_checkpoint(&checkpoint).unwrap();
        }
    }

    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        self.tables
            .checkpoint_by_digest
            .get(digest)
            .map(|maybe_checkpoint| maybe_checkpoint.map(|c| c.into()))
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        self.tables
            .certified_checkpoints
            .get(&sequence_number)
            .map(|maybe_checkpoint| maybe_checkpoint.map(|c| c.into()))
    }

    pub fn get_locally_computed_checkpoint(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointSummary>, TypedStoreError> {
        self.tables
            .locally_computed_checkpoints
            .get(&sequence_number)
    }

    pub fn multi_get_locally_computed_checkpoints(
        &self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Result<Vec<Option<CheckpointSummary>>, TypedStoreError> {
        let checkpoints = self
            .tables
            .locally_computed_checkpoints
            .multi_get(sequence_numbers)?;

        Ok(checkpoints)
    }

    pub fn get_sequence_number_by_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, TypedStoreError> {
        self.tables
            .checkpoint_sequence_by_contents_digest
            .get(digest)
    }

    pub fn delete_contents_digest_sequence_number_mapping(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<(), TypedStoreError> {
        self.tables
            .checkpoint_sequence_by_contents_digest
            .remove(digest)
    }

    pub fn get_latest_certified_checkpoint(
        &self,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        Ok(self
            .tables
            .certified_checkpoints
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
            .transpose()?
            .map(|(_, v)| v.into()))
    }

    pub fn get_latest_locally_computed_checkpoint(
        &self,
    ) -> Result<Option<CheckpointSummary>, TypedStoreError> {
        Ok(self
            .tables
            .locally_computed_checkpoints
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
            .transpose()?
            .map(|(_, v)| v))
    }

    pub fn multi_get_checkpoint_by_sequence_number(
        &self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Result<Vec<Option<VerifiedCheckpoint>>, TypedStoreError> {
        let checkpoints = self
            .tables
            .certified_checkpoints
            .multi_get(sequence_numbers)?
            .into_iter()
            .map(|maybe_checkpoint| maybe_checkpoint.map(|c| c.into()))
            .collect();

        Ok(checkpoints)
    }

    pub fn multi_get_checkpoint_content(
        &self,
        contents_digest: &[CheckpointContentsDigest],
    ) -> Result<Vec<Option<CheckpointContents>>, TypedStoreError> {
        self.tables.checkpoint_content.multi_get(contents_digest)
    }

    pub fn get_highest_verified_checkpoint(
        &self,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        let highest_verified = if let Some(highest_verified) = self
            .tables
            .watermarks
            .get(&CheckpointWatermark::HighestVerified)?
        {
            highest_verified
        } else {
            return Ok(None);
        };
        self.get_checkpoint_by_digest(&highest_verified.1)
    }

    pub fn get_highest_synced_checkpoint(
        &self,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        let highest_synced = if let Some(highest_synced) = self
            .tables
            .watermarks
            .get(&CheckpointWatermark::HighestSynced)?
        {
            highest_synced
        } else {
            return Ok(None);
        };
        self.get_checkpoint_by_digest(&highest_synced.1)
    }

    pub fn get_highest_synced_checkpoint_seq_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>, TypedStoreError> {
        if let Some(highest_synced) = self
            .tables
            .watermarks
            .get(&CheckpointWatermark::HighestSynced)?
        {
            Ok(Some(highest_synced.0))
        } else {
            Ok(None)
        }
    }

    pub fn get_highest_executed_checkpoint_seq_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>, TypedStoreError> {
        if let Some(highest_executed) = self
            .tables
            .watermarks
            .get(&CheckpointWatermark::HighestExecuted)?
        {
            Ok(Some(highest_executed.0))
        } else {
            Ok(None)
        }
    }

    pub fn get_highest_executed_checkpoint(
        &self,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        let highest_executed = if let Some(highest_executed) = self
            .tables
            .watermarks
            .get(&CheckpointWatermark::HighestExecuted)?
        {
            highest_executed
        } else {
            return Ok(None);
        };
        self.get_checkpoint_by_digest(&highest_executed.1)
    }

    pub fn get_highest_pruned_checkpoint_seq_number(
        &self,
    ) -> Result<CheckpointSequenceNumber, TypedStoreError> {
        Ok(self
            .tables
            .watermarks
            .get(&CheckpointWatermark::HighestPruned)?
            .unwrap_or_default()
            .0)
    }

    pub fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>, TypedStoreError> {
        self.tables.checkpoint_content.get(digest)
    }

    pub fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>, TypedStoreError> {
        self.tables.full_checkpoint_content.get(&seq)
    }

    fn prune_local_summaries(&self) -> SuiResult {
        if let Some((last_local_summary, _)) = self
            .tables
            .locally_computed_checkpoints
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
            .transpose()?
        {
            let mut batch = self.tables.locally_computed_checkpoints.batch();
            batch.schedule_delete_range(
                &self.tables.locally_computed_checkpoints,
                &0,
                &last_local_summary,
            )?;
            batch.write()?;
            info!("Pruned local summaries up to {:?}", last_local_summary);
        }
        Ok(())
    }

    fn check_for_checkpoint_fork(
        &self,
        local_checkpoint: &CheckpointSummary,
        verified_checkpoint: &VerifiedCheckpoint,
    ) {
        if local_checkpoint != verified_checkpoint.data() {
            let verified_contents = self
                .get_checkpoint_contents(&verified_checkpoint.content_digest)
                .map(|opt_contents| {
                    opt_contents
                        .map(|contents| format!("{:?}", contents))
                        .unwrap_or_else(|| {
                            format!(
                                "Verified checkpoint contents not found, digest: {:?}",
                                verified_checkpoint.content_digest,
                            )
                        })
                })
                .map_err(|e| {
                    format!(
                        "Failed to get verified checkpoint contents, digest: {:?} error: {:?}",
                        verified_checkpoint.content_digest, e
                    )
                })
                .unwrap_or_else(|err_msg| err_msg);

            let local_contents = self
                .get_checkpoint_contents(&local_checkpoint.content_digest)
                .map(|opt_contents| {
                    opt_contents
                        .map(|contents| format!("{:?}", contents))
                        .unwrap_or_else(|| {
                            format!(
                                "Local checkpoint contents not found, digest: {:?}",
                                local_checkpoint.content_digest
                            )
                        })
                })
                .map_err(|e| {
                    format!(
                        "Failed to get local checkpoint contents, digest: {:?} error: {:?}",
                        local_checkpoint.content_digest, e
                    )
                })
                .unwrap_or_else(|err_msg| err_msg);

            // checkpoint contents may be too large for panic message.
            error!(
                verified_checkpoint = ?verified_checkpoint.data(),
                ?verified_contents,
                ?local_checkpoint,
                ?local_contents,
                "Local checkpoint fork detected!",
            );
            fatal!(
                "Local checkpoint fork detected for sequence number: {}",
                local_checkpoint.sequence_number()
            );
        }
    }

    // Called by consensus (ConsensusAggregator).
    // Different from `insert_verified_checkpoint`, it does not touch
    // the highest_verified_checkpoint watermark such that state sync
    // will have a chance to process this checkpoint and perform some
    // state-sync only things.
    pub fn insert_certified_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        debug!(
            checkpoint_seq = checkpoint.sequence_number(),
            "Inserting certified checkpoint",
        );
        let mut batch = self.tables.certified_checkpoints.batch();
        batch
            .insert_batch(
                &self.tables.certified_checkpoints,
                [(checkpoint.sequence_number(), checkpoint.serializable_ref())],
            )?
            .insert_batch(
                &self.tables.checkpoint_by_digest,
                [(checkpoint.digest(), checkpoint.serializable_ref())],
            )?;
        if checkpoint.next_epoch_committee().is_some() {
            batch.insert_batch(
                &self.tables.epoch_last_checkpoint_map,
                [(&checkpoint.epoch(), checkpoint.sequence_number())],
            )?;
        }
        batch.write()?;

        if let Some(local_checkpoint) = self
            .tables
            .locally_computed_checkpoints
            .get(checkpoint.sequence_number())?
        {
            self.check_for_checkpoint_fork(&local_checkpoint, checkpoint);
        }

        Ok(())
    }

    // Called by state sync, apart from inserting the checkpoint and updating
    // related tables, it also bumps the highest_verified_checkpoint watermark.
    #[instrument(level = "debug", skip_all)]
    pub fn insert_verified_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        self.insert_certified_checkpoint(checkpoint)?;
        self.update_highest_verified_checkpoint(checkpoint)
    }

    pub fn update_highest_verified_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        if Some(*checkpoint.sequence_number())
            > self
                .get_highest_verified_checkpoint()?
                .map(|x| *x.sequence_number())
        {
            debug!(
                checkpoint_seq = checkpoint.sequence_number(),
                "Updating highest verified checkpoint",
            );
            self.tables.watermarks.insert(
                &CheckpointWatermark::HighestVerified,
                &(*checkpoint.sequence_number(), *checkpoint.digest()),
            )?;
        }

        Ok(())
    }

    pub fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        let seq = *checkpoint.sequence_number();
        debug!(checkpoint_seq = seq, "Updating highest synced checkpoint",);
        self.tables.watermarks.insert(
            &CheckpointWatermark::HighestSynced,
            &(seq, *checkpoint.digest()),
        )?;
        self.synced_checkpoint_notify_read.notify(&seq, checkpoint);
        Ok(())
    }

    async fn notify_read_checkpoint_watermark<F>(
        &self,
        notify_read: &NotifyRead<CheckpointSequenceNumber, VerifiedCheckpoint>,
        seq: CheckpointSequenceNumber,
        get_watermark: F,
    ) -> VerifiedCheckpoint
    where
        F: Fn() -> Option<CheckpointSequenceNumber>,
    {
        notify_read
            .read(&[seq], |seqs| {
                let seq = seqs[0];
                let Some(highest) = get_watermark() else {
                    return vec![None];
                };
                if highest < seq {
                    return vec![None];
                }
                let checkpoint = self
                    .get_checkpoint_by_sequence_number(seq)
                    .expect("db error")
                    .expect("checkpoint not found");
                vec![Some(checkpoint)]
            })
            .await
            .into_iter()
            .next()
            .unwrap()
    }

    pub async fn notify_read_synced_checkpoint(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> VerifiedCheckpoint {
        self.notify_read_checkpoint_watermark(&self.synced_checkpoint_notify_read, seq, || {
            self.get_highest_synced_checkpoint_seq_number()
                .expect("db error")
        })
        .await
    }

    pub async fn notify_read_executed_checkpoint(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> VerifiedCheckpoint {
        self.notify_read_checkpoint_watermark(&self.executed_checkpoint_notify_read, seq, || {
            self.get_highest_executed_checkpoint_seq_number()
                .expect("db error")
        })
        .await
    }

    pub fn update_highest_executed_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        if let Some(seq_number) = self.get_highest_executed_checkpoint_seq_number()? {
            if seq_number >= *checkpoint.sequence_number() {
                return Ok(());
            }
            assert_eq!(
                seq_number + 1,
                *checkpoint.sequence_number(),
                "Cannot update highest executed checkpoint to {} when current highest executed checkpoint is {}",
                checkpoint.sequence_number(),
                seq_number
            );
        }
        let seq = *checkpoint.sequence_number();
        debug!(checkpoint_seq = seq, "Updating highest executed checkpoint",);
        self.tables.watermarks.insert(
            &CheckpointWatermark::HighestExecuted,
            &(seq, *checkpoint.digest()),
        )?;
        self.executed_checkpoint_notify_read
            .notify(&seq, checkpoint);
        Ok(())
    }

    pub fn update_highest_pruned_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        self.tables.watermarks.insert(
            &CheckpointWatermark::HighestPruned,
            &(*checkpoint.sequence_number(), *checkpoint.digest()),
        )
    }

    /// Sets highest executed checkpoint to any value.
    ///
    /// WARNING: This method is very subtle and can corrupt the database if used incorrectly.
    /// It should only be used in one-off cases or tests after fully understanding the risk.
    pub fn set_highest_executed_checkpoint_subtle(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        self.tables.watermarks.insert(
            &CheckpointWatermark::HighestExecuted,
            &(*checkpoint.sequence_number(), *checkpoint.digest()),
        )
    }

    pub fn insert_checkpoint_contents(
        &self,
        contents: CheckpointContents,
    ) -> Result<(), TypedStoreError> {
        debug!(
            checkpoint_seq = ?contents.digest(),
            "Inserting checkpoint contents",
        );
        self.tables
            .checkpoint_content
            .insert(contents.digest(), &contents)
    }

    pub fn insert_verified_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        full_contents: VerifiedCheckpointContents,
    ) -> Result<(), TypedStoreError> {
        let mut batch = self.tables.full_checkpoint_content.batch();
        batch.insert_batch(
            &self.tables.checkpoint_sequence_by_contents_digest,
            [(&checkpoint.content_digest, checkpoint.sequence_number())],
        )?;
        let full_contents = full_contents.into_inner();
        batch.insert_batch(
            &self.tables.full_checkpoint_content,
            [(checkpoint.sequence_number(), &full_contents)],
        )?;

        let contents = full_contents.into_checkpoint_contents();
        assert_eq!(&checkpoint.content_digest, contents.digest());

        batch.insert_batch(
            &self.tables.checkpoint_content,
            [(contents.digest(), &contents)],
        )?;

        batch.write()
    }

    pub fn delete_full_checkpoint_contents(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> Result<(), TypedStoreError> {
        self.tables.full_checkpoint_content.remove(&seq)
    }

    pub fn get_epoch_last_checkpoint(
        &self,
        epoch_id: EpochId,
    ) -> SuiResult<Option<VerifiedCheckpoint>> {
        let seq = self.get_epoch_last_checkpoint_seq_number(epoch_id)?;
        let checkpoint = match seq {
            Some(seq) => self.get_checkpoint_by_sequence_number(seq)?,
            None => None,
        };
        Ok(checkpoint)
    }

    pub fn get_epoch_last_checkpoint_seq_number(
        &self,
        epoch_id: EpochId,
    ) -> SuiResult<Option<CheckpointSequenceNumber>> {
        let seq = self.tables.epoch_last_checkpoint_map.get(&epoch_id)?;
        Ok(seq)
    }

    pub fn insert_epoch_last_checkpoint(
        &self,
        epoch_id: EpochId,
        checkpoint: &VerifiedCheckpoint,
    ) -> SuiResult {
        self.tables
            .epoch_last_checkpoint_map
            .insert(&epoch_id, checkpoint.sequence_number())?;
        Ok(())
    }

    pub fn get_epoch_state_commitments(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<Vec<CheckpointCommitment>>> {
        let commitments = self.get_epoch_last_checkpoint(epoch)?.map(|checkpoint| {
            checkpoint
                .end_of_epoch_data
                .as_ref()
                .expect("Last checkpoint of epoch expected to have EndOfEpochData")
                .epoch_commitments
                .clone()
        });
        Ok(commitments)
    }

    /// Given the epoch ID, and the last checkpoint of the epoch, derive a few statistics of the epoch.
    pub fn get_epoch_stats(
        &self,
        epoch: EpochId,
        last_checkpoint: &CheckpointSummary,
    ) -> Option<EpochStats> {
        let (first_checkpoint, prev_epoch_network_transactions) = if epoch == 0 {
            (0, 0)
        } else if let Ok(Some(checkpoint)) = self.get_epoch_last_checkpoint(epoch - 1) {
            (
                checkpoint.sequence_number + 1,
                checkpoint.network_total_transactions,
            )
        } else {
            return None;
        };
        Some(EpochStats {
            checkpoint_count: last_checkpoint.sequence_number - first_checkpoint + 1,
            transaction_count: last_checkpoint.network_total_transactions
                - prev_epoch_network_transactions,
            total_gas_reward: last_checkpoint
                .epoch_rolling_gas_cost_summary
                .computation_cost,
        })
    }

    pub fn checkpoint_db(&self, path: &Path) -> SuiResult {
        // This checkpoints the entire db and not one column family
        self.tables
            .checkpoint_content
            .checkpoint_db(path)
            .map_err(Into::into)
    }

    pub fn delete_highest_executed_checkpoint_test_only(&self) -> Result<(), TypedStoreError> {
        let mut wb = self.tables.watermarks.batch();
        wb.delete_batch(
            &self.tables.watermarks,
            std::iter::once(CheckpointWatermark::HighestExecuted),
        )?;
        wb.write()?;
        Ok(())
    }

    pub fn reset_db_for_execution_since_genesis(&self) -> SuiResult {
        self.delete_highest_executed_checkpoint_test_only()?;
        self.tables.watermarks.rocksdb.flush()?;
        Ok(())
    }

    /// TODO: this is only needed while upgrading from non-dataquarantine to
    /// dataquarantine. After that it can be deleted.
    ///
    /// Re-executes all transactions from all local, uncertified checkpoints for crash recovery.
    /// All transactions thus re-executed are guaranteed to not have any missing dependencies,
    /// because we start from the highest executed checkpoint, and proceed through checkpoints in
    /// order.
    #[instrument(level = "debug", skip_all)]
    pub async fn reexecute_local_checkpoints(
        &self,
        state: &AuthorityState,
        epoch_store: &AuthorityPerEpochStore,
    ) {
        let epoch = epoch_store.epoch();
        let highest_executed = self
            .get_highest_executed_checkpoint_seq_number()
            .expect("get_highest_executed_checkpoint_seq_number should not fail")
            .unwrap_or(0);

        let Ok(Some(highest_built)) = self.get_latest_locally_computed_checkpoint() else {
            info!("no locally built checkpoints to verify");
            return;
        };

        info!(
            "rexecuting locally computed checkpoints for crash recovery from {} to {}",
            highest_executed, highest_built
        );

        for seq in highest_executed + 1..=*highest_built.sequence_number() {
            info!(?seq, "Re-executing locally computed checkpoint");
            let Some(checkpoint) = self
                .get_locally_computed_checkpoint(seq)
                .expect("get_locally_computed_checkpoint should not fail")
            else {
                panic!("locally computed checkpoint {:?} not found", seq);
            };

            let Some(contents) = self
                .get_checkpoint_contents(&checkpoint.content_digest)
                .expect("get_checkpoint_contents should not fail")
            else {
                panic!("checkpoint contents not found for locally computed checkpoint {:?} (digest: {:?})", seq, checkpoint.content_digest);
            };

            let cache = state.get_transaction_cache_reader();

            let tx_digests: Vec<_> = contents.iter().map(|digests| digests.transaction).collect();
            let fx_digests: Vec<_> = contents.iter().map(|digests| digests.effects).collect();
            let txns = cache.multi_get_transaction_blocks(&tx_digests);

            let txns: Vec<_> = itertools::izip!(txns, tx_digests, fx_digests)
                .filter_map(|(tx, digest, fx)| {
                    if let Some(tx) = tx {
                        Some((tx, fx))
                    } else {
                        info!(
                            "transaction {:?} not found during checkpoint re-execution",
                            digest
                        );
                        None
                    }
                })
                // end of epoch transaction can only be executed by CheckpointExecutor
                .filter(|(tx, _)| !tx.data().transaction_data().is_end_of_epoch_tx())
                .map(|(tx, fx)| {
                    (
                        VerifiedExecutableTransaction::new_from_checkpoint(
                            (*tx).clone(),
                            epoch,
                            seq,
                        ),
                        fx,
                    )
                })
                .collect();

            let tx_digests: Vec<_> = txns.iter().map(|(tx, _)| *tx.digest()).collect();

            info!(
                ?seq,
                ?tx_digests,
                "Re-executing transactions for locally built checkpoint"
            );
            // this will panic if any re-execution diverges from the previously recorded effects digest
            state.enqueue_with_expected_effects_digest(txns, epoch_store);

            // a task that logs every so often until it is cancelled
            // This should normally finish very quickly, so seeing this log more than once or twice is
            // likely a sign of a problem.
            let waiting_logger = tokio::task::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    warn!(?seq, "Still waiting for re-execution to complete");
                }
            });

            cache
                .notify_read_executed_effects_digests(&tx_digests)
                .await;

            waiting_logger.abort();
            waiting_logger.await.ok();
            info!(?seq, "Re-execution completed for locally built checkpoint");
        }

        info!("Re-execution of locally built checkpoints completed");
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointWatermark {
    HighestVerified,
    HighestSynced,
    HighestExecuted,
    HighestPruned,
}

struct CheckpointAccumulator {
    epoch_store: Arc<AuthorityPerEpochStore>,
    accumulator: Weak<StateAccumulator>,
    receive_from_builder: mpsc::Receiver<(CheckpointSequenceNumber, Vec<TransactionEffects>)>,
}

impl CheckpointAccumulator {
    fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        accumulator: Weak<StateAccumulator>,
        receive_from_builder: mpsc::Receiver<(CheckpointSequenceNumber, Vec<TransactionEffects>)>,
    ) -> Self {
        Self {
            epoch_store,
            accumulator,
            receive_from_builder,
        }
    }

    async fn run(self) {
        let Self {
            epoch_store,
            accumulator,
            mut receive_from_builder,
        } = self;
        while let Some((seq, effects)) = receive_from_builder.recv().await {
            let Some(accumulator) = accumulator.upgrade() else {
                info!("Accumulator was dropped, stopping checkpoint accumulation");
                break;
            };
            accumulator
                .accumulate_checkpoint(&effects, seq, &epoch_store)
                .expect("epoch ended while accumulating checkpoint");
        }
    }
}

pub struct CheckpointBuilder {
    state: Arc<AuthorityState>,
    store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    notify: Arc<Notify>,
    notify_aggregator: Arc<Notify>,
    last_built: watch::Sender<CheckpointSequenceNumber>,
    effects_store: Arc<dyn TransactionCacheRead>,
    accumulator: Weak<StateAccumulator>,
    send_to_accumulator: mpsc::Sender<(CheckpointSequenceNumber, Vec<TransactionEffects>)>,
    output: Box<dyn CheckpointOutput>,
    metrics: Arc<CheckpointMetrics>,
    max_transactions_per_checkpoint: usize,
    max_checkpoint_size_bytes: usize,
}

pub struct CheckpointAggregator {
    store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    notify: Arc<Notify>,
    current: Option<CheckpointSignatureAggregator>,
    output: Box<dyn CertifiedCheckpointOutput>,
    state: Arc<AuthorityState>,
    metrics: Arc<CheckpointMetrics>,
}

// This holds information to aggregate signatures for one checkpoint
pub struct CheckpointSignatureAggregator {
    next_index: u64,
    summary: CheckpointSummary,
    digest: CheckpointDigest,
    /// Aggregates voting stake for each signed checkpoint proposal by authority
    signatures_by_digest: MultiStakeAggregator<CheckpointDigest, CheckpointSummary, true>,
    store: Arc<CheckpointStore>,
    state: Arc<AuthorityState>,
    metrics: Arc<CheckpointMetrics>,
}

impl CheckpointBuilder {
    fn new(
        state: Arc<AuthorityState>,
        store: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        notify: Arc<Notify>,
        effects_store: Arc<dyn TransactionCacheRead>,
        // for synchronous accumulation of end-of-epoch checkpoint
        accumulator: Weak<StateAccumulator>,
        // for asynchronous/concurrent accumulation of regular checkpoints
        send_to_accumulator: mpsc::Sender<(CheckpointSequenceNumber, Vec<TransactionEffects>)>,
        output: Box<dyn CheckpointOutput>,
        notify_aggregator: Arc<Notify>,
        last_built: watch::Sender<CheckpointSequenceNumber>,
        metrics: Arc<CheckpointMetrics>,
        max_transactions_per_checkpoint: usize,
        max_checkpoint_size_bytes: usize,
    ) -> Self {
        Self {
            state,
            store,
            epoch_store,
            notify,
            effects_store,
            accumulator,
            send_to_accumulator,
            output,
            notify_aggregator,
            last_built,
            metrics,
            max_transactions_per_checkpoint,
            max_checkpoint_size_bytes,
        }
    }

    async fn run(mut self) {
        info!("Starting CheckpointBuilder");
        loop {
            self.maybe_build_checkpoints().await;

            self.notify.notified().await;
        }
    }

    async fn maybe_build_checkpoints(&mut self) {
        let _scope = monitored_scope("BuildCheckpoints");

        // Collect info about the most recently built checkpoint.
        let summary = self
            .epoch_store
            .last_built_checkpoint_builder_summary()
            .expect("epoch should not have ended");
        let mut last_height = summary.clone().and_then(|s| s.checkpoint_height);
        let mut last_timestamp = summary.map(|s| s.summary.timestamp_ms);

        let min_checkpoint_interval_ms = self
            .epoch_store
            .protocol_config()
            .min_checkpoint_interval_ms_as_option()
            .unwrap_or_default();
        let mut grouped_pending_checkpoints = Vec::new();
        let mut checkpoints_iter = self
            .epoch_store
            .get_pending_checkpoints(last_height)
            .expect("unexpected epoch store error")
            .into_iter()
            .peekable();
        while let Some((height, pending)) = checkpoints_iter.next() {
            // Group PendingCheckpoints until:
            // - minimum interval has elapsed ...
            let current_timestamp = pending.details().timestamp_ms;
            let can_build = match last_timestamp {
                    Some(last_timestamp) => {
                        current_timestamp >= last_timestamp + min_checkpoint_interval_ms
                    }
                    None => true,
                // - or, next PendingCheckpoint is last-of-epoch (since the last-of-epoch checkpoint
                //   should be written separately) ...
                } || checkpoints_iter
                    .peek()
                    .is_some_and(|(_, next_pending)| next_pending.details().last_of_epoch)
                // - or, we have reached end of epoch.
                    || pending.details().last_of_epoch;
            grouped_pending_checkpoints.push(pending);
            if !can_build {
                debug!(
                    checkpoint_commit_height = height,
                    ?last_timestamp,
                    ?current_timestamp,
                    "waiting for more PendingCheckpoints: minimum interval not yet elapsed"
                );
                continue;
            }

            // Min interval has elapsed, we can now coalesce and build a checkpoint.
            last_height = Some(height);
            last_timestamp = Some(current_timestamp);
            debug!(
                checkpoint_commit_height = height,
                "Making checkpoint at commit height"
            );

            match self
                .make_checkpoint(std::mem::take(&mut grouped_pending_checkpoints))
                .await
            {
                Ok(seq) => {
                    self.last_built.send_if_modified(|cur| {
                        // when rebuilding checkpoints at startup, seq can be for an old checkpoint
                        if seq > *cur {
                            *cur = seq;
                            true
                        } else {
                            false
                        }
                    });
                }
                Err(e) => {
                    error!("Error while making checkpoint, will retry in 1s: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    self.metrics.checkpoint_errors.inc();
                    return;
                }
            }
            // ensure that the task can be cancelled at end of epoch, even if no other await yields
            // execution.
            tokio::task::yield_now().await;
        }
        debug!(
            "Waiting for more checkpoints from consensus after processing {last_height:?}; {} pending checkpoints left unprocessed until next interval",
            grouped_pending_checkpoints.len(),
        );
    }

    #[instrument(level = "debug", skip_all, fields(last_height = pendings.last().unwrap().details().checkpoint_height))]
    async fn make_checkpoint(
        &self,
        pendings: Vec<PendingCheckpointV2>,
    ) -> anyhow::Result<CheckpointSequenceNumber> {
        let _scope = monitored_scope("CheckpointBuilder::make_checkpoint");
        let last_details = pendings.last().unwrap().details().clone();

        // Keeps track of the effects that are already included in the current checkpoint.
        // This is used when there are multiple pending checkpoints to create a single checkpoint
        // because in such scenarios, dependencies of a transaction may in earlier created checkpoints,
        // or in earlier pending checkpoints.
        let mut effects_in_current_checkpoint = BTreeSet::new();

        // Stores the transactions that should be included in the checkpoint. Transactions will be recorded in the checkpoint
        // in this order.
        let mut sorted_tx_effects_included_in_checkpoint = Vec::new();
        for pending_checkpoint in pendings.into_iter() {
            let pending = pending_checkpoint.into_v2();
            let txn_in_checkpoint = self
                .resolve_checkpoint_transactions(pending.roots, &mut effects_in_current_checkpoint)
                .await?;
            sorted_tx_effects_included_in_checkpoint.extend(txn_in_checkpoint);
        }
        let new_checkpoints = self
            .create_checkpoints(sorted_tx_effects_included_in_checkpoint, &last_details)
            .await?;
        let highest_sequence = *new_checkpoints.last().0.sequence_number();
        self.write_checkpoints(last_details.checkpoint_height, new_checkpoints)
            .await?;
        Ok(highest_sequence)
    }

    // Given the root transactions of a pending checkpoint, resolve the transactions should be included in
    // the checkpoint, and return them in the order they should be included in the checkpoint.
    // `effects_in_current_checkpoint` tracks the transactions that already exist in the current
    // checkpoint.
    #[instrument(level = "debug", skip_all)]
    async fn resolve_checkpoint_transactions(
        &self,
        roots: Vec<TransactionKey>,
        effects_in_current_checkpoint: &mut BTreeSet<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffects>> {
        let _scope = monitored_scope("CheckpointBuilder::resolve_checkpoint_transactions");

        self.metrics
            .checkpoint_roots_count
            .inc_by(roots.len() as u64);

        let root_digests = self
            .epoch_store
            .notify_read_executed_digests(&roots)
            .in_monitored_scope("CheckpointNotifyDigests")
            .await?;
        let root_effects = self
            .effects_store
            .notify_read_executed_effects(&root_digests)
            .in_monitored_scope("CheckpointNotifyRead")
            .await;

        let consensus_commit_prologue = if self
            .epoch_store
            .protocol_config()
            .prepend_prologue_tx_in_consensus_commit_in_checkpoints()
        {
            // If the roots contains consensus commit prologue transaction, we want to extract it,
            // and put it to the front of the checkpoint.

            let consensus_commit_prologue = self
                .extract_consensus_commit_prologue(&root_digests, &root_effects)
                .await?;

            // Get the unincluded depdnencies of the consensus commit prologue. We should expect no
            // other dependencies that haven't been included in any previous checkpoints.
            if let Some((ccp_digest, ccp_effects)) = &consensus_commit_prologue {
                let unsorted_ccp = self.complete_checkpoint_effects(
                    vec![ccp_effects.clone()],
                    effects_in_current_checkpoint,
                )?;

                // No other dependencies of this consensus commit prologue that haven't been included
                // in any previous checkpoint.
                if unsorted_ccp.len() != 1 {
                    fatal!(
                        "Expected 1 consensus commit prologue, got {:?}",
                        unsorted_ccp
                            .iter()
                            .map(|e| e.transaction_digest())
                            .collect::<Vec<_>>()
                    );
                }
                assert_eq!(unsorted_ccp.len(), 1);
                assert_eq!(unsorted_ccp[0].transaction_digest(), ccp_digest);
            }
            consensus_commit_prologue
        } else {
            None
        };

        let unsorted =
            self.complete_checkpoint_effects(root_effects, effects_in_current_checkpoint)?;

        let _scope = monitored_scope("CheckpointBuilder::causal_sort");
        let mut sorted: Vec<TransactionEffects> = Vec::with_capacity(unsorted.len() + 1);
        if let Some((ccp_digest, ccp_effects)) = consensus_commit_prologue {
            if cfg!(debug_assertions) {
                // When consensus_commit_prologue is extracted, it should not be included in the `unsorted`.
                for tx in unsorted.iter() {
                    assert!(tx.transaction_digest() != &ccp_digest);
                }
            }
            sorted.push(ccp_effects);
        }
        sorted.extend(CausalOrder::causal_sort(unsorted));

        #[cfg(msim)]
        {
            // Check consensus commit prologue invariants in sim test.
            self.expensive_consensus_commit_prologue_invariants_check(&root_digests, &sorted);
        }

        Ok(sorted)
    }

    // This function is used to extract the consensus commit prologue digest and effects from the root
    // transactions.
    // This function can only be used when prepend_prologue_tx_in_consensus_commit_in_checkpoints is enabled.
    // The consensus commit prologue is expected to be the first transaction in the roots.
    async fn extract_consensus_commit_prologue(
        &self,
        root_digests: &[TransactionDigest],
        root_effects: &[TransactionEffects],
    ) -> SuiResult<Option<(TransactionDigest, TransactionEffects)>> {
        let _scope = monitored_scope("CheckpointBuilder::extract_consensus_commit_prologue");
        if root_digests.is_empty() {
            return Ok(None);
        }

        // Reads the first transaction in the roots, and checks whether it is a consensus commit prologue
        // transaction.
        // When prepend_prologue_tx_in_consensus_commit_in_checkpoints is enabled, the consensus commit prologue
        // transaction should be the first transaction in the roots written by the consensus handler.
        let first_tx = self
            .state
            .get_transaction_cache_reader()
            .get_transaction_block(&root_digests[0])
            .expect("Transaction block must exist");

        Ok(first_tx
            .transaction_data()
            .is_consensus_commit_prologue()
            .then(|| {
                assert_eq!(first_tx.digest(), root_effects[0].transaction_digest());
                (*first_tx.digest(), root_effects[0].clone())
            }))
    }

    #[instrument(level = "debug", skip_all)]
    async fn write_checkpoints(
        &self,
        height: CheckpointHeight,
        new_checkpoints: NonEmpty<(CheckpointSummary, CheckpointContents)>,
    ) -> SuiResult {
        let _scope = monitored_scope("CheckpointBuilder::write_checkpoints");
        let mut batch = self.store.tables.checkpoint_content.batch();
        let mut all_tx_digests =
            Vec::with_capacity(new_checkpoints.iter().map(|(_, c)| c.size()).sum());

        for (summary, contents) in &new_checkpoints {
            debug!(
                checkpoint_commit_height = height,
                checkpoint_seq = summary.sequence_number,
                contents_digest = ?contents.digest(),
                "writing checkpoint",
            );

            if let Some(previously_computed_summary) = self
                .store
                .tables
                .locally_computed_checkpoints
                .get(&summary.sequence_number)?
            {
                if previously_computed_summary != *summary {
                    // Panic so that we don't send out an equivocating checkpoint sig.
                    fatal!(
                        "Checkpoint {} was previously built with a different result: {:?} vs {:?}",
                        summary.sequence_number,
                        previously_computed_summary,
                        summary
                    );
                }
            }

            all_tx_digests.extend(contents.iter().map(|digests| digests.transaction));

            self.metrics
                .transactions_included_in_checkpoint
                .inc_by(contents.size() as u64);
            let sequence_number = summary.sequence_number;
            self.metrics
                .last_constructed_checkpoint
                .set(sequence_number as i64);

            batch.insert_batch(
                &self.store.tables.checkpoint_content,
                [(contents.digest(), contents)],
            )?;

            batch.insert_batch(
                &self.store.tables.locally_computed_checkpoints,
                [(sequence_number, summary)],
            )?;
        }

        batch.write()?;

        // Send all checkpoint sigs to consensus.
        for (summary, contents) in &new_checkpoints {
            self.output
                .checkpoint_created(summary, contents, &self.epoch_store, &self.store)
                .await?;
        }

        for (local_checkpoint, _) in &new_checkpoints {
            if let Some(certified_checkpoint) = self
                .store
                .tables
                .certified_checkpoints
                .get(local_checkpoint.sequence_number())?
            {
                self.store
                    .check_for_checkpoint_fork(local_checkpoint, &certified_checkpoint.into());
            }
        }

        self.notify_aggregator.notify_one();
        self.epoch_store
            .process_constructed_checkpoint(height, new_checkpoints);
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    fn split_checkpoint_chunks(
        &self,
        effects_and_transaction_sizes: Vec<(TransactionEffects, usize)>,
        signatures: Vec<Vec<GenericSignature>>,
    ) -> anyhow::Result<Vec<Vec<(TransactionEffects, Vec<GenericSignature>)>>> {
        let _guard = monitored_scope("CheckpointBuilder::split_checkpoint_chunks");
        let mut chunks = Vec::new();
        let mut chunk = Vec::new();
        let mut chunk_size: usize = 0;
        for ((effects, transaction_size), signatures) in effects_and_transaction_sizes
            .into_iter()
            .zip(signatures.into_iter())
        {
            // Roll over to a new chunk after either max count or max size is reached.
            // The size calculation here is intended to estimate the size of the
            // FullCheckpointContents struct. If this code is modified, that struct
            // should also be updated accordingly.
            let size = transaction_size
                + bcs::serialized_size(&effects)?
                + bcs::serialized_size(&signatures)?;
            if chunk.len() == self.max_transactions_per_checkpoint
                || (chunk_size + size) > self.max_checkpoint_size_bytes
            {
                if chunk.is_empty() {
                    // Always allow at least one tx in a checkpoint.
                    warn!("Size of single transaction ({size}) exceeds max checkpoint size ({}); allowing excessively large checkpoint to go through.", self.max_checkpoint_size_bytes);
                } else {
                    chunks.push(chunk);
                    chunk = Vec::new();
                    chunk_size = 0;
                }
            }

            chunk.push((effects, signatures));
            chunk_size += size;
        }

        if !chunk.is_empty() || chunks.is_empty() {
            // We intentionally create an empty checkpoint if there is no content provided
            // to make a 'heartbeat' checkpoint.
            // Important: if some conditions are added here later, we need to make sure we always
            // have at least one chunk if last_pending_of_epoch is set
            chunks.push(chunk);
            // Note: empty checkpoints are ok - they shouldn't happen at all on a network with even
            // modest load. Even if they do happen, it is still useful as it allows fullnodes to
            // distinguish between "no transactions have happened" and "i am not receiving new
            // checkpoints".
        }
        Ok(chunks)
    }

    fn load_last_built_checkpoint_summary(
        epoch_store: &AuthorityPerEpochStore,
        store: &CheckpointStore,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, CheckpointSummary)>> {
        let mut last_checkpoint = epoch_store.last_built_checkpoint_summary()?;
        if last_checkpoint.is_none() {
            let epoch = epoch_store.epoch();
            if epoch > 0 {
                let previous_epoch = epoch - 1;
                let last_verified = store.get_epoch_last_checkpoint(previous_epoch)?;
                last_checkpoint = last_verified.map(VerifiedCheckpoint::into_summary_and_sequence);
                if let Some((ref seq, _)) = last_checkpoint {
                    debug!("No checkpoints in builder DB, taking checkpoint from previous epoch with sequence {seq}");
                } else {
                    // This is some serious bug with when CheckpointBuilder started so surfacing it via panic
                    panic!("Can not find last checkpoint for previous epoch {previous_epoch}");
                }
            }
        }
        Ok(last_checkpoint)
    }

    #[instrument(level = "debug", skip_all)]
    async fn create_checkpoints(
        &self,
        all_effects: Vec<TransactionEffects>,
        details: &PendingCheckpointInfo,
    ) -> anyhow::Result<NonEmpty<(CheckpointSummary, CheckpointContents)>> {
        let _scope = monitored_scope("CheckpointBuilder::create_checkpoints");

        let total = all_effects.len();
        let mut last_checkpoint =
            Self::load_last_built_checkpoint_summary(&self.epoch_store, &self.store)?;
        let last_checkpoint_seq = last_checkpoint.as_ref().map(|(seq, _)| *seq);
        info!(
            next_checkpoint_seq = last_checkpoint_seq.unwrap_or_default() + 1,
            checkpoint_timestamp = details.timestamp_ms,
            "Creating checkpoint(s) for {} transactions",
            all_effects.len(),
        );

        let all_digests: Vec<_> = all_effects
            .iter()
            .map(|effect| *effect.transaction_digest())
            .collect();
        let transactions_and_sizes = self
            .state
            .get_transaction_cache_reader()
            .get_transactions_and_serialized_sizes(&all_digests)?;
        let mut all_effects_and_transaction_sizes = Vec::with_capacity(all_effects.len());
        let mut transactions = Vec::with_capacity(all_effects.len());
        let mut transaction_keys = Vec::with_capacity(all_effects.len());
        let mut randomness_rounds = BTreeMap::new();
        {
            let _guard = monitored_scope("CheckpointBuilder::wait_for_transactions_sequenced");
            debug!(
                ?last_checkpoint_seq,
                "Waiting for {:?} certificates to appear in consensus",
                all_effects.len()
            );

            for (effects, transaction_and_size) in all_effects
                .into_iter()
                .zip(transactions_and_sizes.into_iter())
            {
                let (transaction, size) = transaction_and_size
                    .unwrap_or_else(|| panic!("Could not find executed transaction {:?}", effects));
                match transaction.inner().transaction_data().kind() {
                    TransactionKind::ConsensusCommitPrologue(_)
                    | TransactionKind::ConsensusCommitPrologueV2(_)
                    | TransactionKind::ConsensusCommitPrologueV3(_)
                    | TransactionKind::ConsensusCommitPrologueV4(_)
                    | TransactionKind::AuthenticatorStateUpdate(_) => {
                        // ConsensusCommitPrologue and AuthenticatorStateUpdate are guaranteed to be
                        // processed before we reach here.
                    }
                    TransactionKind::RandomnessStateUpdate(rsu) => {
                        randomness_rounds
                            .insert(*effects.transaction_digest(), rsu.randomness_round);
                    }
                    _ => {
                        // All other tx should be included in the call to
                        // `consensus_messages_processed_notify`.
                        transaction_keys.push(SequencedConsensusTransactionKey::External(
                            ConsensusTransactionKey::Certificate(*effects.transaction_digest()),
                        ));
                    }
                }
                transactions.push(transaction);
                all_effects_and_transaction_sizes.push((effects, size));
            }

            self.epoch_store
                .consensus_messages_processed_notify(transaction_keys)
                .await?;
        }

        let signatures = self
            .epoch_store
            .user_signatures_for_checkpoint(&transactions, &all_digests)?;
        debug!(
            ?last_checkpoint_seq,
            "Received {} checkpoint user signatures from consensus",
            signatures.len()
        );

        let mut end_of_epoch_observation_keys: Option<Vec<_>> = if details.last_of_epoch {
            Some(
                transactions
                    .iter()
                    .flat_map(|tx| {
                        if let TransactionKind::ProgrammableTransaction(ptb) =
                            tx.transaction_data().kind()
                        {
                            itertools::Either::Left(
                                ptb.commands
                                    .iter()
                                    .map(ExecutionTimeObservationKey::from_command),
                            )
                        } else {
                            itertools::Either::Right(std::iter::empty())
                        }
                    })
                    .collect(),
            )
        } else {
            None
        };

        let chunks = self.split_checkpoint_chunks(all_effects_and_transaction_sizes, signatures)?;
        let chunks_count = chunks.len();

        let mut checkpoints = Vec::with_capacity(chunks_count);
        debug!(
            ?last_checkpoint_seq,
            "Creating {} checkpoints with {} transactions", chunks_count, total,
        );

        let epoch = self.epoch_store.epoch();
        for (index, transactions) in chunks.into_iter().enumerate() {
            let first_checkpoint_of_epoch = index == 0
                && last_checkpoint
                    .as_ref()
                    .map(|(_, c)| c.epoch != epoch)
                    .unwrap_or(true);
            if first_checkpoint_of_epoch {
                self.epoch_store
                    .record_epoch_first_checkpoint_creation_time_metric();
            }
            let last_checkpoint_of_epoch = details.last_of_epoch && index == chunks_count - 1;

            let sequence_number = last_checkpoint
                .as_ref()
                .map(|(_, c)| c.sequence_number + 1)
                .unwrap_or_default();
            let timestamp_ms = details.timestamp_ms;
            if let Some((_, last_checkpoint)) = &last_checkpoint {
                if last_checkpoint.timestamp_ms > timestamp_ms {
                    error!("Unexpected decrease of checkpoint timestamp, sequence: {}, previous: {}, current: {}",
                    sequence_number,  last_checkpoint.timestamp_ms, timestamp_ms);
                }
            }

            let (mut effects, mut signatures): (Vec<_>, Vec<_>) = transactions.into_iter().unzip();
            let epoch_rolling_gas_cost_summary =
                self.get_epoch_total_gas_cost(last_checkpoint.as_ref().map(|(_, c)| c), &effects);

            let end_of_epoch_data = if last_checkpoint_of_epoch {
                let system_state_obj = self
                    .augment_epoch_last_checkpoint(
                        &epoch_rolling_gas_cost_summary,
                        timestamp_ms,
                        &mut effects,
                        &mut signatures,
                        sequence_number,
                        std::mem::take(&mut end_of_epoch_observation_keys).expect("end_of_epoch_observation_keys must be populated for the last checkpoint"),
                        last_checkpoint_seq.unwrap_or_default(),
                    )
                    .await?;

                let committee = system_state_obj
                    .get_current_epoch_committee()
                    .committee()
                    .clone();

                // This must happen after the call to augment_epoch_last_checkpoint,
                // otherwise we will not capture the change_epoch tx.
                let root_state_digest = {
                    let state_acc = self
                        .accumulator
                        .upgrade()
                        .expect("No checkpoints should be getting built after local configuration");
                    let acc = state_acc.accumulate_checkpoint(
                        &effects,
                        sequence_number,
                        &self.epoch_store,
                    )?;

                    state_acc
                        .wait_for_previous_running_root(&self.epoch_store, sequence_number)
                        .await?;

                    state_acc.accumulate_running_root(
                        &self.epoch_store,
                        sequence_number,
                        Some(acc),
                    )?;
                    state_acc
                        .digest_epoch(self.epoch_store.clone(), sequence_number)
                        .await?
                };
                self.metrics.highest_accumulated_epoch.set(epoch as i64);
                info!("Epoch {epoch} root state hash digest: {root_state_digest:?}");

                let epoch_commitments = if self
                    .epoch_store
                    .protocol_config()
                    .check_commit_root_state_digest_supported()
                {
                    vec![root_state_digest.into()]
                } else {
                    vec![]
                };

                Some(EndOfEpochData {
                    next_epoch_committee: committee.voting_rights,
                    next_epoch_protocol_version: ProtocolVersion::new(
                        system_state_obj.protocol_version(),
                    ),
                    epoch_commitments,
                })
            } else {
                self.send_to_accumulator
                    .send((sequence_number, effects.clone()))
                    .await?;

                None
            };
            let contents = CheckpointContents::new_with_digests_and_signatures(
                effects.iter().map(TransactionEffects::execution_digests),
                signatures,
            );

            let num_txns = contents.size() as u64;

            let network_total_transactions = last_checkpoint
                .as_ref()
                .map(|(_, c)| c.network_total_transactions + num_txns)
                .unwrap_or(num_txns);

            let previous_digest = last_checkpoint.as_ref().map(|(_, c)| c.digest());

            let matching_randomness_rounds: Vec<_> = effects
                .iter()
                .filter_map(|e| randomness_rounds.get(e.transaction_digest()))
                .copied()
                .collect();

            let summary = CheckpointSummary::new(
                self.epoch_store.protocol_config(),
                epoch,
                sequence_number,
                network_total_transactions,
                &contents,
                previous_digest,
                epoch_rolling_gas_cost_summary,
                end_of_epoch_data,
                timestamp_ms,
                matching_randomness_rounds,
            );
            summary.report_checkpoint_age(
                &self.metrics.last_created_checkpoint_age,
                &self.metrics.last_created_checkpoint_age_ms,
            );
            if last_checkpoint_of_epoch {
                info!(
                    checkpoint_seq = sequence_number,
                    "creating last checkpoint of epoch {}", epoch
                );
                if let Some(stats) = self.store.get_epoch_stats(epoch, &summary) {
                    self.epoch_store
                        .report_epoch_metrics_at_last_checkpoint(stats);
                }
            }
            last_checkpoint = Some((sequence_number, summary.clone()));
            checkpoints.push((summary, contents));
        }

        Ok(NonEmpty::from_vec(checkpoints).expect("at least one checkpoint"))
    }

    fn get_epoch_total_gas_cost(
        &self,
        last_checkpoint: Option<&CheckpointSummary>,
        cur_checkpoint_effects: &[TransactionEffects],
    ) -> GasCostSummary {
        let (previous_epoch, previous_gas_costs) = last_checkpoint
            .map(|c| (c.epoch, c.epoch_rolling_gas_cost_summary.clone()))
            .unwrap_or_default();
        let current_gas_costs = GasCostSummary::new_from_txn_effects(cur_checkpoint_effects.iter());
        if previous_epoch == self.epoch_store.epoch() {
            // sum only when we are within the same epoch
            GasCostSummary::new(
                previous_gas_costs.computation_cost + current_gas_costs.computation_cost,
                previous_gas_costs.storage_cost + current_gas_costs.storage_cost,
                previous_gas_costs.storage_rebate + current_gas_costs.storage_rebate,
                previous_gas_costs.non_refundable_storage_fee
                    + current_gas_costs.non_refundable_storage_fee,
            )
        } else {
            current_gas_costs
        }
    }

    #[instrument(level = "error", skip_all)]
    async fn augment_epoch_last_checkpoint(
        &self,
        epoch_total_gas_cost: &GasCostSummary,
        epoch_start_timestamp_ms: CheckpointTimestamp,
        checkpoint_effects: &mut Vec<TransactionEffects>,
        signatures: &mut Vec<Vec<GenericSignature>>,
        checkpoint: CheckpointSequenceNumber,
        end_of_epoch_observation_keys: Vec<ExecutionTimeObservationKey>,
        // This may be less than `checkpoint - 1` if the end-of-epoch PendingCheckpoint produced
        // >1 checkpoint.
        last_checkpoint: CheckpointSequenceNumber,
        // TODO: Check whether we must use anyhow::Result or can we use SuiResult.
    ) -> anyhow::Result<SuiSystemState> {
        let (system_state, effects) = self
            .state
            .create_and_execute_advance_epoch_tx(
                &self.epoch_store,
                epoch_total_gas_cost,
                checkpoint,
                epoch_start_timestamp_ms,
                end_of_epoch_observation_keys,
                last_checkpoint,
            )
            .await?;
        checkpoint_effects.push(effects);
        signatures.push(vec![]);
        Ok(system_state)
    }

    /// For the given roots return complete list of effects to include in checkpoint
    /// This list includes the roots and all their dependencies, which are not part of checkpoint already.
    /// Note that this function may be called multiple times to construct the checkpoint.
    /// `existing_tx_digests_in_checkpoint` is used to track the transactions that are already included in the checkpoint.
    /// Txs in `roots` that need to be included in the checkpoint will be added to `existing_tx_digests_in_checkpoint`
    /// after the call of this function.
    #[instrument(level = "debug", skip_all)]
    fn complete_checkpoint_effects(
        &self,
        mut roots: Vec<TransactionEffects>,
        existing_tx_digests_in_checkpoint: &mut BTreeSet<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffects>> {
        let _scope = monitored_scope("CheckpointBuilder::complete_checkpoint_effects");
        let mut results = vec![];
        let mut seen = HashSet::new();
        loop {
            let mut pending = HashSet::new();

            let transactions_included = self
                .epoch_store
                .builder_included_transactions_in_checkpoint(
                    roots.iter().map(|e| e.transaction_digest()),
                )?;

            for (effect, tx_included) in roots.into_iter().zip(transactions_included.into_iter()) {
                let digest = effect.transaction_digest();
                // Unnecessary to read effects of a dependency if the effect is already processed.
                seen.insert(*digest);

                // Skip roots that are already included in the checkpoint.
                if existing_tx_digests_in_checkpoint.contains(effect.transaction_digest()) {
                    continue;
                }

                // Skip roots already included in checkpoints or roots from previous epochs
                if tx_included || effect.executed_epoch() < self.epoch_store.epoch() {
                    continue;
                }

                let existing_effects = self
                    .epoch_store
                    .transactions_executed_in_cur_epoch(effect.dependencies())?;

                for (dependency, effects_signature_exists) in
                    effect.dependencies().iter().zip(existing_effects.iter())
                {
                    // Skip here if dependency not executed in the current epoch.
                    // Note that the existence of an effects signature in the
                    // epoch store for the given digest indicates that the transaction
                    // was locally executed in the current epoch
                    if !effects_signature_exists {
                        continue;
                    }
                    if seen.insert(*dependency) {
                        pending.insert(*dependency);
                    }
                }
                results.push(effect);
            }
            if pending.is_empty() {
                break;
            }
            let pending = pending.into_iter().collect::<Vec<_>>();
            let effects = self.effects_store.multi_get_executed_effects(&pending);
            let effects = effects
                .into_iter()
                .zip(pending)
                .map(|(opt, digest)| match opt {
                    Some(x) => x,
                    None => panic!(
                        "Can not find effect for transaction {:?}, however transaction that depend on it was already executed",
                        digest
                    ),
                })
                .collect::<Vec<_>>();
            roots = effects;
        }

        existing_tx_digests_in_checkpoint.extend(results.iter().map(|e| e.transaction_digest()));
        Ok(results)
    }

    // This function is used to check the invariants of the consensus commit prologue transactions in the checkpoint
    // in simtest.
    #[cfg(msim)]
    fn expensive_consensus_commit_prologue_invariants_check(
        &self,
        root_digests: &[TransactionDigest],
        sorted: &[TransactionEffects],
    ) {
        if !self
            .epoch_store
            .protocol_config()
            .prepend_prologue_tx_in_consensus_commit_in_checkpoints()
        {
            return;
        }

        // Gets all the consensus commit prologue transactions from the roots.
        let root_txs = self
            .state
            .get_transaction_cache_reader()
            .multi_get_transaction_blocks(root_digests);
        let ccps = root_txs
            .iter()
            .filter_map(|tx| {
                if let Some(tx) = tx {
                    if tx.transaction_data().is_consensus_commit_prologue() {
                        Some(tx)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // There should be at most one consensus commit prologue transaction in the roots.
        assert!(ccps.len() <= 1);

        // Get all the transactions in the checkpoint.
        let txs = self
            .state
            .get_transaction_cache_reader()
            .multi_get_transaction_blocks(
                &sorted
                    .iter()
                    .map(|tx| tx.transaction_digest().clone())
                    .collect::<Vec<_>>(),
            );

        if ccps.len() == 0 {
            // If there is no consensus commit prologue transaction in the roots, then there should be no
            // consensus commit prologue transaction in the checkpoint.
            for tx in txs.iter() {
                if let Some(tx) = tx {
                    assert!(!tx.transaction_data().is_consensus_commit_prologue());
                }
            }
        } else {
            // If there is one consensus commit prologue, it must be the first one in the checkpoint.
            assert!(txs[0]
                .as_ref()
                .unwrap()
                .transaction_data()
                .is_consensus_commit_prologue());

            assert_eq!(ccps[0].digest(), txs[0].as_ref().unwrap().digest());

            for tx in txs.iter().skip(1) {
                if let Some(tx) = tx {
                    assert!(!tx.transaction_data().is_consensus_commit_prologue());
                }
            }
        }
    }
}

impl CheckpointAggregator {
    fn new(
        tables: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        notify: Arc<Notify>,
        output: Box<dyn CertifiedCheckpointOutput>,
        state: Arc<AuthorityState>,
        metrics: Arc<CheckpointMetrics>,
    ) -> Self {
        let current = None;
        Self {
            store: tables,
            epoch_store,
            notify,
            current,
            output,
            state,
            metrics,
        }
    }

    async fn run(mut self) {
        info!("Starting CheckpointAggregator");
        loop {
            if let Err(e) = self.run_and_notify().await {
                error!(
                    "Error while aggregating checkpoint, will retry in 1s: {:?}",
                    e
                );
                self.metrics.checkpoint_errors.inc();
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            let _ = timeout(Duration::from_secs(1), self.notify.notified()).await;
        }
    }

    async fn run_and_notify(&mut self) -> SuiResult {
        let summaries = self.run_inner()?;
        for summary in summaries {
            self.output.certified_checkpoint_created(&summary).await?;
        }
        Ok(())
    }

    fn run_inner(&mut self) -> SuiResult<Vec<CertifiedCheckpointSummary>> {
        let _scope = monitored_scope("CheckpointAggregator");
        let mut result = vec![];
        'outer: loop {
            let next_to_certify = self.next_checkpoint_to_certify()?;
            let current = if let Some(current) = &mut self.current {
                // It's possible that the checkpoint was already certified by
                // the rest of the network and we've already received the
                // certified checkpoint via StateSync. In this case, we reset
                // the current signature aggregator to the next checkpoint to
                // be certified
                if current.summary.sequence_number < next_to_certify {
                    assert_reachable!("skip checkpoint certification");
                    self.current = None;
                    continue;
                }
                current
            } else {
                let Some(summary) = self
                    .epoch_store
                    .get_built_checkpoint_summary(next_to_certify)?
                else {
                    return Ok(result);
                };
                self.current = Some(CheckpointSignatureAggregator {
                    next_index: 0,
                    digest: summary.digest(),
                    summary,
                    signatures_by_digest: MultiStakeAggregator::new(
                        self.epoch_store.committee().clone(),
                    ),
                    store: self.store.clone(),
                    state: self.state.clone(),
                    metrics: self.metrics.clone(),
                });
                self.current.as_mut().unwrap()
            };

            let epoch_tables = self
                .epoch_store
                .tables()
                .expect("should not run past end of epoch");
            let iter = epoch_tables
                .pending_checkpoint_signatures
                .safe_iter_with_bounds(
                    Some((current.summary.sequence_number, current.next_index)),
                    None,
                );
            for item in iter {
                let ((seq, index), data) = item?;
                if seq != current.summary.sequence_number {
                    trace!(
                        checkpoint_seq =? current.summary.sequence_number,
                        "Not enough checkpoint signatures",
                    );
                    // No more signatures (yet) for this checkpoint
                    return Ok(result);
                }
                trace!(
                    checkpoint_seq = current.summary.sequence_number,
                    "Processing signature for checkpoint (digest: {:?}) from {:?}",
                    current.summary.digest(),
                    data.summary.auth_sig().authority.concise()
                );
                self.metrics
                    .checkpoint_participation
                    .with_label_values(&[&format!(
                        "{:?}",
                        data.summary.auth_sig().authority.concise()
                    )])
                    .inc();
                if let Ok(auth_signature) = current.try_aggregate(data) {
                    debug!(
                        checkpoint_seq = current.summary.sequence_number,
                        "Successfully aggregated signatures for checkpoint (digest: {:?})",
                        current.summary.digest(),
                    );
                    let summary = VerifiedCheckpoint::new_unchecked(
                        CertifiedCheckpointSummary::new_from_data_and_sig(
                            current.summary.clone(),
                            auth_signature,
                        ),
                    );

                    self.store.insert_certified_checkpoint(&summary)?;
                    self.metrics
                        .last_certified_checkpoint
                        .set(current.summary.sequence_number as i64);
                    current.summary.report_checkpoint_age(
                        &self.metrics.last_certified_checkpoint_age,
                        &self.metrics.last_certified_checkpoint_age_ms,
                    );
                    result.push(summary.into_inner());
                    self.current = None;
                    continue 'outer;
                } else {
                    current.next_index = index + 1;
                }
            }
            break;
        }
        Ok(result)
    }

    fn next_checkpoint_to_certify(&self) -> SuiResult<CheckpointSequenceNumber> {
        Ok(self
            .store
            .tables
            .certified_checkpoints
            .reversed_safe_iter_with_bounds(None, None)?
            .next()
            .transpose()?
            .map(|(seq, _)| seq + 1)
            .unwrap_or_default())
    }
}

impl CheckpointSignatureAggregator {
    #[allow(clippy::result_unit_err)]
    pub fn try_aggregate(
        &mut self,
        data: CheckpointSignatureMessage,
    ) -> Result<AuthorityStrongQuorumSignInfo, ()> {
        let their_digest = *data.summary.digest();
        let (_, signature) = data.summary.into_data_and_sig();
        let author = signature.authority;
        let envelope =
            SignedCheckpointSummary::new_from_data_and_sig(self.summary.clone(), signature);
        match self.signatures_by_digest.insert(their_digest, envelope) {
            // ignore repeated signatures
            InsertResult::Failed {
                error:
                    SuiError::StakeAggregatorRepeatedSigner {
                        conflicting_sig: false,
                        ..
                    },
            } => Err(()),
            InsertResult::Failed { error } => {
                warn!(
                    checkpoint_seq = self.summary.sequence_number,
                    "Failed to aggregate new signature from validator {:?}: {:?}",
                    author.concise(),
                    error
                );
                self.check_for_split_brain();
                Err(())
            }
            InsertResult::QuorumReached(cert) => {
                // It is not guaranteed that signature.authority == narwhal_cert.author, but we do verify
                // the signature so we know that the author signed the message at some point.
                if their_digest != self.digest {
                    self.metrics.remote_checkpoint_forks.inc();
                    warn!(
                        checkpoint_seq = self.summary.sequence_number,
                        "Validator {:?} has mismatching checkpoint digest {}, we have digest {}",
                        author.concise(),
                        their_digest,
                        self.digest
                    );
                    return Err(());
                }
                Ok(cert)
            }
            InsertResult::NotEnoughVotes {
                bad_votes: _,
                bad_authorities: _,
            } => {
                self.check_for_split_brain();
                Err(())
            }
        }
    }

    /// Check if there is a split brain condition in checkpoint signature aggregation, defined
    /// as any state wherein it is no longer possible to achieve quorum on a checkpoint proposal,
    /// irrespective of the outcome of any outstanding votes.
    fn check_for_split_brain(&self) {
        debug!(
            checkpoint_seq = self.summary.sequence_number,
            "Checking for split brain condition"
        );
        if self.signatures_by_digest.quorum_unreachable() {
            // TODO: at this point we should immediately halt processing
            // of new transaction certificates to avoid building on top of
            // forked output
            // self.halt_all_execution();

            let digests_by_stake_messages = self
                .signatures_by_digest
                .get_all_unique_values()
                .into_iter()
                .sorted_by_key(|(_, (_, stake))| -(*stake as i64))
                .map(|(digest, (_authorities, total_stake))| {
                    format!("{:?} (total stake: {})", digest, total_stake)
                })
                .collect::<Vec<String>>();
            error!(
                checkpoint_seq = self.summary.sequence_number,
                "Split brain detected in checkpoint signature aggregation! Remaining stake: {:?}, Digests by stake: {:?}",
                self.signatures_by_digest.uncommitted_stake(),
                digests_by_stake_messages,
            );
            self.metrics.split_brain_checkpoint_forks.inc();

            let all_unique_values = self.signatures_by_digest.get_all_unique_values();
            let local_summary = self.summary.clone();
            let state = self.state.clone();
            let tables = self.store.clone();

            tokio::spawn(async move {
                diagnose_split_brain(all_unique_values, local_summary, state, tables).await;
            });
        }
    }
}

/// Create data dump containing relevant data for diagnosing cause of the
/// split brain by querying one disagreeing validator for full checkpoint contents.
/// To minimize peer chatter, we only query one validator at random from each
/// disagreeing faction, as all honest validators that participated in this round may
/// inevitably run the same process.
async fn diagnose_split_brain(
    all_unique_values: BTreeMap<CheckpointDigest, (Vec<AuthorityName>, StakeUnit)>,
    local_summary: CheckpointSummary,
    state: Arc<AuthorityState>,
    tables: Arc<CheckpointStore>,
) {
    debug!(
        checkpoint_seq = local_summary.sequence_number,
        "Running split brain diagnostics..."
    );
    let time = SystemTime::now();
    // collect one random disagreeing validator per differing digest
    let digest_to_validator = all_unique_values
        .iter()
        .filter_map(|(digest, (validators, _))| {
            if *digest != local_summary.digest() {
                let random_validator = validators.choose(&mut get_rng()).unwrap();
                Some((*digest, *random_validator))
            } else {
                None
            }
        })
        .collect::<HashMap<_, _>>();
    if digest_to_validator.is_empty() {
        panic!(
            "Given split brain condition, there should be at \
                least one validator that disagrees with local signature"
        );
    }

    let epoch_store = state.load_epoch_store_one_call_per_task();
    let committee = epoch_store
        .epoch_start_state()
        .get_sui_committee_with_network_metadata();
    let network_config = default_mysten_network_config();
    let network_clients =
        make_network_authority_clients_with_network_config(&committee, &network_config);

    // Query all disagreeing validators
    let response_futures = digest_to_validator
        .values()
        .cloned()
        .map(|validator| {
            let client = network_clients
                .get(&validator)
                .expect("Failed to get network client");
            let request = CheckpointRequestV2 {
                sequence_number: Some(local_summary.sequence_number),
                request_content: true,
                certified: false,
            };
            client.handle_checkpoint_v2(request)
        })
        .collect::<Vec<_>>();

    let digest_name_pair = digest_to_validator.iter();
    let response_data = futures::future::join_all(response_futures)
        .await
        .into_iter()
        .zip(digest_name_pair)
        .filter_map(|(response, (digest, name))| match response {
            Ok(response) => match response {
                CheckpointResponseV2 {
                    checkpoint: Some(CheckpointSummaryResponse::Pending(summary)),
                    contents: Some(contents),
                } => Some((*name, *digest, summary, contents)),
                CheckpointResponseV2 {
                    checkpoint: Some(CheckpointSummaryResponse::Certified(_)),
                    contents: _,
                } => {
                    panic!("Expected pending checkpoint, but got certified checkpoint");
                }
                CheckpointResponseV2 {
                    checkpoint: None,
                    contents: _,
                } => {
                    error!(
                        "Summary for checkpoint {:?} not found on validator {:?}",
                        local_summary.sequence_number, name
                    );
                    None
                }
                CheckpointResponseV2 {
                    checkpoint: _,
                    contents: None,
                } => {
                    error!(
                        "Contents for checkpoint {:?} not found on validator {:?}",
                        local_summary.sequence_number, name
                    );
                    None
                }
            },
            Err(e) => {
                error!(
                    "Failed to get checkpoint contents from validator for fork diagnostics: {:?}",
                    e
                );
                None
            }
        })
        .collect::<Vec<_>>();

    let local_checkpoint_contents = tables
        .get_checkpoint_contents(&local_summary.content_digest)
        .unwrap_or_else(|_| {
            panic!(
                "Could not find checkpoint contents for digest {:?}",
                local_summary.digest()
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "Could not find local full checkpoint contents for checkpoint {:?}, digest {:?}",
                local_summary.sequence_number,
                local_summary.digest()
            )
        });
    let local_contents_text = format!("{local_checkpoint_contents:?}");

    let local_summary_text = format!("{local_summary:?}");
    let local_validator = state.name.concise();
    let diff_patches = response_data
        .iter()
        .map(|(name, other_digest, other_summary, contents)| {
            let other_contents_text = format!("{contents:?}");
            let other_summary_text = format!("{other_summary:?}");
            let (local_transactions, local_effects): (Vec<_>, Vec<_>) = local_checkpoint_contents
                .enumerate_transactions(&local_summary)
                .map(|(_, exec_digest)| (exec_digest.transaction, exec_digest.effects))
                .unzip();
            let (other_transactions, other_effects): (Vec<_>, Vec<_>) = contents
                .enumerate_transactions(other_summary)
                .map(|(_, exec_digest)| (exec_digest.transaction, exec_digest.effects))
                .unzip();
            let summary_patch = create_patch(&local_summary_text, &other_summary_text);
            let contents_patch = create_patch(&local_contents_text, &other_contents_text);
            let local_transactions_text = format!("{local_transactions:#?}");
            let other_transactions_text = format!("{other_transactions:#?}");
            let transactions_patch =
                create_patch(&local_transactions_text, &other_transactions_text);
            let local_effects_text = format!("{local_effects:#?}");
            let other_effects_text = format!("{other_effects:#?}");
            let effects_patch = create_patch(&local_effects_text, &other_effects_text);
            let seq_number = local_summary.sequence_number;
            let local_digest = local_summary.digest();
            let other_validator = name.concise();
            format!(
                "Checkpoint: {seq_number:?}\n\
                Local validator (original): {local_validator:?}, digest: {local_digest:?}\n\
                Other validator (modified): {other_validator:?}, digest: {other_digest:?}\n\n\
                Summary Diff: \n{summary_patch}\n\n\
                Contents Diff: \n{contents_patch}\n\n\
                Transactions Diff: \n{transactions_patch}\n\n\
                Effects Diff: \n{effects_patch}",
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n\n");

    let header = format!(
        "Checkpoint Fork Dump - Authority {local_validator:?}: \n\
        Datetime: {:?}",
        time
    );
    let fork_logs_text = format!("{header}\n\n{diff_patches}\n\n");
    let path = tempfile::tempdir()
        .expect("Failed to create tempdir")
        .into_path()
        .join(Path::new("checkpoint_fork_dump.txt"));
    let mut file = File::create(path).unwrap();
    write!(file, "{}", fork_logs_text).unwrap();
    debug!("{}", fork_logs_text);

    fail_point!("split_brain_reached");
}

pub trait CheckpointServiceNotify {
    fn notify_checkpoint_signature(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        info: &CheckpointSignatureMessage,
    ) -> SuiResult;

    fn notify_checkpoint(&self) -> SuiResult;
}

enum CheckpointServiceState {
    Unstarted(
        (
            CheckpointBuilder,
            CheckpointAggregator,
            CheckpointAccumulator,
        ),
    ),
    Started,
}

impl CheckpointServiceState {
    fn take_unstarted(
        &mut self,
    ) -> (
        CheckpointBuilder,
        CheckpointAggregator,
        CheckpointAccumulator,
    ) {
        let mut state = CheckpointServiceState::Started;
        std::mem::swap(self, &mut state);

        match state {
            CheckpointServiceState::Unstarted((builder, aggregator, accumulator)) => {
                (builder, aggregator, accumulator)
            }
            CheckpointServiceState::Started => panic!("CheckpointServiceState is already started"),
        }
    }
}

pub struct CheckpointService {
    tables: Arc<CheckpointStore>,
    notify_builder: Arc<Notify>,
    notify_aggregator: Arc<Notify>,
    last_signature_index: Mutex<u64>,
    // A notification for the current highest built sequence number.
    highest_currently_built_seq_tx: watch::Sender<CheckpointSequenceNumber>,
    // The highest sequence number that had already been built at the time CheckpointService
    // was constructed
    highest_previously_built_seq: CheckpointSequenceNumber,
    metrics: Arc<CheckpointMetrics>,
    state: Mutex<CheckpointServiceState>,
}

impl CheckpointService {
    /// Constructs a new CheckpointService in an un-started state.
    pub fn build(
        state: Arc<AuthorityState>,
        checkpoint_store: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        effects_store: Arc<dyn TransactionCacheRead>,
        accumulator: Weak<StateAccumulator>,
        checkpoint_output: Box<dyn CheckpointOutput>,
        certified_checkpoint_output: Box<dyn CertifiedCheckpointOutput>,
        metrics: Arc<CheckpointMetrics>,
        max_transactions_per_checkpoint: usize,
        max_checkpoint_size_bytes: usize,
    ) -> Arc<Self> {
        info!(
            "Starting checkpoint service with {max_transactions_per_checkpoint} max_transactions_per_checkpoint and {max_checkpoint_size_bytes} max_checkpoint_size_bytes"
        );
        let notify_builder = Arc::new(Notify::new());
        let notify_aggregator = Arc::new(Notify::new());

        // We may have built higher checkpoint numbers before restarting.
        let highest_previously_built_seq = checkpoint_store
            .get_latest_locally_computed_checkpoint()
            .expect("failed to get latest locally computed checkpoint")
            .map(|s| s.sequence_number)
            .unwrap_or(0);

        let highest_currently_built_seq =
            CheckpointBuilder::load_last_built_checkpoint_summary(&epoch_store, &checkpoint_store)
                .expect("epoch should not have ended")
                .map(|(seq, _)| seq)
                .unwrap_or(0);

        let (highest_currently_built_seq_tx, _) = watch::channel(highest_currently_built_seq);

        let aggregator = CheckpointAggregator::new(
            checkpoint_store.clone(),
            epoch_store.clone(),
            notify_aggregator.clone(),
            certified_checkpoint_output,
            state.clone(),
            metrics.clone(),
        );

        let (send_to_accumulator, receive_from_builder) = mpsc::channel(16);

        let ckpt_accumulator = CheckpointAccumulator::new(
            epoch_store.clone(),
            accumulator.clone(),
            receive_from_builder,
        );

        let builder = CheckpointBuilder::new(
            state.clone(),
            checkpoint_store.clone(),
            epoch_store.clone(),
            notify_builder.clone(),
            effects_store,
            accumulator,
            send_to_accumulator,
            checkpoint_output,
            notify_aggregator.clone(),
            highest_currently_built_seq_tx.clone(),
            metrics.clone(),
            max_transactions_per_checkpoint,
            max_checkpoint_size_bytes,
        );

        let last_signature_index = epoch_store
            .get_last_checkpoint_signature_index()
            .expect("should not cross end of epoch");
        let last_signature_index = Mutex::new(last_signature_index);

        Arc::new(Self {
            tables: checkpoint_store,
            notify_builder,
            notify_aggregator,
            last_signature_index,
            highest_currently_built_seq_tx,
            highest_previously_built_seq,
            metrics,
            state: Mutex::new(CheckpointServiceState::Unstarted((
                builder,
                aggregator,
                ckpt_accumulator,
            ))),
        })
    }

    /// Starts the CheckpointService.
    ///
    /// This function blocks until the CheckpointBuilder re-builds all checkpoints that had
    /// been built before the most recent restart. You can think of this as a WAL replay
    /// operation. Upon startup, we may have a number of consensus commits and resulting
    /// checkpoints that were built but not committed to disk. We want to reprocess the
    /// commits and rebuild the checkpoints before starting normal operation.
    pub async fn spawn(&self) -> JoinSet<()> {
        let mut tasks = JoinSet::new();

        let (builder, aggregator, accumulator) = self.state.lock().take_unstarted();
        tasks.spawn(monitored_future!(builder.run()));
        tasks.spawn(monitored_future!(aggregator.run()));
        tasks.spawn(monitored_future!(accumulator.run()));

        // If this times out, the validator may still start up. The worst that can
        // happen is that we will crash later on (due to missing transactions).
        if tokio::time::timeout(Duration::from_secs(60), self.wait_for_rebuilt_checkpoints())
            .await
            .is_err()
        {
            debug_fatal!("Timed out waiting for checkpoints to be rebuilt");
        }

        tasks
    }
}

impl CheckpointService {
    /// Waits until all checkpoints had been built before the node restarted
    /// are rebuilt.
    pub async fn wait_for_rebuilt_checkpoints(&self) {
        let highest_previously_built_seq = self.highest_previously_built_seq;
        let mut rx = self.highest_currently_built_seq_tx.subscribe();
        let mut highest_currently_built_seq = *rx.borrow_and_update();
        info!(
            "Waiting for checkpoints to be rebuilt, previously built seq: {highest_previously_built_seq}, currently built seq: {highest_currently_built_seq}"
        );
        loop {
            if highest_currently_built_seq >= highest_previously_built_seq {
                info!("Checkpoint rebuild complete");
                break;
            }
            rx.changed().await.unwrap();
            highest_currently_built_seq = *rx.borrow_and_update();
        }
    }

    #[cfg(test)]
    fn write_and_notify_checkpoint_for_testing(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        checkpoint: PendingCheckpointV2,
    ) -> SuiResult {
        use crate::authority::authority_per_epoch_store::consensus_quarantine::ConsensusCommitOutput;

        let mut output = ConsensusCommitOutput::new(0);
        epoch_store.write_pending_checkpoint(&mut output, &checkpoint)?;
        output.set_default_commit_stats_for_testing();
        epoch_store.push_consensus_output_for_tests(output);
        self.notify_checkpoint()?;
        Ok(())
    }
}

impl CheckpointServiceNotify for CheckpointService {
    fn notify_checkpoint_signature(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        info: &CheckpointSignatureMessage,
    ) -> SuiResult {
        let sequence = info.summary.sequence_number;
        let signer = info.summary.auth_sig().authority.concise();

        if let Some(highest_verified_checkpoint) = self
            .tables
            .get_highest_verified_checkpoint()?
            .map(|x| *x.sequence_number())
        {
            if sequence <= highest_verified_checkpoint {
                trace!(
                    checkpoint_seq = sequence,
                    "Ignore checkpoint signature from {} - already certified",
                    signer,
                );
                self.metrics
                    .last_ignored_checkpoint_signature_received
                    .set(sequence as i64);
                return Ok(());
            }
        }
        trace!(
            checkpoint_seq = sequence,
            "Received checkpoint signature, digest {} from {}",
            info.summary.digest(),
            signer,
        );
        self.metrics
            .last_received_checkpoint_signatures
            .with_label_values(&[&signer.to_string()])
            .set(sequence as i64);
        // While it can be tempting to make last_signature_index into AtomicU64, this won't work
        // We need to make sure we write to `pending_signatures` and trigger `notify_aggregator` without race conditions
        let mut index = self.last_signature_index.lock();
        *index += 1;
        epoch_store.insert_checkpoint_signature(sequence, *index, info)?;
        self.notify_aggregator.notify_one();
        Ok(())
    }

    fn notify_checkpoint(&self) -> SuiResult {
        self.notify_builder.notify_one();
        Ok(())
    }
}

// test helper
pub struct CheckpointServiceNoop {}
impl CheckpointServiceNotify for CheckpointServiceNoop {
    fn notify_checkpoint_signature(
        &self,
        _: &AuthorityPerEpochStore,
        _: &CheckpointSignatureMessage,
    ) -> SuiResult {
        Ok(())
    }

    fn notify_checkpoint(&self) -> SuiResult {
        Ok(())
    }
}

impl PendingCheckpoint {
    pub fn height(&self) -> CheckpointHeight {
        self.details.checkpoint_height
    }
}

impl PendingCheckpointV2 {}

impl From<PendingCheckpoint> for PendingCheckpointV2 {
    fn from(value: PendingCheckpoint) -> Self {
        PendingCheckpointV2::V2(PendingCheckpointV2Contents {
            roots: value
                .roots
                .into_iter()
                .map(TransactionKey::Digest)
                .collect(),
            details: value.details,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use base64::Engine;
    use futures::future::BoxFuture;
    use futures::FutureExt as _;
    use std::collections::{BTreeMap, HashMap};
    use std::ops::Deref;
    use sui_macros::sim_test;
    use sui_protocol_config::{Chain, ProtocolConfig};
    use sui_types::base_types::{ExecutionData, ObjectID, SequenceNumber, TransactionEffectsDigest};
    use sui_types::crypto::Signature;
    use sui_types::effects::{TransactionEffects, TransactionEvents};
    use sui_types::messages_checkpoint::SignedCheckpointSummary;
    use sui_types::move_package::MovePackage;
    use sui_types::object;
    use sui_types::transaction::{GenesisObject, VerifiedTransaction};
    use tokio::sync::mpsc;

    #[sim_test]
    pub async fn checkpoint_builder_test() {
        telemetry_subscribers::init_for_testing();

        let mut protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
        protocol_config.set_min_checkpoint_interval_ms_for_testing(100);
        let state = TestAuthorityBuilder::new()
            .with_protocol_config(protocol_config)
            .build()
            .await;

        let dummy_tx = VerifiedTransaction::new_genesis_transaction(vec![]);
        let dummy_tx_with_data =
            VerifiedTransaction::new_genesis_transaction(vec![GenesisObject::RawObject {
                data: object::Data::Package(
                    MovePackage::new(
                        ObjectID::random(),
                        SequenceNumber::new(),
                        BTreeMap::from([(format!("{:0>40000}", "1"), Vec::new())]),
                        100_000,
                        // no modules so empty type_origin_table as no types are defined in this package
                        Vec::new(),
                        // no modules so empty linkage_table as no dependencies of this package exist
                        BTreeMap::new(),
                    )
                    .unwrap(),
                ),
                owner: object::Owner::Immutable,
            }]);
        for i in 0..15 {
            state
                .database_for_testing()
                .perpetual_tables
                .transactions
                .insert(&d(i), dummy_tx.serializable_ref())
                .unwrap();
        }
        for i in 15..20 {
            state
                .database_for_testing()
                .perpetual_tables
                .transactions
                .insert(&d(i), dummy_tx_with_data.serializable_ref())
                .unwrap();
        }

        let mut store = HashMap::<TransactionDigest, TransactionEffects>::new();
        commit_cert_for_test(
            &mut store,
            state.clone(),
            d(1),
            vec![d(2), d(3)],
            GasCostSummary::new(11, 12, 11, 1),
        );
        commit_cert_for_test(
            &mut store,
            state.clone(),
            d(2),
            vec![d(3), d(4)],
            GasCostSummary::new(21, 22, 21, 1),
        );
        commit_cert_for_test(
            &mut store,
            state.clone(),
            d(3),
            vec![],
            GasCostSummary::new(31, 32, 31, 1),
        );
        commit_cert_for_test(
            &mut store,
            state.clone(),
            d(4),
            vec![],
            GasCostSummary::new(41, 42, 41, 1),
        );
        for i in [5, 6, 7, 10, 11, 12, 13] {
            commit_cert_for_test(
                &mut store,
                state.clone(),
                d(i),
                vec![],
                GasCostSummary::new(41, 42, 41, 1),
            );
        }
        for i in [15, 16, 17] {
            commit_cert_for_test(
                &mut store,
                state.clone(),
                d(i),
                vec![],
                GasCostSummary::new(51, 52, 51, 1),
            );
        }
        let all_digests: Vec<_> = store.keys().copied().collect();
        for digest in all_digests {
            let signature = Signature::Ed25519SuiSignature(Default::default()).into();
            state
                .epoch_store_for_testing()
                .test_insert_user_signature(digest, vec![signature]);
        }

        let (output, mut result) = mpsc::channel::<(CheckpointContents, CheckpointSummary)>(10);
        let (certified_output, mut certified_result) =
            mpsc::channel::<CertifiedCheckpointSummary>(10);
        let store = Arc::new(store);

        let ckpt_dir = tempfile::tempdir().unwrap();
        let checkpoint_store = CheckpointStore::new(ckpt_dir.path());
        let epoch_store = state.epoch_store_for_testing();

        let accumulator = Arc::new(StateAccumulator::new_for_tests(
            state.get_accumulator_store().clone(),
        ));

        let checkpoint_service = CheckpointService::build(
            state.clone(),
            checkpoint_store,
            epoch_store.clone(),
            store,
            Arc::downgrade(&accumulator),
            Box::new(output),
            Box::new(certified_output),
            CheckpointMetrics::new_for_tests(),
            3,
            100_000,
        );
        let _tasks = checkpoint_service.spawn().await;

        checkpoint_service
            .write_and_notify_checkpoint_for_testing(&epoch_store, p(0, vec![4], 0))
            .unwrap();
        checkpoint_service
            .write_and_notify_checkpoint_for_testing(&epoch_store, p(1, vec![1, 3], 2000))
            .unwrap();
        checkpoint_service
            .write_and_notify_checkpoint_for_testing(&epoch_store, p(2, vec![10, 11, 12, 13], 3000))
            .unwrap();
        checkpoint_service
            .write_and_notify_checkpoint_for_testing(&epoch_store, p(3, vec![15, 16, 17], 4000))
            .unwrap();
        checkpoint_service
            .write_and_notify_checkpoint_for_testing(&epoch_store, p(4, vec![5], 4001))
            .unwrap();
        checkpoint_service
            .write_and_notify_checkpoint_for_testing(&epoch_store, p(5, vec![6], 5000))
            .unwrap();

        let (c1c, c1s) = result.recv().await.unwrap();
        let (c2c, c2s) = result.recv().await.unwrap();

        let c1t = c1c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        let c2t = c2c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        assert_eq!(c1t, vec![d(4)]);
        assert_eq!(c1s.previous_digest, None);
        assert_eq!(c1s.sequence_number, 0);
        assert_eq!(
            c1s.epoch_rolling_gas_cost_summary,
            GasCostSummary::new(41, 42, 41, 1)
        );

        assert_eq!(c2t, vec![d(3), d(2), d(1)]);
        assert_eq!(c2s.previous_digest, Some(c1s.digest()));
        assert_eq!(c2s.sequence_number, 1);
        assert_eq!(
            c2s.epoch_rolling_gas_cost_summary,
            GasCostSummary::new(104, 108, 104, 4)
        );

        // Pending at index 2 had 4 transactions, and we configured 3 transactions max.
        // Verify that we split into 2 checkpoints.
        let (c3c, c3s) = result.recv().await.unwrap();
        let c3t = c3c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        let (c4c, c4s) = result.recv().await.unwrap();
        let c4t = c4c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        assert_eq!(c3s.sequence_number, 2);
        assert_eq!(c3s.previous_digest, Some(c2s.digest()));
        assert_eq!(c4s.sequence_number, 3);
        assert_eq!(c4s.previous_digest, Some(c3s.digest()));
        assert_eq!(c3t, vec![d(10), d(11), d(12)]);
        assert_eq!(c4t, vec![d(13)]);

        // Pending at index 3 had 3 transactions of 40K size, and we configured 100K max.
        // Verify that we split into 2 checkpoints.
        let (c5c, c5s) = result.recv().await.unwrap();
        let c5t = c5c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        let (c6c, c6s) = result.recv().await.unwrap();
        let c6t = c6c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        assert_eq!(c5s.sequence_number, 4);
        assert_eq!(c5s.previous_digest, Some(c4s.digest()));
        assert_eq!(c6s.sequence_number, 5);
        assert_eq!(c6s.previous_digest, Some(c5s.digest()));
        assert_eq!(c5t, vec![d(15), d(16)]);
        assert_eq!(c6t, vec![d(17)]);

        // Pending at index 4 was too soon after the prior one and should be coalesced into
        // the next one.
        let (c7c, c7s) = result.recv().await.unwrap();
        let c7t = c7c.iter().map(|d| d.transaction).collect::<Vec<_>>();
        assert_eq!(c7t, vec![d(5), d(6)]);
        assert_eq!(c7s.previous_digest, Some(c6s.digest()));
        assert_eq!(c7s.sequence_number, 6);

        let c1ss = SignedCheckpointSummary::new(c1s.epoch, c1s, state.secret.deref(), state.name);
        let c2ss = SignedCheckpointSummary::new(c2s.epoch, c2s, state.secret.deref(), state.name);

        checkpoint_service
            .notify_checkpoint_signature(
                &epoch_store,
                &CheckpointSignatureMessage { summary: c2ss },
            )
            .unwrap();
        checkpoint_service
            .notify_checkpoint_signature(
                &epoch_store,
                &CheckpointSignatureMessage { summary: c1ss },
            )
            .unwrap();

        let c1sc = certified_result.recv().await.unwrap();
        let c2sc = certified_result.recv().await.unwrap();
        assert_eq!(c1sc.sequence_number, 0);
        assert_eq!(c2sc.sequence_number, 1);
    }

    impl TransactionCacheRead for HashMap<TransactionDigest, TransactionEffects> {
        fn notify_read_executed_effects(
            &self,
            digests: &[TransactionDigest],
        ) -> BoxFuture<'_, Vec<TransactionEffects>> {
            std::future::ready(
                digests
                    .iter()
                    .map(|d| self.get(d).expect("effects not found").clone())
                    .collect(),
            )
            .boxed()
        }

        fn notify_read_executed_effects_digests(
            &self,
            digests: &[TransactionDigest],
        ) -> BoxFuture<'_, Vec<TransactionEffectsDigest>> {
            std::future::ready(
                digests
                    .iter()
                    .map(|d| {
                        self.get(d)
                            .map(|fx| fx.digest())
                            .expect("effects not found")
                    })
                    .collect(),
            )
            .boxed()
        }

        fn multi_get_executed_effects(
            &self,
            digests: &[TransactionDigest],
        ) -> Vec<Option<TransactionEffects>> {
            digests.iter().map(|d| self.get(d).cloned()).collect()
        }

        // Unimplemented methods - its unfortunate to have this big blob of useless code, but it wasn't
        // worth it to keep EffectsNotifyRead around just for these tests, as it caused a ton of
        // complication in non-test code. (e.g. had to implement EFfectsNotifyRead for all
        // ExecutionCacheRead implementors).

        fn multi_get_transaction_blocks(
            &self,
            _: &[TransactionDigest],
        ) -> Vec<Option<Arc<VerifiedTransaction>>> {
            unimplemented!()
        }

        fn multi_get_executed_effects_digests(
            &self,
            _: &[TransactionDigest],
        ) -> Vec<Option<TransactionEffectsDigest>> {
            unimplemented!()
        }

        fn multi_get_effects(
            &self,
            _: &[TransactionEffectsDigest],
        ) -> Vec<Option<TransactionEffects>> {
            unimplemented!()
        }

        fn multi_get_events(&self, _: &[TransactionDigest]) -> Vec<Option<TransactionEvents>> {
            unimplemented!()
        }
    }

    #[async_trait::async_trait]
    impl CheckpointOutput for mpsc::Sender<(CheckpointContents, CheckpointSummary)> {
        async fn checkpoint_created(
            &self,
            summary: &CheckpointSummary,
            contents: &CheckpointContents,
            _epoch_store: &Arc<AuthorityPerEpochStore>,
            _checkpoint_store: &Arc<CheckpointStore>,
        ) -> SuiResult {
            self.try_send((contents.clone(), summary.clone())).unwrap();
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl CertifiedCheckpointOutput for mpsc::Sender<CertifiedCheckpointSummary> {
        async fn certified_checkpoint_created(
            &self,
            summary: &CertifiedCheckpointSummary,
        ) -> SuiResult {
            self.try_send(summary.clone()).unwrap();
            Ok(())
        }
    }

    fn p(i: u64, t: Vec<u8>, timestamp_ms: u64) -> PendingCheckpointV2 {
        PendingCheckpointV2::V2(PendingCheckpointV2Contents {
            roots: t
                .into_iter()
                .map(|t| TransactionKey::Digest(d(t)))
                .collect(),
            details: PendingCheckpointInfo {
                timestamp_ms,
                last_of_epoch: false,
                checkpoint_height: i,
            },
        })
    }

    fn d(i: u8) -> TransactionDigest {
        let mut bytes: [u8; 32] = Default::default();
        bytes[0] = i;
        TransactionDigest::new(bytes)
    }

    fn e(
        transaction_digest: TransactionDigest,
        dependencies: Vec<TransactionDigest>,
        gas_used: GasCostSummary,
    ) -> TransactionEffects {
        let mut effects = TransactionEffects::default();
        *effects.transaction_digest_mut_for_testing() = transaction_digest;
        *effects.dependencies_mut_for_testing() = dependencies;
        *effects.gas_cost_summary_mut_for_testing() = gas_used;
        effects
    }

    fn commit_cert_for_test(
        store: &mut HashMap<TransactionDigest, TransactionEffects>,
        state: Arc<AuthorityState>,
        digest: TransactionDigest,
        dependencies: Vec<TransactionDigest>,
        gas_used: GasCostSummary,
    ) {
        let epoch_store = state.epoch_store_for_testing();
        let effects = e(digest, dependencies, gas_used);
        store.insert(digest, effects.clone());
        epoch_store
            .insert_tx_key(&TransactionKey::Digest(digest), &digest)
            .expect("Inserting cert fx and sigs should not fail");
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SerializedCheckpointContents {
        transactions: Vec<ExecutionData>,
    }

    #[tokio::test]
    async fn test_parse_checkpoint_contents() {
        telemetry_subscribers::init_for_testing();

        let content_base64 = "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NpzGARorDaL8/jOGEA4n1cBU+djXdxe3FTXkIcKKZI1gaxmCXsSf2xTnUqlOo/rLUumHHPypYgxy9q3ODXeVPe4CAAAAAAAAlPRuAQAAAAAAAc0HBQNMODExNTc4OTc0MTEwODA1NzA4MTEwMzk4NDI3NzYyMDgxNTYyMDQ1NzUyMzIxNDUxNjA3NDQzNTYyNjE3Mjc5NDU2MjE3MzcyNDIwN00xNzkzMzk3MjExMDA1MzY4NDQ4MjY1MTUzMzgwNjgzMzE2MzkyMDQ4MDEwNTQ1NDg5Nzg4NzE0MTUxOTA3NDEwNjU3MDk3NTg4MTk0OQExAwJNMTQyMzIzODM0MDc3NTMyNzExMTY1NDYyMjk3NDg3NTQ5OTQ5NDU0MDA3ODUxMDU3MjM1OTQ3NDUwNjM2OTk1MzYwMzIxNjczNDI0MDhMMzQzODAzNTgwODIyNDQ2MzEyNTQwNjc0MjIyODU5MzUxMTI3NDIyNzE1Mjc4ODU1NDg1OTQ3Mzk0Nzk4NzY0MDk4NDQwMDc2MDg1MwJMNTc4NTgxODk2MTE1MDM2ODgzMjczODg0ODQ1MTU2MzkzNTYwMDM2OTI5NTY4NTczODU2NTMyNzE1MDQ1NTUxMjkyMDkyNTE1ODQyNE0xODczNTA1NzU1NDkzODUyNjIyMjI4NjQ3MzE2NTk3Mzc3MDcwNzU3MTk0MjY2MjQ2NzI3MzI1NjcyNzYzMDExMTI1MzU5OTg0MjQ4NQIBMQEwA0w0MTc4OTk2MDgxMjk2Mzc3MDg4Mjg5NTY1NjMxMTMxMjY2MDc3MDk1ODE5MTI5Mjc3MzQ4OTYxNTE4NDkyMjYxMTY0MzAxODgwNTc3TTE5MzEwOTM4MTMyNDgzOTI3NzQwMzMyMTM5MTgyNDA2NDkxODY3NDgzMzg4NTM1NDAzNTc5NTc3MDc5Nzg1ODUxMjcxNDMzOTc3MjY4ATExeUpwYzNNaU9pSm9kSFJ3Y3pvdkwyRmpZMjkxYm5SekxtZHZiMmRzWlM1amIyMGlMQwFmZXlKaGJHY2lPaUpTVXpJMU5pSXNJbXRwWkNJNkltTTNaVEEwTkRZMU5qUTVabVpoTmpBMk5UVTNOalV3WXpkbE5qVm1NR0U0TjJGbE1EQm1aVGdpTENKMGVYQWlPaUpLVjFRaWZRTTE3NTc0Mzc0MDgyOTc4NzQyODkxNzEyMjA3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pAFhAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJhABpPiQKDOOUUi4DUGNFYNNDl0oZyuOR/hqjhZgc75PwLBABpvp/LNVZF+UgcBEdzr82/TJiLkAnfrZCXsB7FXQBVK6nPgyiK9qrKGC/ZRUIvEDgNX5DaC/YywoqfuZosbGEA5U2K5JR4iwybKxVyWrX/UaOKz0z5SVSW/qWfcMmG7JjoCUvsLJ24MVsdyqSuaIwJyAfieKEEoGTC7Jz9nhNGCsk5yM64y3isBQhEZLQEoEiUGCMuS6oV46wXw5rdAzK1Ac0HBQNMODExNTc4OTc0MTEwODA1NzA4MTEwMzk4NDI3NzYyMDgxNTYyMDQ1NzUyMzIxNDUxNjA3NDQzNTYyNjE3Mjc5NDU2MjE3MzcyNDIwN00xNzkzMzk3MjExMDA1MzY4NDQ4MjY1MTUzMzgwNjgzMzE2MzkyMDQ4MDEwNTQ1NDg5Nzg4NzE0MTUxOTA3NDEwNjU3MDk3NTg4MTk0OQExAwJNMTQyMzIzODM0MDc3NTMyNzExMTY1NDYyMjk3NDg3NTQ5OTQ5NDU0MDA3ODUxMDU3MjM1OTQ3NDUwNjM2OTk1MzYwMzIxNjczNDI0MDhMMzQzODAzNTgwODIyNDQ2MzEyNTQwNjc0MjIyODU5MzUxMTI3NDIyNzE1Mjc4ODU1NDg1OTQ3Mzk0Nzk4NzY0MDk4NDQwMDc2MDg1MwJMNTc4NTgxODk2MTE1MDM2ODgzMjczODg0ODQ1MTU2MzkzNTYwMDM2OTI5NTY4NTczODU2NTMyNzE1MDQ1NTUxMjkyMDkyNTE1ODQyNE0xODczNTA1NzU1NDkzODUyNjIyMjI4NjQ3MzE2NTk3Mzc3MDcwNzU3MTk0MjY2MjQ2NzI3MzI1NjcyNzYzMDExMTI1MzU5OTg0MjQ4NQIBMQEwA0w0MTc4OTk2MDgxMjk2Mzc3MDg4Mjg5NTY1NjMxMTMxMjY2MDc3MDk1ODE5MTI5Mjc3MzQ4OTYxNTE4NDkyMjYxMTY0MzAxODgwNTc3TTE5MzEwOTM4MTMyNDgzOTI3NzQwMzMyMTM5MTgyNDA2NDkxODY3NDgzMzg4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";
        let content_bytes = base64::engine::general_purpose::STANDARD.decode(content_base64).unwrap();

        let serialized_content: SerializedCheckpointContents = bcs::from_bytes(&content_bytes).unwrap();
        println!("num transactions: {}", serialized_content.transactions.len());
    }
}
