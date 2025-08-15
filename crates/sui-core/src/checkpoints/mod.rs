// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod causal_order;
pub mod checkpoint_executor;
mod checkpoint_output;
mod metrics;

use crate::accumulators::AccumulatorSettlementTxBuilder;
use crate::authority::AuthorityState;
use crate::authority_client::{make_network_authority_clients_with_network_config, AuthorityAPI};
use crate::checkpoints::causal_order::CausalOrder;
use crate::checkpoints::checkpoint_output::{CertifiedCheckpointOutput, CheckpointOutput};
pub use crate::checkpoints::checkpoint_output::{
    LogCheckpointOutput, SendCheckpointToStateSync, SubmitCheckpointToConsensus,
};
pub use crate::checkpoints::metrics::CheckpointMetrics;
use crate::consensus_manager::ReplayWaiter;
use crate::execution_cache::TransactionCacheRead;
use crate::execution_scheduler::ExecutionSchedulerAPI;
use crate::global_state_hasher::GlobalStateHasher;
use crate::stake_aggregator::{InsertResult, MultiStakeAggregator};
use diffy::create_patch;
use itertools::Itertools;
use mysten_common::random::get_rng;
use mysten_common::sync::notify_read::{NotifyRead, CHECKPOINT_BUILDER_NOTIFY_READ_TASK_NAME};
use mysten_common::{assert_reachable, debug_fatal, fatal};
use mysten_metrics::{monitored_future, monitored_scope, MonitoredFutureExt};
use nonempty::NonEmpty;
use parking_lot::Mutex;
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use sui_macros::fail_point_arg;
use sui_network::default_mysten_network_config;
use sui_types::base_types::ConciseableName;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::execution::ExecutionTimeObservationKey;
use sui_types::messages_checkpoint::CheckpointCommitment;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use tokio::sync::{mpsc, watch};
use typed_store::rocks::{default_db_options, DBOptions, ReadWriteOptions};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_handler::SequencedConsensusTransactionKey;
use rand::seq::SliceRandom;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Weak;
use std::task::{Context, Poll};
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
use sui_types::transaction::{
    TransactionDataAPI, TransactionKey, TransactionKind, VerifiedTransaction,
};
use tokio::{sync::Notify, task::JoinSet, time::timeout};
use tracing::{debug, error, info, instrument, trace, warn};
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

#[derive(Clone, Debug)]
pub struct PendingCheckpointInfo {
    pub timestamp_ms: CheckpointTimestamp,
    pub last_of_epoch: bool,
    // Computed in calculate_pending_checkpoint_height() from consensus round,
    // there is no guarantee that this is increasing per checkpoint, because of checkpoint splitting.
    pub checkpoint_height: CheckpointHeight,
}

#[derive(Clone, Debug)]
pub struct PendingCheckpoint {
    pub roots: Vec<TransactionKey>,
    pub details: PendingCheckpointInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuilderCheckpointSummary {
    pub summary: CheckpointSummary,
    // Height at which this checkpoint summary was built. None for genesis checkpoint
    pub checkpoint_height: Option<CheckpointHeight>,
    pub position_in_commit: usize,
}

#[derive(DBMapUtils)]
#[cfg_attr(tidehunter, tidehunter)]
pub struct CheckpointStoreTables {
    /// Maps checkpoint contents digest to checkpoint contents
    pub(crate) checkpoint_content: DBMap<CheckpointContentsDigest, CheckpointContents>,

    /// Maps checkpoint contents digest to checkpoint sequence number
    pub(crate) checkpoint_sequence_by_contents_digest:
        DBMap<CheckpointContentsDigest, CheckpointSequenceNumber>,

    /// Stores entire checkpoint contents from state sync, indexed by sequence number, for
    /// efficient reads of full checkpoints. Entries from this table are deleted after state
    /// accumulation has completed.
    #[default_options_override_fn = "full_checkpoint_content_table_default_config"]
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

fn full_checkpoint_content_table_default_config() -> DBOptions {
    DBOptions {
        options: default_db_options().options,
        // We have seen potential data corruption issues in this table after forced shutdowns
        // so we enable value hash logging to help with debugging.
        // TODO: remove this once we have a better understanding of the root cause.
        rw_options: ReadWriteOptions::default().set_log_value_hash(true),
    }
}

impl CheckpointStoreTables {
    #[cfg(not(tidehunter))]
    pub fn new(path: &Path, metric_name: &'static str) -> Self {
        Self::open_tables_read_write(path.to_path_buf(), MetricConf::new(metric_name), None, None)
    }

    #[cfg(tidehunter)]
    pub fn new(path: &Path, metric_name: &'static str) -> Self {
        tracing::warn!("Checkpoint DB using tidehunter");
        use typed_store::tidehunter_util::{
            default_cells_per_mutex, KeySpaceConfig, KeyType, ThConfig,
        };
        const MUTEXES: usize = 4 * 1024;
        let u64_sequence_key = KeyType::prefix_uniform(6, 0);
        let override_dirty_keys_config = KeySpaceConfig::new()
            .with_max_dirty_keys(64_000)
            .with_value_cache_size(1000);
        let config_u64 =
            ThConfig::new_with_config(8, MUTEXES, u64_sequence_key, override_dirty_keys_config);
        let digest_config = ThConfig::new_with_rm_prefix(
            32,
            MUTEXES,
            KeyType::uniform(default_cells_per_mutex()),
            KeySpaceConfig::default(),
            vec![0, 0, 0, 0, 0, 0, 0, 32],
        );
        let watermarks_config = KeySpaceConfig::new()
            .with_value_cache_size(10)
            .disable_unload();
        let lru_config = KeySpaceConfig::new().with_value_cache_size(1000);
        let checkpoint_by_digest_config = digest_config.clone().with_config(lru_config.clone());
        let configs = vec![
            ("checkpoint_content", digest_config.clone()),
            (
                "checkpoint_sequence_by_contents_digest",
                digest_config.clone(),
            ),
            ("full_checkpoint_content", config_u64.clone()),
            ("certified_checkpoints", config_u64.clone()),
            ("checkpoint_by_digest", checkpoint_by_digest_config),
            ("locally_computed_checkpoints", config_u64.clone()),
            ("epoch_last_checkpoint_map", config_u64.clone()),
            (
                "watermarks",
                ThConfig::new_with_config(4, 1, KeyType::uniform(1), watermarks_config),
            ),
        ];
        Self::open_tables_read_write(
            path.to_path_buf(),
            MetricConf::new(metric_name),
            configs
                .into_iter()
                .map(|(cf, config)| (cf.to_string(), config))
                .collect(),
        )
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
    ) -> Result<Option<CheckpointSequenceNumber>, TypedStoreError> {
        self.tables
            .watermarks
            .get(&CheckpointWatermark::HighestPruned)
            .map(|watermark| watermark.map(|w| w.0))
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

            fail_point_arg!(
                "kill_checkpoint_fork_node",
                |checkpoint_overrides: std::sync::Arc<
                    std::sync::Mutex<std::collections::BTreeMap<u64, String>>,
                >| {
                    #[cfg(msim)]
                    {
                        if let Ok(mut overrides) = checkpoint_overrides.lock() {
                            overrides.insert(
                                local_checkpoint.sequence_number,
                                verified_checkpoint.digest().to_string(),
                            );
                        }
                        tracing::error!(
                        fatal = true,
                        "Fork recovery test: killing node due to checkpoint fork for sequence number: {}, using verified digest: {}",
                        local_checkpoint.sequence_number(),
                        verified_checkpoint.digest()
                    );
                        sui_simulator::task::shutdown_current_node();
                    }
                }
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
            .read("notify_read_checkpoint_watermark", &[seq], |seqs| {
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
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointWatermark {
    HighestVerified,
    HighestSynced,
    HighestExecuted,
    HighestPruned,
}

struct CheckpointStateHasher {
    epoch_store: Arc<AuthorityPerEpochStore>,
    hasher: Weak<GlobalStateHasher>,
    receive_from_builder: mpsc::Receiver<(CheckpointSequenceNumber, Vec<TransactionEffects>)>,
}

impl CheckpointStateHasher {
    fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        hasher: Weak<GlobalStateHasher>,
        receive_from_builder: mpsc::Receiver<(CheckpointSequenceNumber, Vec<TransactionEffects>)>,
    ) -> Self {
        Self {
            epoch_store,
            hasher,
            receive_from_builder,
        }
    }

    async fn run(self) {
        let Self {
            epoch_store,
            hasher,
            mut receive_from_builder,
        } = self;
        while let Some((seq, effects)) = receive_from_builder.recv().await {
            let Some(hasher) = hasher.upgrade() else {
                info!("Object state hasher was dropped, stopping checkpoint accumulation");
                break;
            };
            hasher
                .accumulate_checkpoint(&effects, seq, &epoch_store)
                .expect("epoch ended while accumulating checkpoint");
        }
    }
}

#[derive(Debug)]
pub(crate) enum CheckpointBuilderError {
    ChangeEpochTxAlreadyExecuted,
    SystemPackagesMissing,
    Retry(anyhow::Error),
}

impl<SuiError: std::error::Error + Send + Sync + 'static> From<SuiError>
    for CheckpointBuilderError
{
    fn from(e: SuiError) -> Self {
        Self::Retry(e.into())
    }
}

pub(crate) type CheckpointBuilderResult<T = ()> = Result<T, CheckpointBuilderError>;

pub struct CheckpointBuilder {
    state: Arc<AuthorityState>,
    store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    notify: Arc<Notify>,
    notify_aggregator: Arc<Notify>,
    last_built: watch::Sender<CheckpointSequenceNumber>,
    effects_store: Arc<dyn TransactionCacheRead>,
    global_state_hasher: Weak<GlobalStateHasher>,
    send_to_hasher: mpsc::Sender<(CheckpointSequenceNumber, Vec<TransactionEffects>)>,
    output: Box<dyn CheckpointOutput>,
    metrics: Arc<CheckpointMetrics>,
    max_transactions_per_checkpoint: usize,
    max_checkpoint_size_bytes: usize,
    rebuilt_checkpoint_digests: std::collections::HashSet<CheckpointDigest>,
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
        global_state_hasher: Weak<GlobalStateHasher>,
        // for asynchronous/concurrent accumulation of regular checkpoints
        send_to_hasher: mpsc::Sender<(CheckpointSequenceNumber, Vec<TransactionEffects>)>,
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
            global_state_hasher,
            send_to_hasher,
            output,
            notify_aggregator,
            last_built,
            metrics,
            max_transactions_per_checkpoint,
            max_checkpoint_size_bytes,
            rebuilt_checkpoint_digests: std::collections::HashSet::new(),
        }
    }

    /// This function first waits for ConsensusCommitHandler to finish reprocessing
    /// commits that have been processed before the last restart, if consensus_replay_waiter
    /// is supplied. Then it starts building checkpoints in a loop.
    ///
    /// It is optional to pass in consensus_replay_waiter, to make it easier to attribute
    /// if slow recovery of previously built checkpoints is due to consensus replay or
    /// checkpoint building.
    async fn run(mut self, consensus_replay_waiter: Option<ReplayWaiter>) {
        if let Some(replay_waiter) = consensus_replay_waiter {
            info!("Waiting for consensus commits to replay ...");
            replay_waiter.wait_for_replay().await;
            info!("Consensus commits finished replaying");
        }
        info!("Starting CheckpointBuilder");
        loop {
            match self.maybe_build_checkpoints().await {
                Ok(()) => {}
                err @ Err(
                    CheckpointBuilderError::ChangeEpochTxAlreadyExecuted
                    | CheckpointBuilderError::SystemPackagesMissing,
                ) => {
                    info!("CheckpointBuilder stopping: {:?}", err);
                    return;
                }
                Err(CheckpointBuilderError::Retry(inner)) => {
                    let msg = format!("{:?}", inner);
                    debug_fatal!("Error while making checkpoint, will retry in 1s: {}", msg);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    self.metrics.checkpoint_errors.inc();
                    continue;
                }
            }

            self.notify.notified().await;
        }
    }

    async fn maybe_build_checkpoints(&mut self) -> CheckpointBuilderResult {
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
                checkpoint_commit_height_from = grouped_pending_checkpoints
                    .first()
                    .unwrap()
                    .details()
                    .checkpoint_height,
                checkpoint_commit_height_to = last_height,
                "Making checkpoint with commit height range"
            );

            let seq = self
                .make_checkpoint(std::mem::take(&mut grouped_pending_checkpoints))
                .await?;

            self.last_built.send_if_modified(|cur| {
                // when rebuilding checkpoints at startup, seq can be for an old checkpoint
                if seq > *cur {
                    *cur = seq;
                    true
                } else {
                    false
                }
            });

            // ensure that the task can be cancelled at end of epoch, even if no other await yields
            // execution.
            tokio::task::yield_now().await;
        }
        debug!(
            "Waiting for more checkpoints from consensus after processing {last_height:?}; {} pending checkpoints left unprocessed until next interval",
            grouped_pending_checkpoints.len(),
        );

        Ok(())
    }

    #[instrument(level = "debug", skip_all, fields(last_height = pendings.last().unwrap().details().checkpoint_height))]
    async fn make_checkpoint(
        &mut self,
        pendings: Vec<PendingCheckpoint>,
    ) -> CheckpointBuilderResult<CheckpointSequenceNumber> {
        let _scope = monitored_scope("CheckpointBuilder::make_checkpoint");
        let last_details = pendings.last().unwrap().details().clone();

        // Stores the transactions that should be included in the checkpoint. Transactions will be recorded in the checkpoint
        // in this order.
        let highest_executed_sequence = self
            .store
            .get_highest_executed_checkpoint_seq_number()
            .expect("db error")
            .unwrap_or(0);

        let (poll_count, result) = poll_count(self.resolve_checkpoint_transactions(pendings)).await;
        let (sorted_tx_effects_included_in_checkpoint, all_roots) = result?;

        let new_checkpoints = self
            .create_checkpoints(
                sorted_tx_effects_included_in_checkpoint,
                &last_details,
                &all_roots,
            )
            .await?;
        let highest_sequence = *new_checkpoints.last().0.sequence_number();
        if highest_sequence <= highest_executed_sequence && poll_count > 1 {
            debug_fatal!(
                "resolve_checkpoint_transactions should be instantaneous when executed checkpoint is ahead of checkpoint builder"
            );
        }

        self.write_checkpoints(last_details.checkpoint_height, new_checkpoints)
            .await?;
        Ok(highest_sequence)
    }

    async fn construct_and_execute_settlement_transactions(
        &self,
        sorted_tx_effects_included_in_checkpoint: &[TransactionEffects],
        checkpoint_height: CheckpointHeight,
    ) -> (TransactionKey, Vec<TransactionEffects>) {
        let _scope =
            monitored_scope("CheckpointBuilder::construct_and_execute_settlement_transactions");

        let tx_key =
            TransactionKey::AccumulatorSettlement(self.epoch_store.epoch(), checkpoint_height);

        let builder = AccumulatorSettlementTxBuilder::new(
            Some(self.effects_store.as_ref()),
            sorted_tx_effects_included_in_checkpoint,
        );

        let settlements = builder.get_balance_settlements();
        let num_updates = builder.num_updates();
        let settlement_txns = builder.build_tx(&self.epoch_store, checkpoint_height);

        let settlement_txns: Vec<_> = settlement_txns
            .into_iter()
            .map(|tx| {
                VerifiedExecutableTransaction::new_system(
                    VerifiedTransaction::new_system_transaction(tx),
                    self.epoch_store.epoch(),
                )
            })
            .collect();

        let settlement_digests: Vec<_> = settlement_txns.iter().map(|tx| *tx.digest()).collect();

        debug!(
            ?settlement_digests,
            ?tx_key,
            "created settlement transactions with {num_updates} updates"
        );

        self.epoch_store
            .notify_settlement_transactions_ready(tx_key, settlement_txns);

        let settlement_effects = loop {
            match tokio::time::timeout(Duration::from_secs(5), async {
                self.effects_store
                    .notify_read_executed_effects(
                        "CheckpointBuilder::notify_read_settlement_effects",
                        &settlement_digests,
                    )
                    .await
            })
            .await
            {
                Ok(effects) => break effects,
                Err(_) => {
                    debug_fatal!(
                        "Timeout waiting for settlement transactions to be executed {:?}, retrying...",
                        tx_key
                    );
                }
            }
        };

        for fx in settlement_effects.iter() {
            assert!(
                fx.status().is_ok(),
                "settlement transaction cannot fail (digest: {:?}) {:#?}",
                fx.transaction_digest(),
                fx
            );
        }

        self.state
            .execution_scheduler()
            .settle_balances(settlements);

        (tx_key, settlement_effects)
    }

    // Given the root transactions of a pending checkpoint, resolve the transactions should be included in
    // the checkpoint, and return them in the order they should be included in the checkpoint.
    // `effects_in_current_checkpoint` tracks the transactions that already exist in the current
    // checkpoint.
    #[instrument(level = "debug", skip_all)]
    async fn resolve_checkpoint_transactions(
        &self,
        pending_checkpoints: Vec<PendingCheckpoint>,
    ) -> SuiResult<(Vec<TransactionEffects>, HashSet<TransactionDigest>)> {
        let _scope = monitored_scope("CheckpointBuilder::resolve_checkpoint_transactions");

        // Keeps track of the effects that are already included in the current checkpoint.
        // This is used when there are multiple pending checkpoints to create a single checkpoint
        // because in such scenarios, dependencies of a transaction may in earlier created checkpoints,
        // or in earlier pending checkpoints.
        let mut effects_in_current_checkpoint = BTreeSet::new();

        let mut tx_effects = Vec::new();
        let mut tx_roots = HashSet::new();

        for pending_checkpoint in pending_checkpoints.into_iter() {
            let mut pending = pending_checkpoint;
            debug!(
                checkpoint_commit_height = pending.details.checkpoint_height,
                "Resolving checkpoint transactions for pending checkpoint.",
            );

            trace!(
                "roots for pending checkpoint {:?}: {:?}",
                pending.details.checkpoint_height,
                pending.roots,
            );

            let settlement_root = if self.epoch_store.accumulators_enabled() {
                let Some(settlement_root @ TransactionKey::AccumulatorSettlement(..)) =
                    pending.roots.pop()
                else {
                    fatal!("No settlement root found");
                };
                Some(settlement_root)
            } else {
                None
            };

            let roots = &pending.roots;

            self.metrics
                .checkpoint_roots_count
                .inc_by(roots.len() as u64);

            let root_digests = self
                .epoch_store
                .notify_read_tx_key_to_digest(roots)
                .in_monitored_scope("CheckpointNotifyDigests")
                .await?;
            let root_effects = self
                .effects_store
                .notify_read_executed_effects(
                    CHECKPOINT_BUILDER_NOTIFY_READ_TASK_NAME,
                    &root_digests,
                )
                .in_monitored_scope("CheckpointNotifyRead")
                .await;

            let consensus_commit_prologue = if self
                .epoch_store
                .protocol_config()
                .prepend_prologue_tx_in_consensus_commit_in_checkpoints()
            {
                // If the roots contains consensus commit prologue transaction, we want to extract it,
                // and put it to the front of the checkpoint.

                let consensus_commit_prologue =
                    self.extract_consensus_commit_prologue(&root_digests, &root_effects)?;

                // Get the unincluded depdnencies of the consensus commit prologue. We should expect no
                // other dependencies that haven't been included in any previous checkpoints.
                if let Some((ccp_digest, ccp_effects)) = &consensus_commit_prologue {
                    let unsorted_ccp = self.complete_checkpoint_effects(
                        vec![ccp_effects.clone()],
                        &mut effects_in_current_checkpoint,
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
                self.complete_checkpoint_effects(root_effects, &mut effects_in_current_checkpoint)?;

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

            if let Some(settlement_root) = settlement_root {
                let (tx_key, settlement_effects) = self
                    .construct_and_execute_settlement_transactions(
                        &sorted,
                        pending.details.checkpoint_height,
                    )
                    .await;
                debug!(?tx_key, "executed settlement transactions");

                assert_eq!(settlement_root, tx_key);

                // Note: we do not need to add the settlement digests to `tx_roots` - `tx_roots`
                // should only include the digests of transactions that were originally roots in
                // the pending checkpoint. It is later used to identify transactions which were
                // added as dependencies, so that those transactions can be waited on using
                // `consensus_messages_processed_notify()`. System transactions (such as
                // settlements) are exempt from this already.
                sorted.extend(settlement_effects);
            }

            #[cfg(msim)]
            {
                // Check consensus commit prologue invariants in sim test.
                self.expensive_consensus_commit_prologue_invariants_check(&root_digests, &sorted);
            }

            tx_effects.extend(sorted);
            tx_roots.extend(root_digests);
        }

        Ok((tx_effects, tx_roots))
    }

    // This function is used to extract the consensus commit prologue digest and effects from the root
    // transactions.
    // This function can only be used when prepend_prologue_tx_in_consensus_commit_in_checkpoints is enabled.
    // The consensus commit prologue is expected to be the first transaction in the roots.
    fn extract_consensus_commit_prologue(
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
        &mut self,
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
                    let override_digest =
                        if let Some(fork_recovery) = self.state.get_fork_recovery_state() {
                            fork_recovery.get_checkpoint_override(&summary.sequence_number)
                        } else {
                            None
                        };

                    // Also allow if we rebuilt the previous checkpoint of the previously computed summary
                    let rebuilt_previous = previously_computed_summary
                        .previous_digest
                        .map(|digest| {
                            info!(
                                "Fork recovery: Checking if previous digest {:?} is in rebuilt_checkpoint_digests: {:?}",
                                digest,
                                self.rebuilt_checkpoint_digests
                            );
                            self.rebuilt_checkpoint_digests.contains(&digest)
                        })
                        .unwrap_or(false);

                    if let Some(expected_digest) = override_digest {
                        if summary.digest() == expected_digest {
                            info!(
                                checkpoint_seq = summary.sequence_number,
                                "Fork recovery: Using checkpoint override, new summary matches expected digest {:?}",
                                expected_digest
                            );
                            self.rebuilt_checkpoint_digests
                                .insert(previously_computed_summary.digest());
                            info!(
                                "Fork recovery: Added digest to rebuilt_checkpoint_digests via override, contents: {:?}",
                                self.rebuilt_checkpoint_digests
                            );
                        } else {
                            fatal!(
                                "Fork recovery: Checkpoint {} override configured with digest {:?}, but newly built checkpoint has digest {:?}",
                                summary.sequence_number,
                                expected_digest,
                                summary.digest()
                            );
                        }
                    } else if rebuilt_previous {
                        info!(
                            checkpoint_seq = summary.sequence_number,
                            "Fork recovery: Allowing checkpoint rebuild because previous digest {:?} is bad",
                            previously_computed_summary.previous_digest
                        );
                        self.rebuilt_checkpoint_digests
                            .insert(previously_computed_summary.digest());
                    } else {
                        fatal!(
                            "Checkpoint {} was previously built with a different result: previously_computed_summary {:?} vs current_summary {:?}",
                            summary.sequence_number,
                            previously_computed_summary.digest(),
                            summary.digest()
                        );
                    }
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
    ) -> CheckpointBuilderResult<Vec<Vec<(TransactionEffects, Vec<GenericSignature>)>>> {
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
        all_roots: &HashSet<TransactionDigest>,
    ) -> CheckpointBuilderResult<NonEmpty<(CheckpointSummary, CheckpointContents)>> {
        let _scope = monitored_scope("CheckpointBuilder::create_checkpoints");

        let total = all_effects.len();
        let mut last_checkpoint =
            Self::load_last_built_checkpoint_summary(&self.epoch_store, &self.store)?;
        let last_checkpoint_seq = last_checkpoint.as_ref().map(|(seq, _)| *seq);
        info!(
            checkpoint_commit_height = details.checkpoint_height,
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
                    TransactionKind::ProgrammableSystemTransaction(_) => {
                        // settlement transactions are added by checkpoint builder
                    }
                    TransactionKind::ChangeEpoch(_)
                    | TransactionKind::Genesis(_)
                    | TransactionKind::EndOfEpochTransaction(_) => {
                        fatal!(
                            "unexpected transaction in checkpoint effects: {:?}",
                            transaction
                        );
                    }
                    TransactionKind::RandomnessStateUpdate(rsu) => {
                        randomness_rounds
                            .insert(*effects.transaction_digest(), rsu.randomness_round);
                    }
                    TransactionKind::ProgrammableTransaction(_) => {
                        // Only transactions that are not roots should be included in the call to
                        // `consensus_messages_processed_notify`. roots come directly from the consensus
                        // commit and so are known to be processed already.
                        let digest = *effects.transaction_digest();
                        if !all_roots.contains(&digest) {
                            transaction_keys.push(SequencedConsensusTransactionKey::External(
                                ConsensusTransactionKey::Certificate(digest),
                            ));
                        }
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
            .user_signatures_for_checkpoint(&transactions, &all_digests);
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
            let mut timestamp_ms = details.timestamp_ms;
            if let Some((_, last_checkpoint)) = &last_checkpoint {
                if last_checkpoint.timestamp_ms > timestamp_ms {
                    // First consensus commit of an epoch can have zero timestamp.
                    debug!(
                        "Decrease of checkpoint timestamp, possibly due to epoch change. Sequence: {}, previous: {}, current: {}",
                        sequence_number,  last_checkpoint.timestamp_ms, timestamp_ms,
                    );
                    if self
                        .epoch_store
                        .protocol_config()
                        .enforce_checkpoint_timestamp_monotonicity()
                    {
                        timestamp_ms = last_checkpoint.timestamp_ms;
                    }
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
                        .global_state_hasher
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
                self.send_to_hasher
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
    ) -> CheckpointBuilderResult<SuiSystemState> {
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

            let all_unique_values = self.signatures_by_digest.get_all_unique_values();
            let digests_by_stake_messages = all_unique_values
                .iter()
                .sorted_by_key(|(_, (_, stake))| -(*stake as i64))
                .map(|(digest, (_authorities, total_stake))| {
                    format!("{:?} (total stake: {})", digest, total_stake)
                })
                .collect::<Vec<String>>();
            fail_point_arg!("kill_split_brain_node", |(
                checkpoint_overrides,
                forked_authorities,
            ): (
                std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<u64, String>>>,
                std::sync::Arc<std::sync::Mutex<std::collections::HashSet<AuthorityName>>>,
            )| {
                #[cfg(msim)]
                {
                    if let (Ok(mut overrides), Ok(forked_authorities_set)) =
                        (checkpoint_overrides.lock(), forked_authorities.lock())
                    {
                        // Find the digest produced by non-forked authorities
                        let correct_digest = all_unique_values
                            .iter()
                            .find(|(_, (authorities, _))| {
                                // Check if any authority that produced this digest is NOT in the forked set
                                authorities
                                    .iter()
                                    .any(|auth| !forked_authorities_set.contains(auth))
                            })
                            .map(|(digest, _)| digest.to_string())
                            .unwrap_or_else(|| {
                                // Fallback: use the digest with the highest stake
                                all_unique_values
                                    .iter()
                                    .max_by_key(|(_, (_, stake))| *stake)
                                    .map(|(digest, _)| digest.to_string())
                                    .unwrap_or_else(|| self.digest.to_string())
                            });

                        overrides.insert(self.summary.sequence_number, correct_digest.clone());

                        tracing::error!(
                                fatal = true,
                                "Fork recovery test: detected split-brain for sequence number: {}, using digest: {}",
                                self.summary.sequence_number,
                                correct_digest
                            );
                    }
                }
            });

            debug_fatal!(
                "Split brain detected in checkpoint signature aggregation for checkpoint {:?}. Remaining stake: {:?}, Digests by stake: {:?}",
                self.summary.sequence_number,
                self.signatures_by_digest.uncommitted_stake(),
                digests_by_stake_messages
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
        .keep()
        .join(Path::new("checkpoint_fork_dump.txt"));
    let mut file = File::create(path).unwrap();
    write!(file, "{}", fork_logs_text).unwrap();
    debug!("{}", fork_logs_text);
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
            CheckpointStateHasher,
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
        CheckpointStateHasher,
    ) {
        let mut state = CheckpointServiceState::Started;
        std::mem::swap(self, &mut state);

        match state {
            CheckpointServiceState::Unstarted((builder, aggregator, hasher)) => {
                (builder, aggregator, hasher)
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
        global_state_hasher: Weak<GlobalStateHasher>,
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

        let (send_to_hasher, receive_from_builder) = mpsc::channel(16);

        let ckpt_state_hasher = CheckpointStateHasher::new(
            epoch_store.clone(),
            global_state_hasher.clone(),
            receive_from_builder,
        );

        let builder = CheckpointBuilder::new(
            state.clone(),
            checkpoint_store.clone(),
            epoch_store.clone(),
            notify_builder.clone(),
            effects_store,
            global_state_hasher,
            send_to_hasher,
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
                ckpt_state_hasher,
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
    pub async fn spawn(&self, consensus_replay_waiter: Option<ReplayWaiter>) -> JoinSet<()> {
        let mut tasks = JoinSet::new();

        let (builder, aggregator, state_hasher) = self.state.lock().take_unstarted();

        // Clean up state hashes computed after the last committed checkpoint
        // This prevents ECMH divergence after fork recovery restarts
        if let Some(last_committed_seq) = self
            .tables
            .get_highest_executed_checkpoint()
            .expect("Failed to get highest executed checkpoint")
            .map(|checkpoint| *checkpoint.sequence_number())
        {
            if let Err(e) = builder
                .epoch_store
                .clear_state_hashes_after_checkpoint(last_committed_seq)
            {
                error!(
                    "Failed to clear state hashes after checkpoint {}: {:?}",
                    last_committed_seq, e
                );
            } else {
                info!("Cleared state hashes after checkpoint {} to ensure consistent ECMH computation", last_committed_seq);
            }
        }
        tasks.spawn(monitored_future!(builder.run(consensus_replay_waiter)));
        tasks.spawn(monitored_future!(aggregator.run()));
        tasks.spawn(monitored_future!(state_hasher.run()));

        // If this times out, the validator may still start up. The worst that can
        // happen is that we will crash later on instead of immediately. The eventual
        // crash would occur because we may be missing transactions that are below the
        // highest_synced_checkpoint watermark, which can cause a crash in
        // `CheckpointExecutor::extract_randomness_rounds`.
        if tokio::time::timeout(
            Duration::from_secs(120),
            self.wait_for_rebuilt_checkpoints(),
        )
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
    /// are rebuilt. This is required to preserve the invariant that all checkpoints
    /// (and their transactions) below the highest_synced_checkpoint watermark are
    /// available. Once the checkpoints are constructed, we can be sure that the
    /// transactions have also been executed.
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
        checkpoint: PendingCheckpoint,
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

    pub fn roots(&self) -> &Vec<TransactionKey> {
        &self.roots
    }

    pub fn details(&self) -> &PendingCheckpointInfo {
        &self.details
    }
}

pin_project! {
    pub struct PollCounter<Fut> {
        #[pin]
        future: Fut,
        count: usize,
    }
}

impl<Fut> PollCounter<Fut> {
    pub fn new(future: Fut) -> Self {
        Self { future, count: 0 }
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

impl<Fut: Future> Future for PollCounter<Fut> {
    type Output = (usize, Fut::Output);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        *this.count += 1;
        match this.future.poll(cx) {
            Poll::Ready(output) => Poll::Ready((*this.count, output)),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn poll_count<Fut>(future: Fut) -> PollCounter<Fut> {
    PollCounter::new(future)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use crate::transaction_outputs::TransactionOutputs;
    use fastcrypto_zkp::bn254::zk_login::{JwkId, JWK};
    use futures::future::BoxFuture;
    use futures::FutureExt as _;
    use std::collections::HashMap;
    use std::ops::Deref;
    use sui_macros::sim_test;
    use sui_protocol_config::{Chain, ProtocolConfig};
    use sui_types::accumulator_event::AccumulatorEvent;
    use sui_types::authenticator_state::ActiveJwk;
    use sui_types::base_types::{SequenceNumber, TransactionEffectsDigest};
    use sui_types::crypto::Signature;
    use sui_types::effects::{TransactionEffects, TransactionEvents};
    use sui_types::messages_checkpoint::SignedCheckpointSummary;
    use sui_types::transaction::VerifiedTransaction;
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

        let dummy_tx = VerifiedTransaction::new_authenticator_state_update(
            0,
            0,
            vec![],
            SequenceNumber::new(),
        );

        let jwks = {
            let mut jwks = Vec::new();
            while bcs::to_bytes(&jwks).unwrap().len() < 40_000 {
                jwks.push(ActiveJwk {
                    jwk_id: JwkId::new(
                        "https://accounts.google.com".to_string(),
                        "1234567890".to_string(),
                    ),
                    jwk: JWK {
                        kty: "RSA".to_string(),
                        e: "AQAB".to_string(),
                        n: "1234567890".to_string(),
                        alg: "RS256".to_string(),
                    },
                    epoch: 0,
                });
            }
            jwks
        };

        let dummy_tx_with_data =
            VerifiedTransaction::new_authenticator_state_update(0, 1, jwks, SequenceNumber::new());

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

        let global_state_hasher = Arc::new(GlobalStateHasher::new_for_tests(
            state.get_global_state_hash_store().clone(),
        ));

        let checkpoint_service = CheckpointService::build(
            state.clone(),
            checkpoint_store,
            epoch_store.clone(),
            store,
            Arc::downgrade(&global_state_hasher),
            Box::new(output),
            Box::new(certified_output),
            CheckpointMetrics::new_for_tests(),
            3,
            100_000,
        );
        let _tasks = checkpoint_service.spawn(None).await;

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
            _: &str,
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
            _: &str,
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

        fn get_mysticeti_fastpath_outputs(
            &self,
            _: &TransactionDigest,
        ) -> Option<Arc<TransactionOutputs>> {
            unimplemented!()
        }

        fn notify_read_fastpath_transaction_outputs<'a>(
            &'a self,
            _: &'a [TransactionDigest],
        ) -> BoxFuture<'a, Vec<Arc<crate::transaction_outputs::TransactionOutputs>>> {
            unimplemented!()
        }

        fn take_accumulator_events(&self, _: &TransactionDigest) -> Option<Vec<AccumulatorEvent>> {
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

    fn p(i: u64, t: Vec<u8>, timestamp_ms: u64) -> PendingCheckpoint {
        PendingCheckpoint {
            roots: t
                .into_iter()
                .map(|t| TransactionKey::Digest(d(t)))
                .collect(),
            details: PendingCheckpointInfo {
                timestamp_ms,
                last_of_epoch: false,
                checkpoint_height: i,
            },
        }
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
        epoch_store.insert_executed_in_epoch(&digest);
    }
}
