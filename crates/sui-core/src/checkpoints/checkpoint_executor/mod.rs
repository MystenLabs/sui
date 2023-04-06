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
use sui_types::message_envelope::Message;
use sui_types::messages::VerifiedExecutableTransaction;
use sui_types::{
    base_types::{ExecutionDigests, TransactionDigest, TransactionEffectsDigest},
    messages::{TransactionEffects, TransactionEffectsAPI},
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
            );
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
        if seq % 10000 == 0 {
            info!("Finished syncing and executing checkpoint {}", seq);
        }

        fail_point!("highest-executed-checkpoint");

        self.checkpoint_store
            .update_highest_executed_checkpoint(checkpoint)
            .unwrap();
        self.metrics.last_executed_checkpoint.set(seq as i64);
    }

    fn schedule_synced_checkpoints(
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

            self.schedule_checkpoint(checkpoint, pending, epoch_store.clone());
            *next_to_schedule += 1;
        }
    }

    fn schedule_checkpoint(
        &self,
        checkpoint: VerifiedCheckpoint,
        pending: &mut CheckpointExecutionBuffer,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) {
        debug!("Executing checkpoint {:?}", checkpoint.sequence_number());

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
        let authority_store = self.authority_store.clone();
        let checkpoint_store = self.checkpoint_store.clone();
        let tx_manager = self.tx_manager.clone();
        let accumulator = self.accumulator.clone();

        pending.push_back(spawn_monitored_task!(async move {
            let epoch_store = epoch_store.clone();
            while let Err(err) = execute_checkpoint(
                checkpoint.clone(),
                authority_store.clone(),
                checkpoint_store.clone(),
                epoch_store.clone(),
                tx_manager.clone(),
                accumulator.clone(),
                local_execution_timeout_sec,
                &metrics,
            )
            .await
            {
                error!(
                    "Error while executing checkpoint, will retry in 1s: {:?}",
                    err
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
                metrics.checkpoint_exec_errors.inc();
            }
            checkpoint
        }));
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

                    let change_epoch_effects = execute_transactions(
                        vec![change_epoch_execution_digests],
                        vec![change_epoch_tx_digest],
                        vec![change_epoch_tx],
                        self.authority_store.clone(),
                        epoch_store.clone(),
                        self.tx_manager.clone(),
                        self.config.local_execution_timeout_sec,
                        checkpoint.clone(),
                    )
                    .await
                    .expect("Executing change_epoch tx cannot fail");
                    assert_eq!(change_epoch_effects.len(), 1);

                    // verify change_epoch tx effects digest
                    assert_eq!(
                        change_epoch_execution_digests.effects,
                        change_epoch_effects[0].digest(),
                        "change_epoch tx effects digest mismatch"
                    );

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
async fn execute_checkpoint(
    checkpoint: VerifiedCheckpoint,
    authority_store: Arc<AuthorityStore>,
    checkpoint_store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    transaction_manager: Arc<TransactionManager>,
    accumulator: Arc<StateAccumulator>,
    local_execution_timeout_sec: u64,
    metrics: &Arc<CheckpointExecutorMetrics>,
) -> SuiResult {
    let checkpoint_sequence = *checkpoint.sequence_number();
    debug!(
        "Scheduling checkpoint {:?} for execution",
        checkpoint_sequence,
    );

    // this function must guarantee that all transactions in the checkpoint are executed before it
    // returns. This invariant is enforced in two phases:
    // - First, we filter out any already executed transactions from the checkpoint in
    //   get_unexecuted_transactions()
    // - Second, we execute all remaining transactions.

    let (execution_digests, all_tx_digests, executable_txns) = get_unexecuted_transactions(
        checkpoint.clone(),
        authority_store.clone(),
        checkpoint_store.clone(),
        epoch_store.clone(),
    );

    let tx_count = execution_digests.len();
    debug!(
        epoch=?epoch_store.epoch(),
        checkpoint_sequence=?checkpoint.sequence_number(),
        "Number of transactions in the checkpoint: {:?}",
        tx_count
    );
    metrics.checkpoint_transaction_count.report(tx_count as u64);

    let end_of_epoch = checkpoint.end_of_epoch_data.is_some();

    let effects = execute_transactions(
        execution_digests,
        all_tx_digests.clone(),
        executable_txns,
        authority_store.clone(),
        epoch_store.clone(),
        transaction_manager,
        local_execution_timeout_sec,
        checkpoint,
    )
    .await?;

    // if end of epoch checkpoint, we must finalize the checkpoint after executing
    // the change epoch tx, which is done after all other checkpoint execution
    if !end_of_epoch {
        finalize_checkpoint(
            authority_store,
            &all_tx_digests,
            epoch_store,
            checkpoint_sequence,
            accumulator,
            effects,
        )?;
    }
    Ok(())
}

fn assert_not_forked(
    checkpoint: &VerifiedCheckpoint,
    tx_digest: &TransactionDigest,
    expected_digest: &TransactionEffectsDigest,
    actual_effects: &TransactionEffects,
) {
    if *expected_digest != actual_effects.digest() {
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
            actual_effects.digest(),
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
    Vec<VerifiedExecutableTransaction>,
) {
    let checkpoint_sequence = checkpoint.sequence_number();
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

    let executed_effects = authority_store
        .multi_get_executed_effects(&all_tx_digests)
        .expect("failed to read executed_effects from store");

    let unexecuted_txns: Vec<_> = izip!(execution_digests.iter(), executed_effects.iter())
        .filter_map(|(digests, effects)| match effects {
            None => Some(digests.transaction),
            Some(actual_effects) => {
                let tx_digest = &digests.transaction;
                let effects_digest = &digests.effects;
                trace!(
                    "Transaction with digest {:?} has already been executed",
                    tx_digest
                );
                assert_not_forked(&checkpoint, tx_digest, effects_digest, actual_effects);
                None
            }
        })
        .collect();

    // read remaining unexecuted transactions from store
    let executable_txns: Vec<_> = authority_store
        .multi_get_transaction_blocks(&unexecuted_txns)
        .expect("Failed to get checkpoint txes from store")
        .into_iter()
        .enumerate()
        .map(|(i, tx)| {
            let tx = tx.unwrap_or_else(||
                panic!(
                    "state-sync should have ensured that transaction with digest {:?} exists for checkpoint: {checkpoint:?}",
                    unexecuted_txns[i]
                )
            );
            // change epoch tx is handled specially in check_epoch_last_checkpoint
            assert!(!tx.data().intent_message().value.is_change_epoch_tx());
            VerifiedExecutableTransaction::new_from_checkpoint(
                tx,
                epoch_store.epoch(),
                *checkpoint_sequence,
            )
        })
        .collect();

    (execution_digests, all_tx_digests, executable_txns)
}

async fn execute_transactions(
    execution_digests: Vec<ExecutionDigests>,
    all_tx_digests: Vec<TransactionDigest>,
    executable_txns: Vec<VerifiedExecutableTransaction>,
    authority_store: Arc<AuthorityStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    transaction_manager: Arc<TransactionManager>,
    log_timeout_sec: u64,
    checkpoint: VerifiedCheckpoint,
) -> SuiResult<Vec<TransactionEffects>> {
    let effects_digests = execution_digests.iter().map(|digest| digest.effects);

    let digest_to_effects: HashMap<TransactionDigest, TransactionEffects> = authority_store
        .perpetual_tables
        .effects
        .multi_get(effects_digests)?
        .into_iter()
        .map(|fx| {
            if fx.is_none() {
                panic!("Transaction effects do not exist in effects table");
            }
            let fx = fx.unwrap();
            (*fx.transaction_digest(), fx)
        })
        .collect();

    for tx in &executable_txns {
        if tx.contains_shared_object() {
            epoch_store
                .acquire_shared_locks_from_effects(
                    tx,
                    digest_to_effects.get(tx.digest()).unwrap(),
                    &authority_store,
                )
                .await?;
        }
    }

    transaction_manager.enqueue(executable_txns, &epoch_store)?;

    // Once synced_txns have been awaited, all txns should have effects committed.
    let mut periods = 1;
    let log_timeout_sec = Duration::from_secs(log_timeout_sec);

    loop {
        let effects_future = authority_store.notify_read_executed_effects(all_tx_digests.clone());

        match timeout(log_timeout_sec, effects_future).await {
            Err(_elapsed) => {
                let missing_digests: Vec<TransactionDigest> = authority_store
                    .multi_get_executed_effects(&all_tx_digests)?
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
                let missing_input = transaction_manager.get_missing_input(pending_digest);
                let pending_transaction = authority_store
                    .get_transaction_block(pending_digest)?
                    .expect("state-sync should have ensured that the transaction exists");

                warn!(
                    "Transaction {pending_digest:?} has missing input objects {missing_input:?}\
                    \nTransaction input: {:?}\nTransaction content: {:?}",
                    pending_transaction
                        .data()
                        .intent_message()
                        .value
                        .input_objects(),
                    pending_transaction,
                );
                periods += 1;
            }
            Ok(Err(err)) => return Err(err),
            Ok(Ok(effects)) => {
                for (tx_digest, expected_digest, actual_effects) in
                    izip!(&all_tx_digests, &execution_digests, &effects)
                {
                    let expected_effects_digest = &expected_digest.effects;
                    assert_not_forked(
                        &checkpoint,
                        tx_digest,
                        expected_effects_digest,
                        actual_effects,
                    );
                }
                return Ok(effects);
            }
        }
    }
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
