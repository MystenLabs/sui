// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod casual_order;
pub mod checkpoint_executor;
mod checkpoint_output;
mod metrics;

use crate::authority::{AuthorityState, EffectsNotifyRead};
use crate::checkpoints::casual_order::CasualOrder;
use crate::checkpoints::checkpoint_output::{CertifiedCheckpointOutput, CheckpointOutput};
pub use crate::checkpoints::checkpoint_output::{
    LogCheckpointOutput, SendCheckpointToStateSync, SubmitCheckpointToConsensus,
};
pub use crate::checkpoints::metrics::CheckpointMetrics;
use crate::stake_aggregator::{InsertResult, StakeAggregator};
use fastcrypto::encoding::{Encoding, Hex};
use futures::future::{select, Either};
use futures::FutureExt;
use mysten_metrics::{monitored_scope, spawn_monitored_task, MonitoredFutureExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority_aggregator::TransactionCertifier;
use std::collections::HashSet;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::{EpochId, TransactionDigest};
use sui_types::crypto::{AuthoritySignInfo, AuthorityWeakQuorumSignInfo};
use sui_types::error::{SuiError, SuiResult};
use sui_types::gas::GasCostSummary;
use sui_types::messages::{TransactionEffects, VerifiedSignedTransactionEffects};
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointContentsDigest, CheckpointDigest,
    CheckpointSequenceNumber, CheckpointSignatureMessage, CheckpointSummary, CheckpointTimestamp,
    VerifiedCheckpoint,
};
use tokio::sync::{mpsc, watch, Notify};
use tokio::time::Instant;
use tracing::{debug, error, error_span, info, warn, Instrument};
use typed_store::rocks::{DBMap, MetricConf, TypedStoreError};
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store::Map;
use typed_store_derive::DBMapUtils;

pub type CheckpointCommitHeight = u64;

pub struct EpochStats {
    pub checkpoint_count: u64,
    pub transaction_count: u64,
    pub total_gas_reward: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingCheckpointInfo {
    pub timestamp_ms: CheckpointTimestamp,
    pub last_of_epoch: bool,
    pub commit_height: CheckpointCommitHeight,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingCheckpoint {
    pub roots: Vec<TransactionDigest>,
    pub details: PendingCheckpointInfo,
}

#[derive(DBMapUtils)]
pub struct CheckpointStore {
    /// Maps checkpoint contents digest to checkpoint contents
    checkpoint_content: DBMap<CheckpointContentsDigest, CheckpointContents>,

    /// Stores certified checkpoints
    certified_checkpoints: DBMap<CheckpointSequenceNumber, CertifiedCheckpointSummary>,
    /// Map from checkpoint digest to certified checkpoint
    checkpoint_by_digest: DBMap<CheckpointDigest, CertifiedCheckpointSummary>,

    /// A map from epoch ID to the sequence number of the last checkpoint in that epoch.
    epoch_last_checkpoint_map: DBMap<EpochId, CheckpointSequenceNumber>,

    /// Watermarks used to determine the highest verified, fully synced, and
    /// fully executed checkpoints
    watermarks: DBMap<CheckpointWatermark, (CheckpointSequenceNumber, CheckpointDigest)>,
}

impl CheckpointStore {
    pub fn new(path: &Path) -> Arc<Self> {
        Arc::new(Self::open_tables_read_write(
            path.to_path_buf(),
            MetricConf::default(),
            None,
            None,
        ))
    }

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
            checkpoint.sequence_number(),
            0,
            "can't call insert_genesis_checkpoint with a checkpoint that doesn't have a sequence number of 0"
        );

        // Only insert the genesis checkpoint if the DB is empty and doesn't have it already
        if self
            .get_checkpoint_by_digest(&checkpoint.digest())
            .unwrap()
            .is_none()
        {
            if epoch_store.epoch() == checkpoint.summary.epoch {
                epoch_store
                    .put_genesis_checkpoint_in_builder(&checkpoint.summary, &contents)
                    .unwrap();
            } else {
                debug!(
                    validator_epoch =% epoch_store.epoch(),
                    genesis_epoch =% checkpoint.epoch(),
                    "Not inserting checkpoint builder data for genesis checkpoint",
                );
            }
            self.insert_checkpoint_contents(contents).unwrap();
            self.insert_verified_checkpoint(checkpoint.clone()).unwrap();
            self.update_highest_synced_checkpoint(&checkpoint).unwrap();
        }
    }

    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        self.checkpoint_by_digest
            .get(digest)
            .map(|maybe_checkpoint| maybe_checkpoint.map(VerifiedCheckpoint::new_unchecked))
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        self.certified_checkpoints
            .get(&sequence_number)
            .map(|maybe_checkpoint| maybe_checkpoint.map(VerifiedCheckpoint::new_unchecked))
    }

    pub fn get_latest_certified_checkpoint(&self) -> Option<VerifiedCheckpoint> {
        self.certified_checkpoints
            .iter()
            .skip_to_last()
            .next()
            .map(|(_, v)| VerifiedCheckpoint::new_unchecked(v))
    }

    pub fn multi_get_checkpoint_by_sequence_number(
        &self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Result<Vec<Option<VerifiedCheckpoint>>, TypedStoreError> {
        let checkpoints = self
            .certified_checkpoints
            .multi_get(sequence_numbers)?
            .into_iter()
            .map(|maybe_checkpoint| maybe_checkpoint.map(VerifiedCheckpoint::new_unchecked))
            .collect();

        Ok(checkpoints)
    }

    pub fn get_highest_verified_checkpoint(
        &self,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        let highest_verified = if let Some(highest_verified) =
            self.watermarks.get(&CheckpointWatermark::HighestVerified)?
        {
            highest_verified
        } else {
            return Ok(None);
        };
        self.get_checkpoint_by_digest(&highest_verified.1)
    }

    pub fn get_highest_synced_checkpoint_seq_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>, TypedStoreError> {
        if let Some(highest_synced) = self.watermarks.get(&CheckpointWatermark::HighestSynced)? {
            Ok(Some(highest_synced.0))
        } else {
            Ok(None)
        }
    }

    pub fn get_highest_synced_checkpoint(
        &self,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        let highest_synced = if let Some(highest_synced) =
            self.watermarks.get(&CheckpointWatermark::HighestSynced)?
        {
            highest_synced
        } else {
            return Ok(None);
        };
        self.get_checkpoint_by_digest(&highest_synced.1)
    }

    pub fn get_highest_executed_checkpoint_seq_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>, TypedStoreError> {
        if let Some(highest_executed) =
            self.watermarks.get(&CheckpointWatermark::HighestExecuted)?
        {
            Ok(Some(highest_executed.0))
        } else {
            Ok(None)
        }
    }

    pub fn get_highest_executed_checkpoint(
        &self,
    ) -> Result<Option<VerifiedCheckpoint>, TypedStoreError> {
        let highest_executed = if let Some(highest_executed) =
            self.watermarks.get(&CheckpointWatermark::HighestExecuted)?
        {
            highest_executed
        } else {
            return Ok(None);
        };
        self.get_checkpoint_by_digest(&highest_executed.1)
    }

    pub fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>, TypedStoreError> {
        self.checkpoint_content.get(digest)
    }

    pub fn insert_certified_checkpoint(
        &self,
        checkpoint: &CertifiedCheckpointSummary,
    ) -> Result<(), TypedStoreError> {
        let mut batch = self
            .certified_checkpoints
            .batch()
            .insert_batch(
                &self.certified_checkpoints,
                [(&checkpoint.sequence_number(), checkpoint)],
            )?
            .insert_batch(
                &self.checkpoint_by_digest,
                [(&checkpoint.digest(), checkpoint)],
            )?;
        if checkpoint.next_epoch_committee().is_some() {
            batch = batch.insert_batch(
                &self.epoch_last_checkpoint_map,
                [(&checkpoint.epoch(), &checkpoint.sequence_number())],
            )?;
        }
        batch.write()
    }

    pub fn insert_verified_checkpoint(
        &self,
        checkpoint: VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        self.insert_certified_checkpoint(checkpoint.inner())?;

        // Update latest
        if Some(checkpoint.sequence_number())
            > self
                .get_highest_verified_checkpoint()?
                .map(|x| x.sequence_number())
        {
            self.watermarks.insert(
                &CheckpointWatermark::HighestVerified,
                &(checkpoint.sequence_number(), checkpoint.digest()),
            )?;
        }

        Ok(())
    }

    pub fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        self.watermarks.insert(
            &CheckpointWatermark::HighestSynced,
            &(checkpoint.sequence_number(), checkpoint.digest()),
        )
    }

    pub fn update_highest_executed_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), TypedStoreError> {
        match self.get_highest_executed_checkpoint_seq_number()? {
            Some(seq_number) if seq_number > checkpoint.sequence_number() => Ok(()),
            _ => self.watermarks.insert(
                &CheckpointWatermark::HighestExecuted,
                &(checkpoint.sequence_number(), checkpoint.digest()),
            ),
        }
    }

    pub fn insert_checkpoint_contents(
        &self,
        contents: CheckpointContents,
    ) -> Result<(), TypedStoreError> {
        self.checkpoint_content
            .insert(&contents.digest(), &contents)
    }

    pub fn get_epoch_last_checkpoint(
        &self,
        epoch_id: EpochId,
    ) -> SuiResult<Option<VerifiedCheckpoint>> {
        let seq = self.epoch_last_checkpoint_map.get(&epoch_id)?;
        let checkpoint = match seq {
            Some(seq) => self.get_checkpoint_by_sequence_number(seq)?,
            None => None,
        };
        Ok(checkpoint)
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
                checkpoint.summary.sequence_number + 1,
                checkpoint.summary.network_total_transactions,
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
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointWatermark {
    HighestVerified,
    HighestSynced,
    HighestExecuted,
}

pub struct CheckpointBuilder {
    state: Arc<AuthorityState>,
    tables: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    notify: Arc<Notify>,
    notify_aggregator: Arc<Notify>,
    effects_store: Box<dyn EffectsNotifyRead>,
    output: Box<dyn CheckpointOutput>,
    exit: watch::Receiver<()>,
    metrics: Arc<CheckpointMetrics>,
    transaction_certifier: Box<dyn TransactionCertifier>,
    max_transactions_per_checkpoint: usize,
}

pub struct CheckpointAggregator {
    tables: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    notify: Arc<Notify>,
    exit: watch::Receiver<()>,
    current: Option<CheckpointSignatureAggregator>,
    output: Box<dyn CertifiedCheckpointOutput>,
    metrics: Arc<CheckpointMetrics>,
}

// This holds information to aggregate signatures for one checkpoint
pub struct CheckpointSignatureAggregator {
    next_index: u64,
    summary: CheckpointSummary,
    digest: CheckpointDigest,
    signatures: StakeAggregator<AuthoritySignInfo, false>,
}

impl CheckpointBuilder {
    fn new(
        state: Arc<AuthorityState>,
        tables: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        notify: Arc<Notify>,
        effects_store: Box<dyn EffectsNotifyRead>,
        output: Box<dyn CheckpointOutput>,
        exit: watch::Receiver<()>,
        notify_aggregator: Arc<Notify>,
        metrics: Arc<CheckpointMetrics>,
        transaction_certifier: Box<dyn TransactionCertifier>,
        max_transactions_per_checkpoint: usize,
    ) -> Self {
        Self {
            state,
            tables,
            epoch_store,
            notify,
            effects_store,
            output,
            exit,
            notify_aggregator,
            metrics,
            transaction_certifier,
            max_transactions_per_checkpoint,
        }
    }

    async fn run(mut self) {
        info!("Starting CheckpointBuilder");
        loop {
            // Check whether an exit signal has been received, if so we break the loop.
            // This gives us a chance to exit, in case checkpoint making keeps failing.
            match self.exit.has_changed() {
                Ok(true) | Err(_) => {
                    break;
                }
                Ok(false) => (),
            };
            let mut last_processed_height: Option<u64> = None;
            for (height, pending) in self.epoch_store.get_pending_checkpoints() {
                last_processed_height = Some(height);
                debug!("Making checkpoint at commit height {height}");
                if let Err(e) = self.make_checkpoint(height, pending).await {
                    error!("Error while making checkpoint, will retry in 1s: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    self.metrics.checkpoint_errors.inc();
                    continue;
                }
            }
            debug!("Waiting for more checkpoints from consensus after processing {last_processed_height:?}");
            match select(self.exit.changed().boxed(), self.notify.notified().boxed()).await {
                Either::Left(_) => {
                    // break loop on exit signal
                    break;
                }
                Either::Right(_) => {}
            }
        }
        info!("Shutting down CheckpointBuilder");
    }

    async fn make_checkpoint(
        &self,
        height: CheckpointCommitHeight,
        pending: PendingCheckpoint,
    ) -> anyhow::Result<()> {
        self.metrics
            .checkpoint_roots_count
            .inc_by(pending.roots.len() as u64);
        let roots = self
            .effects_store
            .notify_read_effects(pending.roots)
            .in_monitored_scope("CheckpointNotifyRead")
            .await?;
        let _scope = monitored_scope("CheckpointBuilder");
        let unsorted = self.complete_checkpoint_effects(roots)?;
        let sorted = CasualOrder::casual_sort(unsorted);
        let new_checkpoint = self.create_checkpoints(sorted, pending.details).await?;
        self.write_checkpoints(height, new_checkpoint).await?;
        Ok(())
    }

    async fn write_checkpoints(
        &self,
        height: CheckpointCommitHeight,
        new_checkpoint: Vec<(CheckpointSummary, CheckpointContents)>,
    ) -> SuiResult {
        let mut batch = self.tables.checkpoint_content.batch();
        for (summary, contents) in &new_checkpoint {
            debug!(
                "Created checkpoint from commit height {height} with sequence {}",
                summary.sequence_number
            );
            self.output
                .checkpoint_created(summary, contents, &self.epoch_store)
                .await?;

            self.metrics
                .transactions_included_in_checkpoint
                .inc_by(contents.size() as u64);
            let sequence_number = summary.sequence_number;
            self.metrics
                .last_constructed_checkpoint
                .set(sequence_number as i64);

            batch = batch.insert_batch(
                &self.tables.checkpoint_content,
                [(contents.digest(), contents)],
            )?;
        }
        batch.write()?;
        self.notify_aggregator.notify_waiters();
        self.epoch_store
            .process_pending_checkpoint(height, &new_checkpoint)?;
        Ok(())
    }

    async fn create_checkpoints(
        &self,
        all_effects: Vec<TransactionEffects>,
        details: PendingCheckpointInfo,
    ) -> anyhow::Result<Vec<(CheckpointSummary, CheckpointContents)>> {
        let total = all_effects.len();
        let mut last_checkpoint = self.epoch_store.last_built_checkpoint_summary()?;
        if last_checkpoint.is_none() {
            let epoch = self.epoch_store.epoch();
            if epoch > 0 {
                let previous_epoch = epoch - 1;
                let last_verified = self.tables.get_epoch_last_checkpoint(previous_epoch)?;
                last_checkpoint = last_verified.map(VerifiedCheckpoint::into_summary_and_sequence);
                if let Some((ref seq, _)) = last_checkpoint {
                    debug!("No checkpoints in builder DB, taking checkpoint from previous epoch with sequence {seq}");
                } else {
                    // This is some serious bug with when CheckpointBuilder started so surfacing it via panic
                    panic!("Can not find last checkpoint for previous epoch {previous_epoch}");
                }
            }
        }
        let chunks = all_effects.chunks(self.max_transactions_per_checkpoint);
        let chunks = chunks.into_iter().map(|ch| ch.to_vec());
        let mut chunks: Vec<_> = chunks.collect();
        if chunks.is_empty() {
            // We intentionally create an empty checkpoint here if there is no content provided
            // to make a 'heartbeat' checkpoint.
            // Important: if some conditions are added here later, we need to make sure we always
            // have at least one chunk if last_pending_of_epoch is set
            chunks.push(vec![]);
            // Note: empty checkpoints are ok - they shouldn't happen at all on a network with even
            // modest load. Even if they do happen, it is still useful as it allows fullnodes to
            // distinguish between "no transactions have happened" and "i am not receiving new
            // checkpoints".
        }
        let chunks_count = chunks.len();
        let mut checkpoints = Vec::with_capacity(chunks_count);
        debug!(
            "Creating {} checkpoints with {} transactions total after sequence {:?}",
            chunks_count,
            total,
            last_checkpoint.as_ref().map(|(seq, _)| *seq)
        );
        let epoch = self.epoch_store.epoch();
        for (index, mut effects) in chunks.into_iter().enumerate() {
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
            let digests_without_epoch_augment: Vec<_> =
                effects.iter().map(|e| e.transaction_digest).collect();
            debug!("Waiting for checkpoint user signatures for certificates {:?} to appear in consensus", digests_without_epoch_augment);
            let signatures = self
                .epoch_store
                .user_signatures_for_checkpoint(&digests_without_epoch_augment)
                .await?;
            debug!(
                "Received {} checkpoint user signatures from consensus",
                signatures.len()
            );
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

            let epoch_rolling_gas_cost_summary =
                self.get_epoch_total_gas_cost(last_checkpoint.as_ref().map(|(_, c)| c), &effects);
            if last_checkpoint_of_epoch {
                self.augment_epoch_last_checkpoint(
                    &epoch_rolling_gas_cost_summary,
                    timestamp_ms,
                    &mut effects,
                )
                .await?;
            }

            let contents =
                CheckpointContents::new_with_causally_ordered_transactions_and_signatures(
                    effects.iter().map(TransactionEffects::execution_digests),
                    signatures,
                );

            let num_txns = contents.size() as u64;

            let network_total_transactions = last_checkpoint
                .as_ref()
                .map(|(_, c)| c.network_total_transactions + num_txns)
                .unwrap_or(num_txns);

            let previous_digest = last_checkpoint.as_ref().map(|(_, c)| c.digest());
            let summary = CheckpointSummary::new(
                epoch,
                sequence_number,
                network_total_transactions,
                &contents,
                previous_digest,
                epoch_rolling_gas_cost_summary,
                if last_checkpoint_of_epoch {
                    Some(
                        self.state
                            .get_sui_system_state_object()
                            .unwrap()
                            .get_current_epoch_committee()
                            .committee,
                    )
                } else {
                    None
                },
                timestamp_ms,
            );
            if last_checkpoint_of_epoch {
                info!(
                    ?sequence_number,
                    "creating last checkpoint of epoch {}", epoch
                );
                if let Some(stats) = self.tables.get_epoch_stats(epoch, &summary) {
                    self.epoch_store
                        .report_epoch_metrics_at_last_checkpoint(stats);
                }
            }
            last_checkpoint = Some((sequence_number, summary.clone()));
            checkpoints.push((summary, contents));
        }

        Ok(checkpoints)
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
            )
        } else {
            current_gas_costs
        }
    }

    async fn augment_epoch_last_checkpoint(
        &self,
        epoch_total_gas_cost: &GasCostSummary,
        epoch_start_timestamp_ms: CheckpointTimestamp,
        effects: &mut Vec<TransactionEffects>,
    ) -> anyhow::Result<()> {
        let timer = Instant::now();
        let cert = self
            .state
            .create_advance_epoch_tx_cert(
                &self.epoch_store,
                epoch_total_gas_cost,
                epoch_start_timestamp_ms,
                Duration::from_secs(60), // TODO: Is 60s enough?
                self.transaction_certifier.deref(),
            )
            .await?;
        self.epoch_store
            .record_epoch_last_transaction_cert_creation_time_metric(
                timer.elapsed().as_millis() as i64
            );

        let span = error_span!("augment_epoch_last_checkpoint", tx_digest = ?cert.digest());
        let signed_effect = self
            .state
            .try_execute_immediately(&cert, &self.epoch_store)
            .instrument(span)
            .await?;
        debug!(
            "Effects of the change epoch transaction: {:?}",
            signed_effect.data()
        );
        self.epoch_store.record_is_safe_mode_metric(
            self.state.get_sui_system_state_object().unwrap().safe_mode,
        );
        // The change epoch transaction cannot fail to execute.
        // TODO: Audit the advance_epoch move call to make sure there is no way for it to fail.
        assert!(signed_effect.status.is_ok());
        effects.push(signed_effect.into_message());
        Ok(())
    }

    /// For the given roots return complete list of effects to include in checkpoint
    /// This list includes the roots and all their dependencies, which are not part of checkpoint already
    fn complete_checkpoint_effects(
        &self,
        mut roots: Vec<VerifiedSignedTransactionEffects>,
    ) -> SuiResult<Vec<TransactionEffects>> {
        let mut results = vec![];
        let mut seen = HashSet::new();
        loop {
            let mut pending = HashSet::new();
            for effect in roots {
                let digest = effect.transaction_digest;
                if self
                    .epoch_store
                    .builder_included_transaction_in_checkpoint(&digest)?
                {
                    continue;
                }
                let executed_epoch = self.state.database.transaction_executed_in_epoch(&digest)?;
                if let Some(executed_epoch) = executed_epoch {
                    // Skip here if transaction was executed in previous epoch
                    //
                    // Do not skip if transaction was executed in this epoch -
                    // we rely on builder_included_transaction_in_checkpoint instead for current epoch
                    // because execution can run ahead checkpoint builder
                    if executed_epoch < self.epoch_store.epoch() {
                        continue;
                    }
                }
                for dependency in effect.dependencies.iter() {
                    if seen.insert(*dependency) {
                        pending.insert(*dependency);
                    }
                }
                results.push(effect.into_message());
            }
            if pending.is_empty() {
                break;
            }
            let pending = pending.into_iter().collect::<Vec<_>>();
            let effects = self.effects_store.get_effects(&pending)?;
            let effects = effects
                .into_iter()
                .zip(pending.into_iter())
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
        Ok(results)
    }
}

impl CheckpointAggregator {
    fn new(
        tables: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        notify: Arc<Notify>,
        exit: watch::Receiver<()>,
        output: Box<dyn CertifiedCheckpointOutput>,
        metrics: Arc<CheckpointMetrics>,
    ) -> Self {
        let current = None;
        Self {
            tables,
            epoch_store,
            notify,
            exit,
            current,
            output,
            metrics,
        }
    }

    async fn run(mut self) {
        info!("Starting CheckpointAggregator");
        loop {
            if let Err(e) = self.run_inner().await {
                error!(
                    "Error while aggregating checkpoint, will retry in 1s: {:?}",
                    e
                );
                self.metrics.checkpoint_errors.inc();
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            match select(self.exit.changed().boxed(), self.notify.notified().boxed()).await {
                Either::Left(_) => {
                    // return on exit signal
                    info!("Shutting down CheckpointAggregator");
                    return;
                }
                Either::Right(_) => {}
            }
        }
    }

    async fn run_inner(&mut self) -> SuiResult {
        let _scope = monitored_scope("CheckpointAggregator");
        'outer: loop {
            let current = if let Some(current) = &mut self.current {
                current
            } else {
                let next_to_certify = self.next_checkpoint_to_certify();
                let Some(summary) = self.epoch_store.get_built_checkpoint_summary(next_to_certify)? else { return Ok(()); };
                self.current = Some(CheckpointSignatureAggregator {
                    next_index: 0,
                    digest: summary.digest(),
                    summary,
                    signatures: StakeAggregator::new(self.epoch_store.committee().clone()),
                });
                self.current.as_mut().unwrap()
            };
            let iter = self.epoch_store.get_pending_checkpoint_signatures_iter(
                current.summary.sequence_number,
                current.next_index,
            )?;
            for ((seq, index), data) in iter {
                if seq != current.summary.sequence_number {
                    debug!(
                        "Not enough checkpoint signatures on height {}",
                        current.summary.sequence_number
                    );
                    // No more signatures (yet) for this checkpoint
                    return Ok(());
                }
                debug!(
                    "Processing signature for checkpoint {} from {:?}",
                    current.summary.sequence_number,
                    data.summary.auth_signature.authority.concise()
                );
                self.metrics
                    .checkpoint_participation
                    .with_label_values(&[&format!(
                        "{:?}",
                        data.summary.auth_signature.authority.concise()
                    )])
                    .inc();
                if let Ok(auth_signature) = current.try_aggregate(data) {
                    let summary = CertifiedCheckpointSummary {
                        summary: current.summary.clone(),
                        auth_signature,
                    };
                    self.tables.insert_certified_checkpoint(&summary)?;
                    self.metrics
                        .last_certified_checkpoint
                        .set(current.summary.sequence_number as i64);
                    self.output.certified_checkpoint_created(&summary).await?;
                    self.current = None;
                    continue 'outer;
                } else {
                    current.next_index = index + 1;
                }
            }
            break;
        }
        Ok(())
    }

    fn next_checkpoint_to_certify(&self) -> CheckpointSequenceNumber {
        self.tables
            .certified_checkpoints
            .iter()
            .skip_to_last()
            .next()
            .map(|(seq, _)| seq + 1)
            .unwrap_or_default()
    }
}

impl CheckpointSignatureAggregator {
    #[allow(clippy::result_unit_err)]
    pub fn try_aggregate(
        &mut self,
        data: CheckpointSignatureMessage,
    ) -> Result<AuthorityWeakQuorumSignInfo, ()> {
        let their_digest = data.summary.summary.digest();
        let author = data.summary.auth_signature.authority;
        let signature = data.summary.auth_signature;
        if their_digest != self.digest {
            // todo - consensus need to ensure data.summary.auth_signature.authority == narwhal_cert.author
            warn!(
                "Validator {:?} has mismatching checkpoint digest {} at seq {}, we have digest {}",
                author.concise(),
                Hex::encode(their_digest),
                self.summary.sequence_number,
                Hex::encode(self.digest)
            );
            return Err(());
        }
        match self.signatures.insert(signature) {
            InsertResult::RepeatingEntry { previous, new } => {
                if previous != new {
                    warn!("Validator {:?} submitted two different signatures for checkpoint {}: {:?}, {:?}", author.concise(), self.summary.sequence_number, previous, new);
                }
                Err(())
            }
            InsertResult::Failed { error } => {
                warn!(
                    "Failed to aggregate new signature from validator {:?} for checkpoint {}: {:?}",
                    author.concise(),
                    self.summary.sequence_number,
                    error
                );
                Err(())
            }
            InsertResult::QuorumReached(cert) => Ok(cert),
            InsertResult::NotEnoughVotes => Err(()),
        }
    }
}

pub trait CheckpointServiceNotify {
    fn notify_checkpoint_signature(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        info: &CheckpointSignatureMessage,
    ) -> SuiResult;

    fn notify_checkpoint(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        checkpoint: PendingCheckpoint,
    ) -> SuiResult;
}

/// This is a service used to communicate with other pieces of sui(for ex. authority)
pub struct CheckpointService {
    tables: Arc<CheckpointStore>,
    notify_builder: Arc<Notify>,
    notify_aggregator: Arc<Notify>,
    last_signature_index: Mutex<u64>,
}

impl CheckpointService {
    pub fn spawn(
        state: Arc<AuthorityState>,
        checkpoint_store: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        effects_store: Box<dyn EffectsNotifyRead>,
        checkpoint_output: Box<dyn CheckpointOutput>,
        certified_checkpoint_output: Box<dyn CertifiedCheckpointOutput>,
        transaction_certifier: Box<dyn TransactionCertifier>,
        metrics: Arc<CheckpointMetrics>,
        max_transactions_per_checkpoint: usize,
    ) -> (Arc<Self>, watch::Sender<()> /* The exit sender */) {
        let notify_builder = Arc::new(Notify::new());
        let notify_aggregator = Arc::new(Notify::new());

        let (exit_snd, exit_rcv) = watch::channel(());

        let builder = CheckpointBuilder::new(
            state,
            checkpoint_store.clone(),
            epoch_store.clone(),
            notify_builder.clone(),
            effects_store,
            checkpoint_output,
            exit_rcv.clone(),
            notify_aggregator.clone(),
            metrics.clone(),
            transaction_certifier,
            max_transactions_per_checkpoint,
        );

        spawn_monitored_task!(builder.run());

        let aggregator = CheckpointAggregator::new(
            checkpoint_store.clone(),
            epoch_store.clone(),
            notify_aggregator.clone(),
            exit_rcv,
            certified_checkpoint_output,
            metrics,
        );

        spawn_monitored_task!(aggregator.run());

        let last_signature_index = epoch_store.get_last_checkpoint_signature_index();
        let last_signature_index = Mutex::new(last_signature_index);

        let service = Arc::new(Self {
            tables: checkpoint_store,
            notify_builder,
            notify_aggregator,
            last_signature_index,
        });
        (service, exit_snd)
    }

    /// Used by internal systems that want to subscribe to checkpoints.
    /// Returned sender will contain all checkpoints starting from(inclusive) given sequence number
    /// CheckpointSequenceNumber::default() can be used to start from the beginning
    pub fn subscribe_checkpoints(
        &self,
        from_sequence: CheckpointSequenceNumber,
    ) -> mpsc::Receiver<(CheckpointSummary, CheckpointContents)> {
        let (sender, receiver) = mpsc::channel(8);
        let tailer = CheckpointTailer {
            sender,
            sequence: from_sequence,
            tables: self.tables.clone(),
            notify: self.notify_aggregator.clone(),
        };
        spawn_monitored_task!(tailer.run());
        receiver
    }
}

impl CheckpointServiceNotify for CheckpointService {
    fn notify_checkpoint_signature(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        info: &CheckpointSignatureMessage,
    ) -> SuiResult {
        let sequence = info.summary.summary.sequence_number;
        if let Some((last_certified, _)) = self
            .tables
            .certified_checkpoints
            .iter()
            .skip_to_last()
            .next()
        {
            // TODO(emmazzz): Right now we only record participation of validators whose
            // checkpoint signatures make it to the certified checkpoint, which is only
            // f+1 validators, and the rest of the signatures received are ignored. Later
            // we may want to record those as well so that we have more fine grained scores
            // for tallying rule.
            if sequence <= last_certified {
                debug!(
                    "Ignore signature for checkpoint sequence {} from {} - already certified",
                    info.summary.summary.sequence_number,
                    info.summary.auth_signature.authority.concise(),
                );
                return Ok(());
            }
        }
        debug!(
            "Received signature for checkpoint sequence {}, digest {} from {}",
            sequence,
            Hex::encode(info.summary.summary.digest()),
            info.summary.auth_signature.authority.concise(),
        );
        // While it can be tempting to make last_signature_index into AtomicU64, this won't work
        // We need to make sure we write to `pending_signatures` and trigger `notify_aggregator` without race conditions
        let mut index = self.last_signature_index.lock();
        *index += 1;
        epoch_store.insert_checkpoint_signature(sequence, *index, info)?;
        self.notify_aggregator.notify_one();
        Ok(())
    }

    fn notify_checkpoint(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        checkpoint: PendingCheckpoint,
    ) -> SuiResult {
        if let Some(pending) = epoch_store.get_pending_checkpoint(&checkpoint.height())? {
            if pending.roots != checkpoint.roots {
                panic!("Received checkpoint at index {} that contradicts previously stored checkpoint. Old digests: {:?}, new digests: {:?}", checkpoint.height(), pending.roots, checkpoint.roots);
            }
            debug!(
                "Ignoring duplicate checkpoint notification at height {}",
                checkpoint.height()
            );
            return Ok(());
        }
        debug!(
            "Transaction roots for pending checkpoint at height {}: {:?}",
            checkpoint.height(),
            checkpoint.roots
        );
        epoch_store.insert_pending_checkpoint(&checkpoint.height(), &checkpoint)?;
        debug!(
            "Notifying builder about checkpoint at {}",
            checkpoint.height()
        );
        self.notify_builder.notify_one();
        Ok(())
    }
}

#[cfg(test)]
pub struct CheckpointServiceNoop {}
#[cfg(test)]
impl CheckpointServiceNotify for CheckpointServiceNoop {
    fn notify_checkpoint_signature(
        &self,
        _: &AuthorityPerEpochStore,
        _: &CheckpointSignatureMessage,
    ) -> SuiResult {
        Ok(())
    }

    fn notify_checkpoint(&self, _: &AuthorityPerEpochStore, _: PendingCheckpoint) -> SuiResult {
        Ok(())
    }
}

struct CheckpointTailer {
    sequence: CheckpointSequenceNumber,
    sender: mpsc::Sender<(CheckpointSummary, CheckpointContents)>,
    tables: Arc<CheckpointStore>,
    notify: Arc<Notify>,
}

impl CheckpointTailer {
    async fn run(mut self) {
        loop {
            match self.do_run().await {
                Err(err) => {
                    error!(
                        "Error while tailing checkpoint, will retry in 1s: {:?}",
                        err
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Ok(true) => {}
                Ok(false) => return,
            }
            self.notify.notified().await;
        }
    }

    // Returns Ok(false) if sender channel is closed
    async fn do_run(&mut self) -> SuiResult<bool> {
        loop {
            let summary = self.tables.certified_checkpoints.get(&self.sequence)?;
            let Some(summary) = summary else { return Ok(true); };
            let content = self
                .tables
                .checkpoint_content
                .get(&summary.content_digest())?;
            let Some(content) = content else {
                return Err(SuiError::from("Checkpoint summary for sequence {} exists, but content does not. This should not happen"));
            };
            if self.sender.send((summary.summary, content)).await.is_err() {
                return Ok(false);
            }
            self.sequence += 1;
        }
    }
}

impl PendingCheckpoint {
    pub fn height(&self) -> CheckpointCommitHeight {
        self.details.commit_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority_aggregator::NetworkTransactionCertifier;
    use async_trait::async_trait;
    use fastcrypto::traits::KeyPair;
    use std::collections::HashMap;
    use sui_types::crypto::Signature;
    use sui_types::messages::{SignedTransactionEffects, TrustedSignedTransactionEffects};
    use sui_types::messages_checkpoint::SignedCheckpointSummary;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    #[tokio::test]
    pub async fn checkpoint_builder_test() {
        let tempdir = tempdir().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let network_config = sui_config::builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;
        let committee = genesis.committee().unwrap();
        let keypair = network_config.validator_configs[0]
            .protocol_key_pair()
            .copy();
        let state =
            AuthorityState::new_for_testing(committee.clone(), &keypair, None, &genesis).await;

        let mut store = HashMap::<TransactionDigest, TrustedSignedTransactionEffects>::new();
        store.insert(
            d(1),
            e(
                &state,
                d(1),
                vec![d(2), d(3)],
                GasCostSummary::new(11, 12, 13),
            ),
        );
        store.insert(
            d(2),
            e(
                &state,
                d(2),
                vec![d(3), d(4)],
                GasCostSummary::new(21, 22, 23),
            ),
        );
        store.insert(
            d(3),
            e(&state, d(3), vec![], GasCostSummary::new(31, 32, 33)),
        );
        store.insert(
            d(4),
            e(&state, d(4), vec![], GasCostSummary::new(41, 42, 43)),
        );
        for i in [10, 11, 12, 13] {
            store.insert(
                d(i),
                e(&state, d(i), vec![], GasCostSummary::new(41, 42, 43)),
            );
        }
        let all_digests: Vec<_> = store.iter().map(|(k, _v)| *k).collect();
        for digest in all_digests {
            let signature = Signature::Ed25519SuiSignature(Default::default()).into();
            state
                .epoch_store()
                .test_insert_user_signature(digest, &signature);
        }

        let (output, mut result) = mpsc::channel::<(CheckpointContents, CheckpointSummary)>(10);
        let (certified_output, mut certified_result) =
            mpsc::channel::<CertifiedCheckpointSummary>(10);
        let store = Box::new(store);

        let checkpoint_store = CheckpointStore::new(tempdir.path());
        let epoch_store = state.epoch_store_for_testing();
        let (checkpoint_service, _exit) = CheckpointService::spawn(
            state.clone(),
            checkpoint_store,
            epoch_store.clone(),
            store,
            Box::new(output),
            Box::new(certified_output),
            Box::new(NetworkTransactionCertifier::default()),
            CheckpointMetrics::new_for_tests(),
            3,
        );
        let mut tailer = checkpoint_service.subscribe_checkpoints(0);
        checkpoint_service
            .notify_checkpoint(&epoch_store, p(0, vec![4]))
            .unwrap();
        // Verify that sending same digests at same height is noop
        checkpoint_service
            .notify_checkpoint(&epoch_store, p(0, vec![4]))
            .unwrap();
        checkpoint_service
            .notify_checkpoint(&epoch_store, p(1, vec![1, 3]))
            .unwrap();
        checkpoint_service
            .notify_checkpoint(&epoch_store, p(2, vec![10, 11, 12, 13]))
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
            GasCostSummary::new(41, 42, 43)
        );

        assert_eq!(c2t, vec![d(3), d(2), d(1)]);
        assert_eq!(c2s.previous_digest, Some(c1s.digest()));
        assert_eq!(c2s.sequence_number, 1);
        assert_eq!(
            c2s.epoch_rolling_gas_cost_summary,
            GasCostSummary::new(104, 108, 112)
        );

        // Pending at index 2 had 4 transactions, and we configured 3 transactions max
        // Verify that we split that we generated 2 checkpoints
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

        let c1ss =
            SignedCheckpointSummary::new_from_summary(c1s, keypair.public().into(), &keypair);
        let c2ss =
            SignedCheckpointSummary::new_from_summary(c2s, keypair.public().into(), &keypair);

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
        assert_eq!(c1sc.summary.sequence_number, 0);
        assert_eq!(c2sc.summary.sequence_number, 1);

        let (t1s, _content) = tailer.recv().await.unwrap();
        let (t2s, _content) = tailer.recv().await.unwrap();
        assert_eq!(t1s.sequence_number, 0);
        assert_eq!(t2s.sequence_number, 1);
    }

    #[async_trait]
    impl EffectsNotifyRead for HashMap<TransactionDigest, TrustedSignedTransactionEffects> {
        async fn notify_read_effects(
            &self,
            digests: Vec<TransactionDigest>,
        ) -> SuiResult<Vec<VerifiedSignedTransactionEffects>> {
            Ok(digests
                .into_iter()
                .map(|d| {
                    self.get(d.as_ref())
                        .expect("effects not found")
                        .clone()
                        .into()
                })
                .collect())
        }

        fn get_effects(
            &self,
            digests: &[TransactionDigest],
        ) -> SuiResult<Vec<Option<VerifiedSignedTransactionEffects>>> {
            Ok(digests
                .iter()
                .map(|d| self.get(d.as_ref()).cloned().map(|e_opt| e_opt.into()))
                .collect())
        }
    }

    #[async_trait::async_trait]
    impl CheckpointOutput for mpsc::Sender<(CheckpointContents, CheckpointSummary)> {
        async fn checkpoint_created(
            &self,
            summary: &CheckpointSummary,
            contents: &CheckpointContents,
            _epoch_store: &Arc<AuthorityPerEpochStore>,
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

    fn p(i: u64, t: Vec<u8>) -> PendingCheckpoint {
        PendingCheckpoint {
            roots: t.into_iter().map(d).collect(),
            details: PendingCheckpointInfo {
                timestamp_ms: 0,
                last_of_epoch: false,
                commit_height: i,
            },
        }
    }

    fn d(i: u8) -> TransactionDigest {
        let mut bytes: [u8; 32] = Default::default();
        bytes[0] = i;
        TransactionDigest::new(bytes)
    }

    fn e(
        state: &AuthorityState,
        transaction_digest: TransactionDigest,
        dependencies: Vec<TransactionDigest>,
        gas_used: GasCostSummary,
    ) -> TrustedSignedTransactionEffects {
        let effects = TransactionEffects {
            transaction_digest,
            dependencies,
            gas_used,
            ..Default::default()
        };
        VerifiedSignedTransactionEffects::new_unchecked(SignedTransactionEffects::new(
            state.epoch_store_for_testing().epoch(),
            effects,
            &*state.secret,
            state.name,
        ))
        .serializable()
    }
}
