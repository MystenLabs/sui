// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! CheckpointExecutor is a Node component that executes all checkpoints for the
//! given epoch. It acts as a Consumer to StateSync
//! for newly synced checkpoints, taking these checkpoints and
//! scheduling and monitoring their execution. Its primary goal is to allow
//! for catching up to the current checkpoint sequence number of the network
//! as quickly as possible so that a newly joined, or recovering Node can
//! participate in a timely manner. To that end, CheckpointExecutor attempts
//! to saturate the CPU with executor tasks (one per checkpoint), each of which
//! handle scheduling and awaiting checkpoint transaction execution.
//!
//! CheckpointExecutor is made recoverable in the event of Node shutdown by way of a watermark,
//! highest_executed_checkpoint, which is guaranteed to be updated sequentially in order,
//! despite checkpoints themselves potentially being executed nonsequentially and in parallel.
//! CheckpointExecutor parallelizes checkpoints of the same epoch as much as possible.
//! CheckpointExecutor enforces the invariant that if `run` returns successfully, we have reached the
//! end of epoch. This allows us to use it as a signal for reconfig.

use std::path::PathBuf;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use either::Either;
use futures::stream::FuturesOrdered;
use itertools::izip;
use mysten_common::fatal;
use mysten_metrics::spawn_monitored_task;
use sui_config::node::{CheckpointExecutorConfig, RunWithRange};
use sui_macros::{fail_point, fail_point_async};
use sui_types::accumulator::Accumulator;
use sui_types::crypto::RandomnessRound;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::inner_temporary_store::PackageStoreWithFallback;
use sui_types::message_envelope::Message;
use sui_types::transaction::TransactionKind;
use sui_types::{
    base_types::{ExecutionDigests, TransactionDigest, TransactionEffectsDigest},
    messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint},
    transaction::VerifiedTransaction,
};
use sui_types::{error::SuiResult, transaction::TransactionDataAPI};
use tap::{TapFallible, TapOptional};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    task::JoinHandle,
    time::timeout,
};
use tokio_stream::StreamExt;
use tracing::{debug, error, info, instrument, trace, warn};

use self::metrics::CheckpointExecutorMetrics;
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::backpressure::BackpressureManager;
use crate::authority::AuthorityState;
use crate::checkpoints::checkpoint_executor::data_ingestion_handler::{
    load_checkpoint_data, store_checkpoint_locally,
};
use crate::state_accumulator::StateAccumulator;
use crate::transaction_manager::TransactionManager;
use crate::{
    checkpoints::CheckpointStore,
    execution_cache::{ObjectCacheRead, TransactionCacheRead},
};

mod data_ingestion_handler;
pub mod metrics;

type CheckpointExecutionBuffer = FuturesOrdered<
    JoinHandle<(
        VerifiedCheckpoint,
        Option<Accumulator>,
        Option<CheckpointData>,
        Vec<TransactionDigest>,
        Vec<RandomnessRound>,
    )>,
>;

/// The interval to log checkpoint progress, in # of checkpoints processed.
const CHECKPOINT_PROGRESS_LOG_COUNT_INTERVAL: u64 = 5000;

#[derive(Debug, Clone, Copy)]
pub struct CheckpointTimeoutConfig {
    pub panic_timeout: Option<Duration>,
    pub warning_timeout: Duration,
}

// We use a thread local so that the config can be overridden on a per-test basis. This means
// that get_scheduling_timeout() can be called multiple times in a multithreaded context, but
// the function is still very cheap to call so this is okay.
thread_local! {
    static SCHEDULING_TIMEOUT: once_cell::sync::OnceCell<CheckpointTimeoutConfig> =
        const { once_cell::sync::OnceCell::new() };
}

#[cfg(msim)]
pub fn init_checkpoint_timeout_config(config: CheckpointTimeoutConfig) {
    SCHEDULING_TIMEOUT.with(|s| {
        s.set(config).expect("SchedulingTimeoutConfig already set");
    });
}

fn get_scheduling_timeout() -> CheckpointTimeoutConfig {
    fn inner() -> CheckpointTimeoutConfig {
        let panic_timeout: Option<Duration> = if cfg!(msim) {
            Some(Duration::from_secs(45))
        } else {
            std::env::var("NEW_CHECKPOINT_PANIC_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_millis)
        };

        let warning_timeout: Duration = std::env::var("NEW_CHECKPOINT_WARNING_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_secs(5));

        CheckpointTimeoutConfig {
            panic_timeout,
            warning_timeout,
        }
    }

    SCHEDULING_TIMEOUT.with(|s| *s.get_or_init(inner))
}

#[derive(PartialEq, Eq, Debug)]
pub enum StopReason {
    EpochComplete,
    RunWithRangeCondition,
}

pub struct CheckpointExecutor {
    mailbox: broadcast::Receiver<VerifiedCheckpoint>,
    // TODO: AuthorityState is only needed because we have to call deprecated_insert_finalized_transactions
    // once that code is fully deprecated we can remove this
    state: Arc<AuthorityState>,
    checkpoint_store: Arc<CheckpointStore>,
    object_cache_reader: Arc<dyn ObjectCacheRead>,
    transaction_cache_reader: Arc<dyn TransactionCacheRead>,
    tx_manager: Arc<TransactionManager>,
    accumulator: Arc<StateAccumulator>,
    backpressure_manager: Arc<BackpressureManager>,
    config: CheckpointExecutorConfig,
    metrics: Arc<CheckpointExecutorMetrics>,
    subscription_service_checkpoint_sender: Option<tokio::sync::mpsc::Sender<CheckpointData>>,
}

impl CheckpointExecutor {
    pub fn new(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        state: Arc<AuthorityState>,
        accumulator: Arc<StateAccumulator>,
        backpressure_manager: Arc<BackpressureManager>,
        config: CheckpointExecutorConfig,
        metrics: Arc<CheckpointExecutorMetrics>,
        subscription_service_checkpoint_sender: Option<tokio::sync::mpsc::Sender<CheckpointData>>,
    ) -> Self {
        Self {
            mailbox,
            state: state.clone(),
            checkpoint_store,
            object_cache_reader: state.get_object_cache_reader().clone(),
            transaction_cache_reader: state.get_transaction_cache_reader().clone(),
            tx_manager: state.transaction_manager().clone(),
            accumulator,
            backpressure_manager,
            config,
            metrics,
            subscription_service_checkpoint_sender,
        }
    }

    pub fn new_for_tests(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        state: Arc<AuthorityState>,
        accumulator: Arc<StateAccumulator>,
    ) -> Self {
        Self::new(
            mailbox,
            checkpoint_store,
            state,
            accumulator,
            BackpressureManager::new_for_tests(),
            Default::default(),
            CheckpointExecutorMetrics::new_for_tests(),
            None,
        )
    }

    /// Ensure that all checkpoints in the current epoch will be executed.
    /// We don't technically need &mut on self, but passing it to make sure only one instance is
    /// running at one time.
    pub async fn run_epoch(
        &mut self,
        epoch_store: Arc<AuthorityPerEpochStore>,
        run_with_range: Option<RunWithRange>,
    ) -> StopReason {
        // check if we want to run this epoch based on RunWithRange condition value
        // we want to be inclusive of the defined RunWithRangeEpoch::Epoch
        // i.e Epoch(N) means we will execute epoch N and stop when reaching N+1
        if run_with_range.map_or(false, |rwr| rwr.is_epoch_gt(epoch_store.epoch())) {
            info!(
                "RunWithRange condition satisfied at {:?}, run_epoch={:?}",
                run_with_range,
                epoch_store.epoch()
            );
            return StopReason::RunWithRangeCondition;
        };

        debug!(
            "Checkpoint executor running for epoch {}",
            epoch_store.epoch(),
        );
        self.metrics
            .checkpoint_exec_epoch
            .set(epoch_store.epoch() as i64);

        // Decide the first checkpoint to schedule for execution.
        // If we haven't executed anything in the past, we schedule checkpoint 0.
        // Otherwise we schedule the one after highest executed.
        let mut highest_executed = self
            .checkpoint_store
            .get_highest_executed_checkpoint()
            .unwrap();

        if let Some(highest_executed) = &highest_executed {
            if epoch_store.epoch() == highest_executed.epoch()
                && highest_executed.is_last_checkpoint_of_epoch()
            {
                // We can arrive at this point if we bump the highest_executed_checkpoint watermark, and then
                // crash before completing reconfiguration.
                info!(seq = ?highest_executed.sequence_number, "final checkpoint of epoch has already been executed");
                return StopReason::EpochComplete;
            }
        }

        let mut next_to_schedule = highest_executed
            .as_ref()
            .map(|c| c.sequence_number() + 1)
            .unwrap_or_else(|| {
                // TODO this invariant may no longer hold once we introduce snapshots
                assert_eq!(epoch_store.epoch(), 0);
                0
            });
        let mut pending: CheckpointExecutionBuffer = FuturesOrdered::new();

        let mut now_time = Instant::now();
        let mut now_transaction_num = highest_executed
            .as_ref()
            .map(|c| c.network_total_transactions)
            .unwrap_or(0);
        let scheduling_timeout_config = get_scheduling_timeout();

        loop {
            let schedule_scope = mysten_metrics::monitored_scope("ScheduleCheckpointExecution");

            // If we have executed the last checkpoint of the current epoch, stop.
            // Note: when we arrive here with highest_executed == the final checkpoint of the epoch,
            // we are in an edge case where highest_executed does not actually correspond to the watermark.
            // The watermark is only bumped past the epoch final checkpoint after execution of the change
            // epoch tx, and state accumulation.
            if self
                .check_epoch_last_checkpoint(epoch_store.clone(), &highest_executed)
                .await
            {
                self.checkpoint_store
                    .prune_local_summaries()
                    .tap_err(|e| error!("Failed to prune local summaries: {}", e))
                    .ok();

                // be extra careful to ensure we don't have orphans
                assert!(
                    pending.is_empty(),
                    "Pending checkpoint execution buffer should be empty after processing last checkpoint of epoch",
                );
                fail_point!("crash");
                debug!(epoch = epoch_store.epoch(), "finished epoch");
                return StopReason::EpochComplete;
            }

            self.schedule_synced_checkpoints(
                &mut pending,
                // next_to_schedule will be updated to the next checkpoint to schedule.
                // This makes sure we don't re-schedule the same checkpoint multiple times.
                &mut next_to_schedule,
                epoch_store.clone(),
                run_with_range,
            );

            self.metrics
                .checkpoint_exec_inflight
                .set(pending.len() as i64);

            let panic_timeout = scheduling_timeout_config.panic_timeout;
            let warning_timeout = scheduling_timeout_config.warning_timeout;

            drop(schedule_scope);
            tokio::select! {
                // Check for completed workers and ratchet the highest_checkpoint_executed
                // watermark accordingly. Note that given that checkpoints are guaranteed to
                // be processed (added to FuturesOrdered) in seq_number order, using FuturesOrdered
                // guarantees that we will also ratchet the watermarks in order.
                Some(Ok((checkpoint, checkpoint_acc, checkpoint_data, tx_digests, randomness_rounds))) = pending.next() => {
                    let _process_scope = mysten_metrics::monitored_scope("ProcessExecutedCheckpoint");

                    self.process_executed_checkpoint(&epoch_store, &checkpoint, checkpoint_acc, checkpoint_data, &tx_digests, randomness_rounds).await;
                    self.backpressure_manager.update_highest_executed_checkpoint(*checkpoint.sequence_number());
                    highest_executed = Some(checkpoint.clone());

                    // Estimate TPS every 10k transactions or 30 sec
                    let elapsed = now_time.elapsed().as_millis();
                    let current_transaction_num =  highest_executed.as_ref().map(|c| c.network_total_transactions).unwrap_or(0);
                    if current_transaction_num - now_transaction_num > 10_000 || elapsed > 30_000 {
                        let tps = (1000.0 * (current_transaction_num - now_transaction_num) as f64 / elapsed as f64) as i32;
                        self.metrics.checkpoint_exec_sync_tps.set(tps as i64);
                        now_time = Instant::now();
                        now_transaction_num = current_transaction_num;
                    }
                     // we want to be inclusive of checkpoints in RunWithRange::Checkpoint type
                    if run_with_range.map_or(false, |rwr| rwr.matches_checkpoint(checkpoint.sequence_number)) {
                        info!(
                            "RunWithRange condition satisfied after checkpoint sequence number {:?}",
                            checkpoint.sequence_number
                        );
                        return StopReason::RunWithRangeCondition;
                    }
                }

                received = self.mailbox.recv() => match received {
                    Ok(checkpoint) => {
                        info!(
                            sequence_number = ?checkpoint.sequence_number,
                            "Received checkpoint summary from state sync"
                        );
                        checkpoint.report_checkpoint_age(&self.metrics.checkpoint_contents_age, &self.metrics.checkpoint_contents_age_ms);
                        // Note: checkpoints arrive in increasing order by sequence number, but they are not
                        // necessarily consecutive.
                        self.backpressure_manager.update_highest_certified_checkpoint(*checkpoint.sequence_number());
                    },
                    Err(RecvError::Lagged(num_skipped)) => {
                        debug!(
                            "Checkpoint Execution Recv channel overflowed with {:?} messages",
                            num_skipped,
                        );
                    }
                    Err(RecvError::Closed) => {
                        panic!("Checkpoint Execution Sender (StateSync) closed channel unexpectedly");
                    },
                },

                _ = tokio::time::sleep(warning_timeout) => {
                    warn!(
                        "Received no new synced checkpoints for {warning_timeout:?}. Next checkpoint to be scheduled: {next_to_schedule}",
                    );
                }

                _ = panic_timeout
                            .map(|d| Either::Left(tokio::time::sleep(d)))
                            .unwrap_or_else(|| Either::Right(futures::future::pending())) => {
                    panic!("No new synced checkpoints received for {panic_timeout:?} on node {:?}", self.state.name);
                },
            }
        }
    }

    fn bump_highest_executed_checkpoint(&self, checkpoint: &VerifiedCheckpoint) {
        // Ensure that we are not skipping checkpoints at any point
        let seq = *checkpoint.sequence_number();
        debug!("Bumping highest_executed_checkpoint watermark to {seq:?}");
        if let Some(prev_highest) = self
            .checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
        {
            assert_eq!(prev_highest + 1, seq);
        } else {
            assert_eq!(seq, 0);
        }
        if seq % CHECKPOINT_PROGRESS_LOG_COUNT_INTERVAL == 0 {
            info!("Finished syncing and executing checkpoint {}", seq);
        }

        fail_point!("highest-executed-checkpoint");

        // We store a fixed number of additional FullCheckpointContents after execution is complete
        // for use in state sync.
        const NUM_SAVED_FULL_CHECKPOINT_CONTENTS: u64 = 5_000;
        if seq >= NUM_SAVED_FULL_CHECKPOINT_CONTENTS {
            let prune_seq = seq - NUM_SAVED_FULL_CHECKPOINT_CONTENTS;
            if let Some(prune_checkpoint) = self
                .checkpoint_store
                .get_checkpoint_by_sequence_number(prune_seq)
                .expect("Failed to fetch checkpoint")
            {
                self.checkpoint_store
                    .delete_full_checkpoint_contents(prune_seq)
                    .expect("Failed to delete full checkpoint contents");
                self.checkpoint_store
                    .delete_contents_digest_sequence_number_mapping(
                        &prune_checkpoint.content_digest,
                    )
                    .expect("Failed to delete contents digest -> sequence number mapping");
            } else {
                // If this is directly after a snapshot restore with skiplisting,
                // this is expected for the first `NUM_SAVED_FULL_CHECKPOINT_CONTENTS`
                // checkpoints.
                debug!(
                    "Failed to fetch checkpoint with sequence number {:?}",
                    prune_seq
                );
            }
        }

        self.checkpoint_store
            .update_highest_executed_checkpoint(checkpoint)
            .unwrap();
        self.metrics.last_executed_checkpoint.set(seq as i64);

        self.metrics
            .last_executed_checkpoint_timestamp_ms
            .set(checkpoint.timestamp_ms as i64);
        checkpoint.report_checkpoint_age(
            &self.metrics.last_executed_checkpoint_age,
            &self.metrics.last_executed_checkpoint_age_ms,
        );
    }

    /// Post processing and plumbing after we executed a checkpoint. This function is guaranteed
    /// to be called in the order of checkpoint sequence number.
    #[instrument(level = "info", skip_all, fields(seq = ?checkpoint.sequence_number()))]
    async fn process_executed_checkpoint(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        checkpoint: &VerifiedCheckpoint,
        checkpoint_acc: Option<Accumulator>,
        checkpoint_data: Option<CheckpointData>,
        all_tx_digests: &[TransactionDigest],
        randomness_rounds: Vec<RandomnessRound>,
    ) {
        // Commit all transaction effects to disk
        let cache_commit = self.state.get_cache_commit();
        debug!("committing checkpoint transactions to disk");
        cache_commit.commit_transaction_outputs(
            epoch_store.epoch(),
            all_tx_digests,
            epoch_store
                .protocol_config()
                .use_object_per_epoch_marker_table_v2_as_option()
                .unwrap_or(false),
        );

        epoch_store
            .handle_finalized_checkpoint(checkpoint.data(), all_tx_digests)
            .expect("cannot fail");

        // Once the checkpoint is finalized, we know that any randomness contained in this checkpoint has
        // been successfully included in a checkpoint certified by quorum of validators.
        // (RandomnessManager/RandomnessReporter is only present on validators.)
        if let Some(randomness_reporter) = epoch_store.randomness_reporter() {
            for round in randomness_rounds {
                debug!(
                    ?round,
                    "notifying RandomnessReporter that randomness update was executed in checkpoint"
                );
                randomness_reporter
                    .notify_randomness_in_checkpoint(round)
                    .expect("epoch cannot have ended");
            }
        }

        if let Some(checkpoint_data) = checkpoint_data {
            self.commit_index_updates_and_enqueue_to_subscription_service(checkpoint_data)
                .await;
        }

        if !checkpoint.is_last_checkpoint_of_epoch() {
            self.accumulator
                .accumulate_running_root(epoch_store, checkpoint.sequence_number, checkpoint_acc)
                .await
                .expect("Failed to accumulate running root");
            self.bump_highest_executed_checkpoint(checkpoint);
        }
    }

    /// If configured, commit the pending index updates for the provided checkpoint as well as
    /// enqueuing the checkpoint to the subscription service
    async fn commit_index_updates_and_enqueue_to_subscription_service(
        &self,
        checkpoint: CheckpointData,
    ) {
        if let Some(rpc_index) = &self.state.rpc_index {
            rpc_index
                .commit_update_for_checkpoint(checkpoint.checkpoint_summary.sequence_number)
                .expect("failed to update rpc_indexes");
        }

        if let Some(sender) = &self.subscription_service_checkpoint_sender {
            if let Err(e) = sender.send(checkpoint).await {
                tracing::warn!("unable to send checkpoint to subscription service: {e}");
            }
        }
    }

    #[instrument(level = "debug", skip_all)]
    fn schedule_synced_checkpoints(
        &self,
        pending: &mut CheckpointExecutionBuffer,
        next_to_schedule: &mut CheckpointSequenceNumber,
        epoch_store: Arc<AuthorityPerEpochStore>,
        run_with_range: Option<RunWithRange>,
    ) {
        let Some(latest_synced_checkpoint) = self
            .checkpoint_store
            .get_highest_synced_checkpoint()
            .expect("Failed to read highest synced checkpoint")
        else {
            debug!("No checkpoints to schedule, highest synced checkpoint is None",);
            return;
        };

        while *next_to_schedule <= *latest_synced_checkpoint.sequence_number()
            && pending.len() < self.config.checkpoint_execution_max_concurrency
        {
            let checkpoint = self
                .checkpoint_store
                .get_checkpoint_by_sequence_number(*next_to_schedule)
                .unwrap()
                .unwrap_or_else(|| {
                    panic!(
                        "Checkpoint sequence number {:?} does not exist in checkpoint store",
                        *next_to_schedule
                    )
                });
            if checkpoint.epoch() > epoch_store.epoch() {
                return;
            }
            match run_with_range {
                Some(RunWithRange::Checkpoint(seq)) if *next_to_schedule > seq => {
                    debug!(
                        "RunWithRange Checkpoint {} is set, not scheduling checkpoint {}",
                        seq, *next_to_schedule
                    );
                    return;
                }
                _ => {
                    self.schedule_checkpoint(checkpoint, pending, epoch_store.clone());
                    *next_to_schedule += 1;
                }
            }
        }
    }

    #[instrument(level = "error", skip_all, fields(seq = ?checkpoint.sequence_number(), epoch = ?epoch_store.epoch()))]
    fn schedule_checkpoint(
        &self,
        checkpoint: VerifiedCheckpoint,
        pending: &mut CheckpointExecutionBuffer,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) {
        debug!("Scheduling checkpoint for execution");

        // Mismatch between node epoch and checkpoint epoch after startup
        // crash recovery is invalid
        let checkpoint_epoch = checkpoint.epoch();
        assert_eq!(
            checkpoint_epoch,
            epoch_store.epoch(),
            "Epoch mismatch after startup recovery. checkpoint epoch: {:?}, node epoch: {:?}",
            checkpoint_epoch,
            epoch_store.epoch(),
        );

        let metrics = self.metrics.clone();
        let local_execution_timeout_sec = self.config.local_execution_timeout_sec;
        let data_ingestion_dir = self.config.data_ingestion_dir.clone();
        let checkpoint_store = self.checkpoint_store.clone();
        let object_cache_reader = self.object_cache_reader.clone();
        let transaction_cache_reader = self.transaction_cache_reader.clone();
        let tx_manager = self.tx_manager.clone();
        let accumulator = self.accumulator.clone();
        let state = self.state.clone();
        let subscription_service_enabled = self.subscription_service_checkpoint_sender.is_some();

        epoch_store.notify_synced_checkpoint(*checkpoint.sequence_number());

        pending.push_back(spawn_monitored_task!(async move {
            let epoch_store = epoch_store.clone();
            let (tx_digests, checkpoint_acc, checkpoint_data, randomness_rounds) = loop {
                match execute_checkpoint(
                    checkpoint.clone(),
                    &state,
                    object_cache_reader.as_ref(),
                    transaction_cache_reader.as_ref(),
                    checkpoint_store.clone(),
                    epoch_store.clone(),
                    tx_manager.clone(),
                    accumulator.clone(),
                    local_execution_timeout_sec,
                    &metrics,
                    data_ingestion_dir.clone(),
                    subscription_service_enabled,
                )
                .await
                {
                    Err(err) => {
                        error!(
                            "Error while executing checkpoint, will retry in 1s: {:?}",
                            err
                        );
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        metrics.checkpoint_exec_errors.inc();
                    }
                    Ok((tx_digests, checkpoint_acc, checkpoint_data, randomness_rounds)) => {
                        break (
                            tx_digests,
                            checkpoint_acc,
                            checkpoint_data,
                            randomness_rounds,
                        )
                    }
                }
            };
            (
                checkpoint,
                checkpoint_acc,
                checkpoint_data,
                tx_digests,
                randomness_rounds,
            )
        }));
    }

    #[instrument(level = "info", skip_all)]
    async fn execute_change_epoch_tx(
        &self,
        execution_digests: ExecutionDigests,
        change_epoch_tx_digest: TransactionDigest,
        change_epoch_tx: VerifiedExecutableTransaction,
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint: VerifiedCheckpoint,
    ) {
        let change_epoch_fx = self
            .transaction_cache_reader
            .get_effects(&execution_digests.effects)
            .expect("Change_epoch tx effects must exist");

        if change_epoch_tx.contains_shared_object() {
            epoch_store
                .acquire_shared_version_assignments_from_effects(
                    &change_epoch_tx,
                    &change_epoch_fx,
                    self.object_cache_reader.as_ref(),
                )
                .expect("Acquiring shared version assignments for change_epoch tx cannot fail");
        }

        self.tx_manager.enqueue_with_expected_effects_digest(
            vec![(change_epoch_tx.clone(), execution_digests.effects)],
            &epoch_store,
        );
        handle_execution_effects(
            &self.state,
            vec![execution_digests],
            vec![change_epoch_tx_digest],
            checkpoint.clone(),
            self.checkpoint_store.clone(),
            self.object_cache_reader.as_ref(),
            self.transaction_cache_reader.as_ref(),
            epoch_store.clone(),
            self.tx_manager.clone(),
            self.accumulator.clone(),
            self.config.local_execution_timeout_sec,
            self.config.data_ingestion_dir.clone(),
            self.subscription_service_checkpoint_sender.is_some(),
        )
        .await;
    }

    /// Check whether `checkpoint` is the last checkpoint of the current epoch. If so,
    /// perform special case logic (execute change_epoch tx, accumulate epoch,
    /// finalize transactions), then return true.
    pub async fn check_epoch_last_checkpoint(
        &self,
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint: &Option<VerifiedCheckpoint>,
    ) -> bool {
        let cur_epoch = epoch_store.epoch();

        if let Some(checkpoint) = checkpoint {
            if checkpoint.epoch() == cur_epoch {
                if let Some((change_epoch_execution_digests, change_epoch_tx)) =
                    extract_end_of_epoch_tx(
                        checkpoint,
                        self.transaction_cache_reader.as_ref(),
                        self.checkpoint_store.clone(),
                        epoch_store.clone(),
                    )
                {
                    let change_epoch_tx_digest = change_epoch_execution_digests.transaction;

                    info!(
                        ended_epoch = cur_epoch,
                        last_checkpoint = checkpoint.sequence_number(),
                        "Reached end of epoch, executing change_epoch transaction",
                    );

                    self.execute_change_epoch_tx(
                        change_epoch_execution_digests,
                        change_epoch_tx_digest,
                        change_epoch_tx,
                        epoch_store.clone(),
                        checkpoint.clone(),
                    )
                    .await;

                    let cache_commit = self.state.get_cache_commit();
                    cache_commit.commit_transaction_outputs(
                        cur_epoch,
                        &[change_epoch_tx_digest],
                        epoch_store
                            .protocol_config()
                            .use_object_per_epoch_marker_table_v2_as_option()
                            .unwrap_or(false),
                    );
                    fail_point_async!("prune-and-compact");

                    // For finalizing the checkpoint, we need to pass in all checkpoint
                    // transaction effects, not just the change_epoch tx effects. However,
                    // we have already notify awaited all tx effects separately (once
                    // for change_epoch tx, and once for all other txes). Therefore this
                    // should be a fast operation
                    let all_tx_digests: Vec<_> = self
                        .checkpoint_store
                        .get_checkpoint_contents(&checkpoint.content_digest)
                        .expect("read cannot fail")
                        .expect("Checkpoint contents should exist")
                        .iter()
                        .map(|digests| digests.transaction)
                        .collect();

                    let effects = self
                        .transaction_cache_reader
                        .notify_read_executed_effects(&all_tx_digests)
                        .await;

                    let (_acc, checkpoint_data) = finalize_checkpoint(
                        &self.state,
                        self.object_cache_reader.as_ref(),
                        self.transaction_cache_reader.as_ref(),
                        self.checkpoint_store.clone(),
                        &all_tx_digests,
                        &epoch_store,
                        checkpoint.clone(),
                        self.accumulator.clone(),
                        effects,
                        self.config.data_ingestion_dir.clone(),
                        self.subscription_service_checkpoint_sender.is_some(),
                    )
                    .await
                    .expect("Finalizing checkpoint cannot fail");

                    if let Some(checkpoint_data) = checkpoint_data {
                        self.commit_index_updates_and_enqueue_to_subscription_service(
                            checkpoint_data,
                        )
                        .await;
                    }

                    self.checkpoint_store
                        .insert_epoch_last_checkpoint(cur_epoch, checkpoint)
                        .expect("Failed to insert epoch last checkpoint");

                    self.accumulator
                        .accumulate_running_root(&epoch_store, checkpoint.sequence_number, None)
                        .await
                        .expect("Failed to accumulate running root");
                    self.accumulator
                        .accumulate_epoch(epoch_store.clone(), *checkpoint.sequence_number())
                        .expect("Accumulating epoch cannot fail");

                    self.bump_highest_executed_checkpoint(checkpoint);

                    return true;
                }
            }
        }
        false
    }
}

// Logs within the function are annotated with the checkpoint sequence number and epoch,
// from schedule_checkpoint().
#[allow(clippy::type_complexity)]
#[instrument(level = "debug", skip_all, fields(seq = ?checkpoint.sequence_number(), epoch = ?epoch_store.epoch()))]
async fn execute_checkpoint(
    checkpoint: VerifiedCheckpoint,
    state: &AuthorityState,
    object_cache_reader: &dyn ObjectCacheRead,
    transaction_cache_reader: &dyn TransactionCacheRead,
    checkpoint_store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    transaction_manager: Arc<TransactionManager>,
    accumulator: Arc<StateAccumulator>,
    local_execution_timeout_sec: u64,
    metrics: &Arc<CheckpointExecutorMetrics>,
    data_ingestion_dir: Option<PathBuf>,
    subscription_service_enabled: bool,
) -> SuiResult<(
    Vec<TransactionDigest>,
    Option<Accumulator>,
    Option<CheckpointData>,
    Vec<RandomnessRound>,
)> {
    debug!("Preparing checkpoint for execution",);
    let prepare_start = Instant::now();

    // this function must guarantee that all transactions in the checkpoint are executed before it
    // returns. This invariant is enforced in two phases:
    // - First, we filter out any already executed transactions from the checkpoint in
    //   get_unexecuted_transactions()
    // - Second, we execute all remaining transactions.

    let (execution_digests, all_tx_digests, executable_txns, randomness_rounds) =
        get_unexecuted_transactions(
            checkpoint.clone(),
            transaction_cache_reader,
            checkpoint_store.clone(),
            epoch_store.clone(),
        );

    let tx_count = execution_digests.len();
    debug!("Number of transactions in the checkpoint: {:?}", tx_count);
    metrics
        .checkpoint_transaction_count
        .observe(tx_count as f64);

    let (checkpoint_acc, checkpoint_data) = execute_transactions(
        execution_digests,
        all_tx_digests.clone(),
        executable_txns,
        state,
        object_cache_reader,
        transaction_cache_reader,
        checkpoint_store.clone(),
        epoch_store.clone(),
        transaction_manager,
        accumulator,
        local_execution_timeout_sec,
        checkpoint,
        metrics,
        prepare_start,
        data_ingestion_dir,
        subscription_service_enabled,
    )
    .await?;

    Ok((
        all_tx_digests,
        checkpoint_acc,
        checkpoint_data,
        randomness_rounds,
    ))
}

#[instrument(level = "error", skip_all, fields(seq = ?checkpoint.sequence_number(), epoch = ?epoch_store.epoch()))]
async fn handle_execution_effects(
    state: &AuthorityState,
    execution_digests: Vec<ExecutionDigests>,
    all_tx_digests: Vec<TransactionDigest>,
    checkpoint: VerifiedCheckpoint,
    checkpoint_store: Arc<CheckpointStore>,
    object_cache_reader: &dyn ObjectCacheRead,
    transaction_cache_reader: &dyn TransactionCacheRead,
    epoch_store: Arc<AuthorityPerEpochStore>,
    transaction_manager: Arc<TransactionManager>,
    accumulator: Arc<StateAccumulator>,
    local_execution_timeout_sec: u64,
    data_ingestion_dir: Option<PathBuf>,
    subscription_service_enabled: bool,
) -> (Option<Accumulator>, Option<CheckpointData>) {
    // Once synced_txns have been awaited, all txns should have effects committed.
    let mut periods = 1;
    let log_timeout_sec = Duration::from_secs(local_execution_timeout_sec);
    // Whether the checkpoint is next to execute and blocking additional executions.
    let mut blocking_execution = false;
    loop {
        let effects_future = transaction_cache_reader.notify_read_executed_effects(&all_tx_digests);

        match timeout(log_timeout_sec, effects_future).await {
            Err(_elapsed) => {
                // Reading this value every timeout should be ok.
                let highest_seq = checkpoint_store
                    .get_highest_executed_checkpoint_seq_number()
                    .unwrap()
                    .unwrap_or_default();
                if checkpoint.sequence_number <= highest_seq {
                    error!(
                        "Re-executing checkpoint {} after higher checkpoint {} has executed!",
                        checkpoint.sequence_number, highest_seq
                    );
                    continue;
                }
                if checkpoint.sequence_number > highest_seq + 1 {
                    trace!(
                        "Checkpoint {} is still executing. Highest executed = {}",
                        checkpoint.sequence_number,
                        highest_seq
                    );
                    continue;
                }
                if !blocking_execution {
                    trace!(
                        "Checkpoint {} is next to execute.",
                        checkpoint.sequence_number
                    );
                    blocking_execution = true;
                    continue;
                }

                // Only log details when the checkpoint is next to execute, but has not finished
                // execution within log_timeout_sec.
                let missing_digests: Vec<TransactionDigest> = transaction_cache_reader
                    .multi_get_executed_effects_digests(&all_tx_digests)
                    .iter()
                    .zip(all_tx_digests.clone())
                    .filter_map(
                        |(fx, digest)| {
                            if fx.is_none() {
                                Some(digest)
                            } else {
                                None
                            }
                        },
                    )
                    .collect();

                if missing_digests.is_empty() {
                    // All effects just become available.
                    continue;
                }

                warn!(
                    "Transaction effects for checkpoint tx digests {:?} not present within {:?}. ",
                    missing_digests,
                    log_timeout_sec * periods,
                );

                // Print out more information for the 1st pending transaction, which should have
                // all of its input available.
                let pending_digest = missing_digests.first().unwrap();
                if let Some(missing_input) = transaction_manager.get_missing_input(pending_digest) {
                    warn!(
                        "Transaction {pending_digest:?} has missing input objects {missing_input:?}",
                    );
                }
                periods += 1;
            }
            Ok(effects) => {
                for (tx_digest, expected_digest, actual_effects) in
                    izip!(&all_tx_digests, &execution_digests, &effects)
                {
                    let expected_effects_digest = &expected_digest.effects;
                    assert_not_forked(
                        &checkpoint,
                        tx_digest,
                        expected_effects_digest,
                        &actual_effects.digest(),
                        transaction_cache_reader,
                    );
                }

                // if end of epoch checkpoint, we must finalize the checkpoint after executing
                // the change epoch tx, which is done after all other checkpoint execution
                if checkpoint.end_of_epoch_data.is_none() {
                    let (checkpoint_acc, checkpoint_data) = finalize_checkpoint(
                        state,
                        object_cache_reader,
                        transaction_cache_reader,
                        checkpoint_store.clone(),
                        &all_tx_digests,
                        &epoch_store,
                        checkpoint.clone(),
                        accumulator.clone(),
                        effects,
                        data_ingestion_dir,
                        subscription_service_enabled,
                    )
                    .await
                    .expect("Finalizing checkpoint cannot fail");
                    return (Some(checkpoint_acc), checkpoint_data);
                } else {
                    return (None, None);
                }
            }
        }
    }
}

fn assert_not_forked(
    checkpoint: &VerifiedCheckpoint,
    tx_digest: &TransactionDigest,
    expected_digest: &TransactionEffectsDigest,
    actual_effects_digest: &TransactionEffectsDigest,
    cache_reader: &dyn TransactionCacheRead,
) {
    if *expected_digest != *actual_effects_digest {
        let actual_effects = cache_reader
            .get_executed_effects(tx_digest)
            .expect("actual effects should exist");

        // log observed effects (too big for panic message) and then panic.
        error!(
            ?checkpoint,
            ?tx_digest,
            ?expected_digest,
            ?actual_effects,
            "fork detected!"
        );
        panic!(
            "When executing checkpoint {}, transaction {} \
            is expected to have effects digest {}, but got {}!",
            checkpoint.sequence_number(),
            tx_digest,
            expected_digest,
            actual_effects_digest,
        );
    }
}

// Given a checkpoint, find the end of epoch transaction, if it exists
fn extract_end_of_epoch_tx(
    checkpoint: &VerifiedCheckpoint,
    cache_reader: &dyn TransactionCacheRead,
    checkpoint_store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
) -> Option<(ExecutionDigests, VerifiedExecutableTransaction)> {
    checkpoint.end_of_epoch_data.as_ref()?;

    // Last checkpoint must have the end of epoch transaction as the last transaction.

    let checkpoint_sequence = checkpoint.sequence_number();
    let execution_digests = checkpoint_store
        .get_checkpoint_contents(&checkpoint.content_digest)
        .expect("Failed to get checkpoint contents from store")
        .unwrap_or_else(|| {
            panic!(
                "Checkpoint contents for digest {:?} does not exist",
                checkpoint.content_digest
            )
        })
        .into_inner();

    let digests = execution_digests
        .last()
        .expect("Final checkpoint must have at least one transaction");

    let change_epoch_tx = cache_reader.get_transaction_block(&digests.transaction);

    let change_epoch_tx = VerifiedExecutableTransaction::new_from_checkpoint(
        (*change_epoch_tx.unwrap_or_else(||
            panic!(
                "state-sync should have ensured that transaction with digests {:?} exists for checkpoint: {checkpoint:?}",
                digests
            )
        )).clone(),
        epoch_store.epoch(),
        *checkpoint_sequence,
    );

    assert!(change_epoch_tx
        .data()
        .intent_message()
        .value
        .is_end_of_epoch_tx());

    Some((*digests, change_epoch_tx))
}

// Given a checkpoint, filter out any already executed transactions, then return the remaining
// execution digests, transaction digests, transactions to be executed, and randomness rounds
// (if any) included in the checkpoint.
#[allow(clippy::type_complexity)]
fn get_unexecuted_transactions(
    checkpoint: VerifiedCheckpoint,
    cache_reader: &dyn TransactionCacheRead,
    checkpoint_store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
) -> (
    Vec<ExecutionDigests>,
    Vec<TransactionDigest>,
    Vec<(VerifiedExecutableTransaction, TransactionEffectsDigest)>,
    Vec<RandomnessRound>,
) {
    let checkpoint_sequence = checkpoint.sequence_number();
    let full_contents = checkpoint_store
        .get_full_checkpoint_contents_by_sequence_number(*checkpoint_sequence)
        .expect("Failed to get checkpoint contents from store")
        .tap_some(|_| {
            debug!("loaded full checkpoint contents in bulk for sequence {checkpoint_sequence}")
        });

    let mut execution_digests = checkpoint_store
        .get_checkpoint_contents(&checkpoint.content_digest)
        .expect("Failed to get checkpoint contents from store")
        .unwrap_or_else(|| {
            panic!(
                "Checkpoint contents for digest {:?} does not exist",
                checkpoint.content_digest
            )
        })
        .into_inner();

    let full_contents_txns = full_contents.map(|c| {
        c.into_iter()
            .zip(execution_digests.iter())
            .map(|(txn, digests)| (digests.transaction, txn))
            .collect::<HashMap<_, _>>()
    });

    // Remove the change epoch transaction so that we can special case its execution.
    checkpoint.end_of_epoch_data.as_ref().tap_some(|_| {
        let digests = execution_digests
            .pop()
            .expect("Final checkpoint must have at least one transaction");

        let change_epoch_tx = cache_reader
            .get_transaction_block(&digests.transaction)
            .unwrap_or_else(||
                panic!(
                    "state-sync should have ensured that transaction with digests {digests:?} exists for checkpoint: {}",
                    checkpoint.sequence_number()
                )
            );
        assert!(change_epoch_tx.data().intent_message().value.is_end_of_epoch_tx());
    });

    let randomness_rounds = if let Some(version_specific_data) = checkpoint
        .version_specific_data(epoch_store.protocol_config())
        .expect("unable to get verison_specific_data")
    {
        // With version-specific data, randomness rounds are stored in checkpoint summary.
        version_specific_data.into_v1().randomness_rounds
    } else {
        // Before version-specific data, checkpoint batching must be disabled. In this case,
        // randomness state update tx must be first if it exists, because all other
        // transactions in a checkpoint that includes a randomness state update are causally
        // dependent on it.
        assert_eq!(
            0,
            epoch_store
                .protocol_config()
                .min_checkpoint_interval_ms_as_option()
                .unwrap_or_default(),
        );
        if let Some(first_digest) = execution_digests.first() {
            let maybe_randomness_tx = cache_reader.get_transaction_block(&first_digest.transaction)
            .unwrap_or_else(||
                panic!(
                    "state-sync should have ensured that transaction with digests {first_digest:?} exists for checkpoint: {}",
                    checkpoint.sequence_number()
                )
            );
            if let TransactionKind::RandomnessStateUpdate(rsu) =
                maybe_randomness_tx.data().transaction_data().kind()
            {
                vec![rsu.randomness_round]
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    };

    let all_tx_digests: Vec<TransactionDigest> =
        execution_digests.iter().map(|tx| tx.transaction).collect();

    let executed_effects_digests = cache_reader.multi_get_executed_effects_digests(&all_tx_digests);

    let (unexecuted_txns, expected_effects_digests): (Vec<_>, Vec<_>) =
        izip!(execution_digests.iter(), executed_effects_digests.iter())
            .filter_map(|(digests, effects_digest)| match effects_digest {
                None => Some((digests.transaction, digests.effects)),
                Some(actual_effects_digest) => {
                    let tx_digest = &digests.transaction;
                    let effects_digest = &digests.effects;
                    trace!(
                        "Transaction with digest {:?} has already been executed",
                        tx_digest
                    );
                    assert_not_forked(
                        &checkpoint,
                        tx_digest,
                        effects_digest,
                        actual_effects_digest,
                        cache_reader,
                    );
                    None
                }
            })
            .unzip();

    // read remaining unexecuted transactions from store
    let executable_txns: Vec<_> = if let Some(full_contents_txns) = full_contents_txns {
        unexecuted_txns
            .into_iter()
            .zip(expected_effects_digests)
            .map(|(tx_digest, expected_effects_digest)| {
                let tx = &full_contents_txns.get(&tx_digest).unwrap().transaction;
                (
                    VerifiedExecutableTransaction::new_from_checkpoint(
                        VerifiedTransaction::new_unchecked(tx.clone()),
                        epoch_store.epoch(),
                        *checkpoint_sequence,
                    ),
                    expected_effects_digest,
                )
            })
            .collect()
    } else {
        cache_reader
            .multi_get_transaction_blocks(&unexecuted_txns)
            .into_iter()
            .zip(expected_effects_digests)
            .enumerate()
            .map(|(i, (tx, expected_effects_digest))| {
                let tx = tx.unwrap_or_else(||
                    fatal!(
                        "state-sync should have ensured that transaction with digest {:?} exists for checkpoint: {checkpoint:?}",
                        unexecuted_txns[i]
                    )
                );
                // change epoch tx is handled specially in check_epoch_last_checkpoint
                assert!(!tx.data().intent_message().value.is_end_of_epoch_tx());
                (
                    VerifiedExecutableTransaction::new_from_checkpoint(
                        Arc::try_unwrap(tx).unwrap_or_else(|tx| (*tx).clone()),
                        epoch_store.epoch(),
                        *checkpoint_sequence,
                    ),
                    expected_effects_digest
                )
            })
            .collect()
    };

    (
        execution_digests,
        all_tx_digests,
        executable_txns,
        randomness_rounds,
    )
}

// Logs within the function are annotated with the checkpoint sequence number and epoch,
// from schedule_checkpoint().
#[instrument(level = "debug", skip_all)]
async fn execute_transactions(
    execution_digests: Vec<ExecutionDigests>,
    all_tx_digests: Vec<TransactionDigest>,
    executable_txns: Vec<(VerifiedExecutableTransaction, TransactionEffectsDigest)>,
    state: &AuthorityState,
    object_cache_reader: &dyn ObjectCacheRead,
    transaction_cache_reader: &dyn TransactionCacheRead,
    checkpoint_store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    transaction_manager: Arc<TransactionManager>,
    state_accumulator: Arc<StateAccumulator>,
    local_execution_timeout_sec: u64,
    checkpoint: VerifiedCheckpoint,
    metrics: &Arc<CheckpointExecutorMetrics>,
    prepare_start: Instant,
    data_ingestion_dir: Option<PathBuf>,
    subscription_service_enabled: bool,
) -> SuiResult<(Option<Accumulator>, Option<CheckpointData>)> {
    let effects_digests: HashMap<_, _> = execution_digests
        .iter()
        .map(|digest| (digest.transaction, digest.effects))
        .collect();

    let shared_effects_digests = executable_txns
        .iter()
        .filter(|(tx, _)| tx.contains_shared_object())
        .map(|(tx, _)| {
            *effects_digests
                .get(tx.digest())
                .expect("Transaction digest not found in effects_digests")
        })
        .collect::<Vec<_>>();

    let digest_to_effects: HashMap<TransactionDigest, TransactionEffects> =
        transaction_cache_reader
            .multi_get_effects(&shared_effects_digests)
            .into_iter()
            .zip(shared_effects_digests)
            .map(|(fx, fx_digest)| {
                if fx.is_none() {
                    panic!(
                        "Transaction effects for effects digest {:?} do not exist in effects table",
                        fx_digest
                    );
                }
                let fx = fx.unwrap();
                (*fx.transaction_digest(), fx)
            })
            .collect();

    for (tx, _) in &executable_txns {
        if tx.contains_shared_object() {
            epoch_store.acquire_shared_version_assignments_from_effects(
                tx,
                digest_to_effects.get(tx.digest()).unwrap(),
                object_cache_reader,
            )?;
        }
    }

    let prepare_elapsed = prepare_start.elapsed();
    metrics
        .checkpoint_prepare_latency
        .observe(prepare_elapsed.as_secs_f64());
    if checkpoint.sequence_number % CHECKPOINT_PROGRESS_LOG_COUNT_INTERVAL == 0 {
        info!(
            "Checkpoint preparation for execution took {:?}",
            prepare_elapsed
        );
    }

    let exec_start = Instant::now();
    transaction_manager.enqueue_with_expected_effects_digest(executable_txns.clone(), &epoch_store);

    let (checkpoint_acc, checkpoint_data) = handle_execution_effects(
        state,
        execution_digests,
        all_tx_digests,
        checkpoint.clone(),
        checkpoint_store,
        object_cache_reader,
        transaction_cache_reader,
        epoch_store,
        transaction_manager,
        state_accumulator,
        local_execution_timeout_sec,
        data_ingestion_dir,
        subscription_service_enabled,
    )
    .await;

    let exec_elapsed = exec_start.elapsed();
    metrics
        .checkpoint_exec_latency
        .observe(exec_elapsed.as_secs_f64());
    if checkpoint.sequence_number % CHECKPOINT_PROGRESS_LOG_COUNT_INTERVAL == 0 {
        info!("Checkpoint execution took {:?}", exec_elapsed);
    }

    Ok((checkpoint_acc, checkpoint_data))
}

#[instrument(level = "info", skip_all, fields(seq = ?checkpoint.sequence_number(), epoch = ?epoch_store.epoch()))]
async fn finalize_checkpoint(
    state: &AuthorityState,
    object_cache_reader: &dyn ObjectCacheRead,
    transaction_cache_reader: &dyn TransactionCacheRead,
    checkpoint_store: Arc<CheckpointStore>,
    tx_digests: &[TransactionDigest],
    epoch_store: &Arc<AuthorityPerEpochStore>,
    checkpoint: VerifiedCheckpoint,
    accumulator: Arc<StateAccumulator>,
    effects: Vec<TransactionEffects>,
    data_ingestion_dir: Option<PathBuf>,
    subscription_service_enabled: bool,
) -> SuiResult<(Accumulator, Option<CheckpointData>)> {
    debug!("finalizing checkpoint");
    epoch_store.insert_finalized_transactions(tx_digests, checkpoint.sequence_number)?;

    if state.is_fullnode(epoch_store) {
        state.congestion_tracker.process_checkpoint_effects(
            transaction_cache_reader,
            &checkpoint,
            &effects,
        );
    }

    // TODO remove once we no longer need to support this table for read RPC
    state
        .get_checkpoint_cache()
        .deprecated_insert_finalized_transactions(
            tx_digests,
            epoch_store.epoch(),
            checkpoint.sequence_number,
        );

    let checkpoint_acc =
        accumulator.accumulate_checkpoint(&effects, checkpoint.sequence_number, epoch_store)?;

    let checkpoint_data = if subscription_service_enabled
        || state.rpc_index.is_some()
        || data_ingestion_dir.is_some()
    {
        let checkpoint_data = load_checkpoint_data(
            checkpoint,
            object_cache_reader,
            transaction_cache_reader,
            checkpoint_store,
            tx_digests,
        )?;

        // Index the checkpoint. this is done out of order and is not written and committed to the
        // DB until later (committing must be done in-order)
        if let Some(rpc_index) = &state.rpc_index {
            let mut layout_resolver = epoch_store.executor().type_layout_resolver(Box::new(
                PackageStoreWithFallback::new(state.get_backing_package_store(), &checkpoint_data),
            ));

            rpc_index.index_checkpoint(&checkpoint_data, layout_resolver.as_mut());
        }

        if let Some(path) = data_ingestion_dir {
            store_checkpoint_locally(path, &checkpoint_data)?;
        }

        Some(checkpoint_data)
    } else {
        None
    };

    Ok((checkpoint_acc, checkpoint_data))
}
