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

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use futures::stream::FuturesOrdered;
use itertools::izip;
use mysten_metrics::{spawn_monitored_task, MonitoredFutureExt};
use prometheus::Registry;
use sui_config::node::CheckpointExecutorConfig;
use sui_macros::{fail_point, fail_point_async};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::message_envelope::Message;
use sui_types::messages::VerifiedExecutableTransaction;
use sui_types::{
    base_types::{ExecutionDigests, TransactionDigest, TransactionEffectsDigest},
    messages::VerifiedTransaction,
    messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint},
};
use sui_types::{error::SuiResult, messages::TransactionDataAPI};
use tap::{TapFallible, TapOptional};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    task::JoinHandle,
    time::timeout,
};
use tokio_stream::StreamExt;
use tracing::{debug, error, info, instrument, trace, warn};
use typed_store::Map;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::AuthorityStore;
use crate::state_accumulator::StateAccumulator;
use crate::transaction_manager::TransactionManager;
use crate::{authority::EffectsNotifyRead, checkpoints::CheckpointStore};

use self::metrics::CheckpointExecutorMetrics;

mod metrics;
#[cfg(test)]
pub(crate) mod tests;

type CheckpointExecutionBuffer = FuturesOrdered<JoinHandle<VerifiedCheckpoint>>;

/// The interval to log checkpoint progress, in # of checkpoints processed.
const CHECKPOINT_PROGRESS_LOG_COUNT_INTERVAL: u64 = 5000;

pub struct CheckpointExecutor {
    mailbox: broadcast::Receiver<VerifiedCheckpoint>,
    checkpoint_store: Arc<CheckpointStore>,
    authority_store: Arc<AuthorityStore>,
    tx_manager: Arc<TransactionManager>,
    accumulator: Arc<StateAccumulator>,
    config: CheckpointExecutorConfig,
    metrics: Arc<CheckpointExecutorMetrics>,
}

impl CheckpointExecutor {
    pub fn new(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        authority_store: Arc<AuthorityStore>,
        tx_manager: Arc<TransactionManager>,
        accumulator: Arc<StateAccumulator>,
        config: CheckpointExecutorConfig,
        prometheus_registry: &Registry,
    ) -> Self {
        Self {
            mailbox,
            checkpoint_store,
            authority_store,
            tx_manager,
            accumulator,
            config,
            metrics: CheckpointExecutorMetrics::new(prometheus_registry),
        }
    }

    pub fn new_for_tests(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        authority_store: Arc<AuthorityStore>,
        tx_manager: Arc<TransactionManager>,
        accumulator: Arc<StateAccumulator>,
    ) -> Self {
        Self {
            mailbox,
            checkpoint_store,
            authority_store,
            tx_manager,
            accumulator,
            config: Default::default(),
            metrics: CheckpointExecutorMetrics::new_for_tests(),
        }
    }

    /// Ensure that all checkpoints in the current epoch will be executed.
    /// We don't technically need &mut on self, but passing it to make sure only one instance is
    /// running at one time.
    pub async fn run_epoch(&mut self, epoch_store: Arc<AuthorityPerEpochStore>) {
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

        loop {
            // If we have executed the last checkpoint of the current epoch, stop.
            if self
                .check_epoch_last_checkpoint(epoch_store.clone(), &highest_executed)
                .await
            {
                // be extra careful to ensure we don't have orphans
                assert!(
                    pending.is_empty(),
                    "Pending checkpoint execution buffer should be empty after processing last checkpoint of epoch",
                );
                fail_point_async!("crash");
                return;
            }
            self.schedule_synced_checkpoints(
                &mut pending,
                // next_to_schedule will be updated to the next checkpoint to schedule.
                // This makes sure we don't re-schedule the same checkpoint multiple times.
                &mut next_to_schedule,
                epoch_store.clone(),
            )
            .await;
            self.metrics
                .checkpoint_exec_inflight
                .set(pending.len() as i64);
            tokio::select! {
                // Check for completed workers and ratchet the highest_checkpoint_executed
                // watermark accordingly. Note that given that checkpoints are guaranteed to
                // be processed (added to FuturesOrdered) in seq_number order, using FuturesOrdered
                // guarantees that we will also ratchet the watermarks in order.
                Some(Ok(checkpoint)) = pending.next() => {
                    self.process_executed_checkpoint(&checkpoint);
                    highest_executed = Some(checkpoint);

                    // Estimate TPS every 10k transactions or 30 sec
                    let elapsed = now_time.elapsed().as_millis();
                    let current_transaction_num =  highest_executed.as_ref().map(|c| c.network_total_transactions).unwrap_or(0);
                    if current_transaction_num - now_transaction_num > 10_000 || elapsed > 30_000{
                        let tps = (1000.0 * (current_transaction_num - now_transaction_num) as f64 / elapsed as f64) as i32;
                        self.metrics.checkpoint_exec_sync_tps.set(tps as i64);
                        now_time = Instant::now();
                        now_transaction_num = current_transaction_num;
                    }

                }
                // Check for newly synced checkpoints from StateSync.
                received = self.mailbox.recv() => match received {
                    Ok(checkpoint) => {
                        debug!(
                            sequence_number = ?checkpoint.sequence_number,
                            "received checkpoint summary from state sync"
                        );
                        SystemTime::now().duration_since(checkpoint.timestamp())
                            .map(|latency|
                                self.metrics.checkpoint_contents_age_ms.report(latency.as_millis() as u64)
                            )
                            .tap_err(|err| warn!("unable to compute checkpoint age: {}", err))
                            .ok();
                    },
                    // In this case, messages in the mailbox have been overwritten
                    // as a result of lagging too far behind.
                    Err(RecvError::Lagged(num_skipped)) => {
                        debug!(
                            "Checkpoint Execution Recv channel overflowed {:?} messages",
                            num_skipped,
                        );
                    }
                    Err(RecvError::Closed) => {
                        panic!("Checkpoint Execution Sender (StateSync) closed channel unexpectedly");
                    }
                }
            }
        }
    }

    pub fn set_inconsistent_state(&self, is_inconsistent_state: bool) {
        self.metrics
            .accumulator_inconsistent_state
            .set(is_inconsistent_state as i64);
    }

    /// Post processing and plumbing after we executed a checkpoint. This function is guaranteed
    /// to be called in the order of checkpoint sequence number.
    fn process_executed_checkpoint(&self, checkpoint: &VerifiedCheckpoint) {
        // Ensure that we are not skipping checkpoints at any point
        let seq = *checkpoint.sequence_number();
        let timestamp_ms = checkpoint.timestamp_ms;
        if let Some(prev_highest) = self
            .checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
        {
            assert_eq!(prev_highest + 1, seq);
        } else {
            assert_eq!(seq, 0);
        }
        debug!("Bumping highest_executed_checkpoint watermark to {:?}", seq);
        if seq % CHECKPOINT_PROGRESS_LOG_COUNT_INTERVAL == 0 {
            info!("Finished syncing and executing checkpoint {}", seq);
        }

        fail_point!("highest-executed-checkpoint");

        // We store a fixed number of additional FullCheckpointContents after execution is complete
        // for use in state sync.
        const NUM_SAVED_FULL_CHECKPOINT_CONTENTS: u64 = 5_000;
        if seq >= NUM_SAVED_FULL_CHECKPOINT_CONTENTS {
            let prune_seq = seq - NUM_SAVED_FULL_CHECKPOINT_CONTENTS;
            let prune_checkpoint = self
                .checkpoint_store
                .get_checkpoint_by_sequence_number(prune_seq)
                .expect("Failed to fetch checkpoint")
                .expect("Failed to retrieve earlier checkpoint by sequence number");
            self.checkpoint_store
                .delete_full_checkpoint_contents(prune_seq)
                .expect("Failed to delete full checkpoint contents");
            self.checkpoint_store
                .delete_contents_digest_sequence_number_mapping(&prune_checkpoint.content_digest)
                .expect("Failed to delete contents digest -> sequence number mapping");
        }

        self.checkpoint_store
            .update_highest_executed_checkpoint(checkpoint)
            .unwrap();
        self.metrics.last_executed_checkpoint.set(seq as i64);
        self.metrics
            .last_executed_checkpoint_timestamp_ms
            .set(timestamp_ms as i64);
    }

    async fn schedule_synced_checkpoints(
        &self,
        pending: &mut CheckpointExecutionBuffer,
        next_to_schedule: &mut CheckpointSequenceNumber,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) {
        let Some(latest_synced_checkpoint) = self
            .checkpoint_store
            .get_highest_synced_checkpoint()
            .expect("Failed to read highest synced checkpoint") else {
            debug!(
                "No checkpoints to schedule, highest synced checkpoint is None",
            );
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

            self.schedule_checkpoint(checkpoint, pending, epoch_store.clone())
                .await;
            *next_to_schedule += 1;
        }
    }

    #[instrument(level = "error", skip_all, fields(seq = ?checkpoint.sequence_number(), epoch = ?epoch_store.epoch()))]
    async fn schedule_checkpoint(
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

        let epoch_store = epoch_store.clone();
        // NOTE: We can't re-enqueue out of order. Therefore we cannot allow
        // any retryable failures after enqueue. Before is ok.
        while let Err(err) = self
            .execute_checkpoint(checkpoint.clone(), epoch_store.clone(), pending)
            .await
        {
            error!(
                "Error while executing checkpoint, will retry in 1s: {:?}",
                err
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
            self.metrics.checkpoint_exec_errors.inc();
        }
    }

    // Logs within the function are annotated with the checkpoint sequence number and epoch,
    // from schedule_checkpoint().
    async fn execute_checkpoint(
        &self,
        checkpoint: VerifiedCheckpoint,
        epoch_store: Arc<AuthorityPerEpochStore>,
        pending: &mut CheckpointExecutionBuffer,
    ) -> SuiResult {
        debug!("Preparing checkpoint for execution",);
        let prepare_start = Instant::now();

        // this function must guarantee that all transactions in the checkpoint are executed before it
        // returns. This invariant is enforced in two phases:
        // - First, we filter out any already executed transactions from the checkpoint in
        //   get_unexecuted_transactions()
        // - Second, we execute all remaining transactions.

        let (execution_digests, all_tx_digests, executable_txns) = get_unexecuted_transactions(
            checkpoint.clone(),
            self.authority_store.clone(),
            self.checkpoint_store.clone(),
            epoch_store.clone(),
        );

        let tx_count = execution_digests.len();
        debug!("Number of transactions in the checkpoint: {:?}", tx_count);
        self.metrics
            .checkpoint_transaction_count
            .report(tx_count as u64);

        self.execute_transactions(
            execution_digests,
            all_tx_digests.clone(),
            executable_txns,
            epoch_store.clone(),
            checkpoint,
            pending,
            prepare_start,
        )
        .await?;
        Ok(())
    }

    // Logs within the function are annotated with the checkpoint sequence number and epoch,
    // from schedule_checkpoint().
    async fn execute_transactions(
        &self,
        execution_digests: Vec<ExecutionDigests>,
        all_tx_digests: Vec<TransactionDigest>,
        executable_txns: Vec<(VerifiedExecutableTransaction, TransactionEffectsDigest)>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint: VerifiedCheckpoint,
        pending: &mut CheckpointExecutionBuffer,
        prepare_start: Instant,
    ) -> SuiResult {
        let effects_digests: HashMap<_, _> = execution_digests
            .iter()
            .map(|digest| (digest.transaction, digest.effects))
            .collect();

        let shared_effects_digests = executable_txns
            .iter()
            .filter(|(tx, _)| tx.contains_shared_object())
            .map(|(tx, _)| {
                effects_digests
                    .get(tx.digest())
                    .expect("Transaction digest not found in effects_digests")
            });

        let digest_to_effects: HashMap<TransactionDigest, TransactionEffects> = self
            .authority_store
            .perpetual_tables
            .effects
            .multi_get(shared_effects_digests)?
            .into_iter()
            .map(|fx| {
                if fx.is_none() {
                    panic!("Transaction effects do not exist in effects table");
                }
                let fx = fx.unwrap();
                (*fx.transaction_digest(), fx)
            })
            .collect();

        for (tx, _) in &executable_txns {
            if tx.contains_shared_object() {
                epoch_store
                    .acquire_shared_locks_from_effects(
                        tx,
                        digest_to_effects.get(tx.digest()).unwrap(),
                        &self.authority_store,
                    )
                    .await?;
            }
        }

        let exec_start = Instant::now();
        let prepare_elapsed = exec_start - prepare_start;
        self.metrics
            .checkpoint_prepare_latency_us
            .report(prepare_elapsed.as_micros() as u64);
        if checkpoint.sequence_number % CHECKPOINT_PROGRESS_LOG_COUNT_INTERVAL == 0 {
            info!(
                "Checkpoint preparation for execution took {:?}",
                prepare_elapsed
            );
        }

        self.tx_manager
            .enqueue_with_expected_effects_digest(executable_txns.clone(), &epoch_store)?;

        let local_execution_timeout_sec = self.config.local_execution_timeout_sec;
        let checkpoint_store = self.checkpoint_store.clone();
        let authority_store = self.authority_store.clone();
        let tx_manager = self.tx_manager.clone();
        let accumulator = self.accumulator.clone();
        let metrics = self.metrics.clone();
        pending.push_back(spawn_monitored_task!(async move {
            handle_execution_effects(
                execution_digests,
                all_tx_digests,
                checkpoint.clone(),
                checkpoint_store,
                authority_store,
                epoch_store,
                tx_manager,
                accumulator,
                local_execution_timeout_sec,
            )
            .await;
            let exec_elapsed = exec_start.elapsed();
            metrics
                .checkpoint_exec_latency_us
                .report(exec_elapsed.as_micros() as u64);
            if checkpoint.sequence_number % CHECKPOINT_PROGRESS_LOG_COUNT_INTERVAL == 0 {
                info!(seq = ?checkpoint.sequence_number(), "Checkpoint execution took {:?}", exec_elapsed);
            }
            checkpoint
        }));

        Ok(())
    }

    async fn execute_change_epoch_tx(
        &self,
        execution_digests: ExecutionDigests,
        change_epoch_tx_digest: TransactionDigest,
        change_epoch_tx: VerifiedExecutableTransaction,
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint: VerifiedCheckpoint,
    ) {
        let change_epoch_fx = self
            .authority_store
            .perpetual_tables
            .effects
            .get(&execution_digests.effects)
            .expect("Fetching effects for change_epoch tx cannot fail")
            .expect("Change_epoch tx effects must exist");

        if change_epoch_tx.contains_shared_object() {
            epoch_store
                .acquire_shared_locks_from_effects(
                    &change_epoch_tx,
                    &change_epoch_fx,
                    &self.authority_store,
                )
                .await
                .expect("Acquiring shared locks for change_epoch tx cannot fail");
        }

        self.tx_manager
            .enqueue_with_expected_effects_digest(
                vec![(change_epoch_tx.clone(), execution_digests.effects)],
                &epoch_store,
            )
            .expect("Enqueueing change_epoch tx cannot fail");
        handle_execution_effects(
            vec![execution_digests],
            vec![change_epoch_tx_digest],
            checkpoint.clone(),
            self.checkpoint_store.clone(),
            self.authority_store.clone(),
            epoch_store.clone(),
            self.tx_manager.clone(),
            self.accumulator.clone(),
            self.config.local_execution_timeout_sec,
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
                        self.authority_store.clone(),
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
                        .authority_store
                        .notify_read_executed_effects(all_tx_digests.clone())
                        .await
                        .expect("Failed to get executed effects for finalizing checkpoint");

                    finalize_checkpoint(
                        self.authority_store.clone(),
                        &all_tx_digests,
                        epoch_store.clone(),
                        *checkpoint.sequence_number(),
                        self.accumulator.clone(),
                        effects,
                    )
                    .expect("Finalizing checkpoint cannot fail");

                    self.accumulator
                        .accumulate_epoch(
                            &cur_epoch,
                            *checkpoint.sequence_number(),
                            epoch_store.clone(),
                        )
                        .in_monitored_scope("CheckpointExecutor::accumulate_epoch")
                        .await
                        .expect("Accumulating epoch cannot fail");

                    return true;
                }
            }
        }
        false
    }
}

#[instrument(level = "error", skip_all, fields(seq = ?checkpoint.sequence_number(), epoch = ?epoch_store.epoch()))]
async fn handle_execution_effects(
    execution_digests: Vec<ExecutionDigests>,
    all_tx_digests: Vec<TransactionDigest>,
    checkpoint: VerifiedCheckpoint,
    checkpoint_store: Arc<CheckpointStore>,
    authority_store: Arc<AuthorityStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    transaction_manager: Arc<TransactionManager>,
    accumulator: Arc<StateAccumulator>,
    local_execution_timeout_sec: u64,
) {
    // Once synced_txns have been awaited, all txns should have effects committed.
    let mut periods = 1;
    let log_timeout_sec = Duration::from_secs(local_execution_timeout_sec);
    // Whether the checkpoint is next to execute and blocking additional executions.
    let mut blocking_execution = false;
    loop {
        let effects_future = authority_store.notify_read_executed_effects(all_tx_digests.clone());

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
                let missing_digests: Vec<TransactionDigest> = authority_store
                    .multi_get_executed_effects_digests(&all_tx_digests)
                    .expect("multi_get_executed_effects cannot fail")
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
            Ok(Err(err)) => panic!("Failed to notify_read_executed_effects: {:?}", err),
            Ok(Ok(effects)) => {
                for (tx_digest, expected_digest, actual_effects) in
                    izip!(&all_tx_digests, &execution_digests, &effects)
                {
                    let expected_effects_digest = &expected_digest.effects;
                    assert_not_forked(
                        &checkpoint,
                        tx_digest,
                        expected_effects_digest,
                        &actual_effects.digest(),
                        authority_store.clone(),
                    );
                }

                // return Ok(effects);

                // if end of epoch checkpoint, we must finalize the checkpoint after executing
                // the change epoch tx, which is done after all other checkpoint execution
                if checkpoint.end_of_epoch_data.is_none() {
                    finalize_checkpoint(
                        authority_store.clone(),
                        &all_tx_digests,
                        epoch_store.clone(),
                        *checkpoint.sequence_number(),
                        accumulator.clone(),
                        effects,
                    )
                    .expect("Finalizing checkpoint cannot fail");
                }
                return;
            }
        }
    }
}

fn assert_not_forked(
    checkpoint: &VerifiedCheckpoint,
    tx_digest: &TransactionDigest,
    expected_digest: &TransactionEffectsDigest,
    actual_effects_digest: &TransactionEffectsDigest,
    authority_store: Arc<AuthorityStore>,
) {
    if *expected_digest != *actual_effects_digest {
        let actual_effects = authority_store
            .get_executed_effects(tx_digest)
            .expect("get_executed_effects cannot fail")
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
    authority_store: Arc<AuthorityStore>,
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

    let change_epoch_tx = authority_store
        .get_transaction_block(&digests.transaction)
        .expect("read cannot fail");

    let change_epoch_tx = VerifiedExecutableTransaction::new_from_checkpoint(
        change_epoch_tx.unwrap_or_else(||
            panic!(
                "state-sync should have ensured that transaction with digest {:?} exists for checkpoint: {checkpoint:?}",
                digests.transaction,
            )
        ),
        epoch_store.epoch(),
        *checkpoint_sequence,
    );

    assert!(change_epoch_tx
        .data()
        .intent_message()
        .value
        .is_change_epoch_tx());

    Some((*digests, change_epoch_tx))
}

// Given a checkpoint, filter out any already executed transactions, then return the remaining
// execution digests, transaction digests, and transactions to be executed.
fn get_unexecuted_transactions(
    checkpoint: VerifiedCheckpoint,
    authority_store: Arc<AuthorityStore>,
    checkpoint_store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
) -> (
    Vec<ExecutionDigests>,
    Vec<TransactionDigest>,
    Vec<(VerifiedExecutableTransaction, TransactionEffectsDigest)>,
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
        let change_epoch_tx_digest = execution_digests
            .pop()
            .expect("Final checkpoint must have at least one transaction")
            .transaction;

        let change_epoch_tx = authority_store
            .get_transaction_block(&change_epoch_tx_digest)
            .expect("read cannot fail")
            .unwrap_or_else(||
                panic!(
                    "state-sync should have ensured that transaction with digest {:?} exists for checkpoint: {}",
                    change_epoch_tx_digest, checkpoint.sequence_number()
                )
            );
        assert!(change_epoch_tx.data().intent_message().value.is_change_epoch_tx());
    });

    let all_tx_digests: Vec<TransactionDigest> =
        execution_digests.iter().map(|tx| tx.transaction).collect();

    let executed_effects_digests = authority_store
        .multi_get_executed_effects_digests(&all_tx_digests)
        .expect("failed to read executed_effects from store");

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
                        authority_store.clone(),
                    );
                    None
                }
            })
            .unzip();

    // read remaining unexecuted transactions from store
    let executable_txns: Vec<_> = if let Some(full_contents_txns) = full_contents_txns {
        unexecuted_txns
            .into_iter()
            .zip(expected_effects_digests.into_iter())
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
        authority_store
            .multi_get_transaction_blocks(&unexecuted_txns)
            .expect("Failed to get checkpoint txes from store")
            .into_iter()
            .zip(expected_effects_digests.into_iter())
            .enumerate()
            .map(|(i, (tx, expected_effects_digest))| {
                let tx = tx.unwrap_or_else(||
                    panic!(
                        "state-sync should have ensured that transaction with digest {:?} exists for checkpoint: {checkpoint:?}",
                        unexecuted_txns[i]
                    )
                );
                // change epoch tx is handled specially in check_epoch_last_checkpoint
                assert!(!tx.data().intent_message().value.is_change_epoch_tx());
                (
                    VerifiedExecutableTransaction::new_from_checkpoint(
                        tx,
                        epoch_store.epoch(),
                        *checkpoint_sequence,
                    ),
                    expected_effects_digest
                )
            })
            .collect()
    };

    (execution_digests, all_tx_digests, executable_txns)
}

fn finalize_checkpoint(
    authority_store: Arc<AuthorityStore>,
    tx_digests: &[TransactionDigest],
    epoch_store: Arc<AuthorityPerEpochStore>,
    checkpoint_sequence: u64,
    accumulator: Arc<StateAccumulator>,
    effects: Vec<TransactionEffects>,
) -> SuiResult {
    authority_store.insert_finalized_transactions(
        tx_digests,
        epoch_store.epoch(),
        checkpoint_sequence,
    )?;
    accumulator.accumulate_checkpoint(effects, checkpoint_sequence, epoch_store)?;
    Ok(())
}
