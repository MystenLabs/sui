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

use futures::StreamExt;
use mysten_common::{debug_fatal, fatal};
use parking_lot::Mutex;
use std::{sync::Arc, time::Instant};
use sui_types::crypto::RandomnessRound;
use sui_types::inner_temporary_store::PackageStoreWithFallback;
use sui_types::messages_checkpoint::{CheckpointContents, CheckpointSequenceNumber};
use sui_types::transaction::{TransactionDataAPI, TransactionKind};

use sui_config::node::{CheckpointExecutorConfig, RunWithRange};
use sui_macros::fail_point;
use sui_types::accumulator::Accumulator;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::message_envelope::Message;
use sui_types::{
    base_types::{TransactionDigest, TransactionEffectsDigest},
    messages_checkpoint::VerifiedCheckpoint,
    transaction::VerifiedTransaction,
};
use tap::{TapFallible, TapOptional};
use tracing::{debug, info, instrument, warn};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::backpressure::BackpressureManager;
use crate::authority::AuthorityState;
use crate::execution_scheduler::ExecutionScheduler;
use crate::state_accumulator::StateAccumulator;
use crate::{
    checkpoints::CheckpointStore,
    execution_cache::{ObjectCacheRead, TransactionCacheRead},
};

mod data_ingestion_handler;
pub mod metrics;
pub(crate) mod utils;

use data_ingestion_handler::{load_checkpoint_data, store_checkpoint_locally};
use metrics::CheckpointExecutorMetrics;
use utils::*;

const CHECKPOINT_PROGRESS_LOG_COUNT_INTERVAL: u64 = 5000;

#[derive(PartialEq, Eq, Debug)]
pub enum StopReason {
    EpochComplete,
    RunWithRangeCondition,
}

pub(crate) struct CheckpointExecutionData {
    pub checkpoint: VerifiedCheckpoint,
    pub checkpoint_contents: CheckpointContents,
    pub tx_digests: Vec<TransactionDigest>,
    pub fx_digests: Vec<TransactionEffectsDigest>,
}

pub(crate) struct CheckpointTransactionData {
    pub transactions: Vec<VerifiedExecutableTransaction>,
    pub effects: Vec<TransactionEffects>,
    pub executed_fx_digests: Vec<Option<TransactionEffectsDigest>>,
}

pub(crate) struct CheckpointExecutionState {
    pub data: CheckpointExecutionData,

    accumulator: Option<Accumulator>,
    full_data: Option<CheckpointData>,
}

impl CheckpointExecutionState {
    pub fn new(data: CheckpointExecutionData) -> Self {
        Self {
            data,
            accumulator: None,
            full_data: None,
        }
    }

    pub fn new_with_accumulator(data: CheckpointExecutionData, accumulator: Accumulator) -> Self {
        Self {
            data,
            accumulator: Some(accumulator),
            full_data: None,
        }
    }
}

macro_rules! finish_stage {
    ($handle:expr, $stage:ident) => {
        $handle.finish_stage(PipelineStage::$stage).await;
    };
}

pub struct CheckpointExecutor {
    epoch_store: Arc<AuthorityPerEpochStore>,
    state: Arc<AuthorityState>,
    // TODO: We should use RocksDbStore in the executor
    // to consolidate DB accesses.
    checkpoint_store: Arc<CheckpointStore>,
    object_cache_reader: Arc<dyn ObjectCacheRead>,
    transaction_cache_reader: Arc<dyn TransactionCacheRead>,
    execution_scheduler: Arc<ExecutionScheduler>,
    accumulator: Arc<StateAccumulator>,
    backpressure_manager: Arc<BackpressureManager>,
    config: CheckpointExecutorConfig,
    metrics: Arc<CheckpointExecutorMetrics>,
    tps_estimator: Mutex<TPSEstimator>,
    subscription_service_checkpoint_sender: Option<tokio::sync::mpsc::Sender<CheckpointData>>,
}

impl CheckpointExecutor {
    pub fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint_store: Arc<CheckpointStore>,
        state: Arc<AuthorityState>,
        accumulator: Arc<StateAccumulator>,
        backpressure_manager: Arc<BackpressureManager>,
        config: CheckpointExecutorConfig,
        metrics: Arc<CheckpointExecutorMetrics>,
        subscription_service_checkpoint_sender: Option<tokio::sync::mpsc::Sender<CheckpointData>>,
    ) -> Self {
        Self {
            epoch_store,
            state: state.clone(),
            checkpoint_store,
            object_cache_reader: state.get_object_cache_reader().clone(),
            transaction_cache_reader: state.get_transaction_cache_reader().clone(),
            execution_scheduler: state.execution_scheduler().clone(),
            accumulator,
            backpressure_manager,
            config,
            metrics,
            tps_estimator: Mutex::new(TPSEstimator::default()),
            subscription_service_checkpoint_sender,
        }
    }

    pub fn new_for_tests(
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint_store: Arc<CheckpointStore>,
        state: Arc<AuthorityState>,
        accumulator: Arc<StateAccumulator>,
    ) -> Self {
        Self::new(
            epoch_store,
            checkpoint_store,
            state,
            accumulator,
            BackpressureManager::new_for_tests(),
            Default::default(),
            CheckpointExecutorMetrics::new_for_tests(),
            None,
        )
    }

    // Gets the next checkpoint to schedule for execution. If the epoch is already
    // completed, returns None.
    fn get_next_to_schedule(&self) -> Option<CheckpointSequenceNumber> {
        // Decide the first checkpoint to schedule for execution.
        // If we haven't executed anything in the past, we schedule checkpoint 0.
        // Otherwise we schedule the one after highest executed.
        let highest_executed = self
            .checkpoint_store
            .get_highest_executed_checkpoint()
            .unwrap();

        if let Some(highest_executed) = &highest_executed {
            if self.epoch_store.epoch() == highest_executed.epoch()
                && highest_executed.is_last_checkpoint_of_epoch()
            {
                // We can arrive at this point if we bump the highest_executed_checkpoint watermark, and then
                // crash before completing reconfiguration.
                info!(seq = ?highest_executed.sequence_number, "final checkpoint of epoch has already been executed");
                return None;
            }
        }

        Some(
            highest_executed
                .as_ref()
                .map(|c| c.sequence_number() + 1)
                .unwrap_or_else(|| {
                    // TODO this invariant may no longer hold once we introduce snapshots
                    assert_eq!(self.epoch_store.epoch(), 0);
                    // we need to execute the genesis checkpoint
                    0
                }),
        )
    }

    /// Execute all checkpoints for the current epoch, ensuring that the node has not
    /// forked, and return when finished.
    /// If `run_with_range` is set, execution will stop early.
    #[instrument(level = "error", skip_all, fields(epoch = ?self.epoch_store.epoch()))]
    pub async fn run_epoch(self, run_with_range: Option<RunWithRange>) -> StopReason {
        let _metrics_scope = mysten_metrics::monitored_scope("CheckpointExecutor::run_epoch");
        info!(?run_with_range, "CheckpointExecutor::run_epoch");
        debug!(
            "Checkpoint executor running for epoch {:?}",
            self.epoch_store.epoch(),
        );

        // check if we want to run this epoch based on RunWithRange condition value
        // we want to be inclusive of the defined RunWithRangeEpoch::Epoch
        // i.e Epoch(N) means we will execute epoch N and stop when reaching N+1
        if run_with_range.is_some_and(|rwr| rwr.is_epoch_gt(self.epoch_store.epoch())) {
            info!("RunWithRange condition satisfied at {:?}", run_with_range,);
            return StopReason::RunWithRangeCondition;
        };

        self.metrics
            .checkpoint_exec_epoch
            .set(self.epoch_store.epoch() as i64);

        let Some(next_to_schedule) = self.get_next_to_schedule() else {
            return StopReason::EpochComplete;
        };

        let this = Arc::new(self);

        let concurrency = std::env::var("SUI_CHECKPOINT_EXECUTION_MAX_CONCURRENCY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(this.config.checkpoint_execution_max_concurrency);

        let pipeline_stages = PipelineStages::new(next_to_schedule, this.metrics.clone());

        let final_checkpoint_executed = stream_synced_checkpoints(
            this.checkpoint_store.clone(),
            next_to_schedule,
            run_with_range.and_then(|rwr| rwr.into_checkpoint_bound()),
        )
        // Checkpoint loading and execution is parallelized
        .map(|checkpoint| {
            let this = this.clone();
            let pipeline_handle = pipeline_stages.handle(*checkpoint.sequence_number());
            async move {
                let pipeline_handle = pipeline_handle.await;
                tokio::spawn(this.execute_checkpoint(checkpoint, pipeline_handle))
                    .await
                    .unwrap()
            }
        })
        .buffered(concurrency)
        // Take the last value from the stream to determine if we completed the epoch
        .fold(false, |state, is_final_checkpoint| async move {
            assert!(
                !state,
                "fold can't be called again after the final checkpoint"
            );
            is_final_checkpoint
        })
        .await;

        if final_checkpoint_executed {
            StopReason::EpochComplete
        } else {
            StopReason::RunWithRangeCondition
        }
    }
}

impl CheckpointExecutor {
    /// Load all data for a checkpoint, ensure all transactions are executed, and check for forks.
    #[instrument(level = "info", skip_all, fields(seq = ?checkpoint.sequence_number()))]
    async fn execute_checkpoint(
        self: Arc<Self>,
        checkpoint: VerifiedCheckpoint,
        mut pipeline_handle: PipelineHandle,
    ) -> bool /* is final checkpoint */ {
        info!("executing checkpoint");
        let sequence_number = checkpoint.sequence_number;

        checkpoint.report_checkpoint_age(
            &self.metrics.checkpoint_contents_age,
            &self.metrics.checkpoint_contents_age_ms,
        );
        self.backpressure_manager
            .update_highest_certified_checkpoint(sequence_number);

        if checkpoint.is_last_checkpoint_of_epoch() && sequence_number > 0 {
            let _wait_for_previous_checkpoints_guard = mysten_metrics::monitored_scope(
                "CheckpointExecutor::wait_for_previous_checkpoints",
            );

            info!("Reached end of epoch checkpoint, waiting for all previous checkpoints to be executed");
            self.checkpoint_store
                .notify_read_executed_checkpoint(sequence_number - 1)
                .await;
        }

        let _parallel_step_guard =
            mysten_metrics::monitored_scope("CheckpointExecutor::parallel_step");

        // Note: only `execute_transactions_from_synced_checkpoint` has end-of-epoch logic.
        let ckpt_state = if self.state.is_fullnode(&self.epoch_store)
            || checkpoint.is_last_checkpoint_of_epoch()
        {
            self.execute_transactions_from_synced_checkpoint(checkpoint, &mut pipeline_handle)
                .await
        } else {
            self.verify_locally_built_checkpoint(checkpoint, &mut pipeline_handle)
                .await
        };

        let tps = self.tps_estimator.lock().update(
            Instant::now(),
            ckpt_state.data.checkpoint.network_total_transactions,
        );
        self.metrics.checkpoint_exec_sync_tps.set(tps as i64);

        self.backpressure_manager
            .update_highest_executed_checkpoint(*ckpt_state.data.checkpoint.sequence_number());

        let is_final_checkpoint = ckpt_state.data.checkpoint.is_last_checkpoint_of_epoch();

        let seq = ckpt_state.data.checkpoint.sequence_number;

        let batch = self
            .state
            .get_cache_commit()
            .build_db_batch(self.epoch_store.epoch(), &ckpt_state.data.tx_digests);

        finish_stage!(pipeline_handle, BuildDbBatch);

        let mut ckpt_state = tokio::task::spawn_blocking({
            let this = self.clone();
            move || {
                // Commit all transaction effects to disk
                let cache_commit = this.state.get_cache_commit();
                debug!(?seq, "committing checkpoint transactions to disk");
                cache_commit.commit_transaction_outputs(
                    this.epoch_store.epoch(),
                    batch,
                    &ckpt_state.data.tx_digests,
                );
                ckpt_state
            }
        })
        .await
        .unwrap();

        finish_stage!(pipeline_handle, CommitTransactionOutputs);

        self.epoch_store
            .handle_finalized_checkpoint(&ckpt_state.data.checkpoint, &ckpt_state.data.tx_digests)
            .expect("cannot fail");

        // Once the checkpoint is finalized, we know that any randomness contained in this checkpoint has
        // been successfully included in a checkpoint certified by quorum of validators.
        // (RandomnessManager/RandomnessReporter is only present on validators.)
        if let Some(randomness_reporter) = self.epoch_store.randomness_reporter() {
            let randomness_rounds = self.extract_randomness_rounds(
                &ckpt_state.data.checkpoint,
                &ckpt_state.data.checkpoint_contents,
            );
            for round in randomness_rounds {
                debug!(?round, "notifying RandomnessReporter that randomness update was executed in checkpoint");
                randomness_reporter
                    .notify_randomness_in_checkpoint(round)
                    .expect("epoch cannot have ended");
            }
        }

        finish_stage!(pipeline_handle, FinalizeCheckpoint);

        if let Some(checkpoint_data) = ckpt_state.full_data.take() {
            self.commit_index_updates_and_enqueue_to_subscription_service(checkpoint_data)
                .await;
        }

        finish_stage!(pipeline_handle, UpdateRpcIndex);

        self.accumulator
            .accumulate_running_root(&self.epoch_store, seq, ckpt_state.accumulator)
            .expect("Failed to accumulate running root");

        if is_final_checkpoint {
            self.checkpoint_store
                .insert_epoch_last_checkpoint(self.epoch_store.epoch(), &ckpt_state.data.checkpoint)
                .expect("Failed to insert epoch last checkpoint");

            self.accumulator
                .accumulate_epoch(self.epoch_store.clone(), seq)
                .expect("Accumulating epoch cannot fail");

            self.checkpoint_store
                .prune_local_summaries()
                .tap_err(|e| debug_fatal!("Failed to prune local summaries: {}", e))
                .ok();
        }

        fail_point!("crash");

        self.bump_highest_executed_checkpoint(&ckpt_state.data.checkpoint);

        finish_stage!(pipeline_handle, BumpHighestExecutedCheckpoint);

        // Important: code after the last pipeline stage is finished can run out of checkpoint order.

        ckpt_state.data.checkpoint.is_last_checkpoint_of_epoch()
    }

    // On validators, checkpoints have often already been constructed locally, in which
    // case we can skip many steps of the checkpoint execution process.
    #[instrument(level = "info", skip_all)]
    async fn verify_locally_built_checkpoint(
        &self,
        checkpoint: VerifiedCheckpoint,
        pipeline_handle: &mut PipelineHandle,
    ) -> CheckpointExecutionState {
        assert!(
            !checkpoint.is_last_checkpoint_of_epoch(),
            "only fullnode path has end-of-epoch logic"
        );

        let sequence_number = checkpoint.sequence_number;
        let locally_built_checkpoint = self
            .checkpoint_store
            .get_locally_computed_checkpoint(sequence_number)
            .expect("db error");

        let Some(locally_built_checkpoint) = locally_built_checkpoint else {
            // fall back to tx-by-tx execution path if we are catching up.
            return self
                .execute_transactions_from_synced_checkpoint(checkpoint, pipeline_handle)
                .await;
        };

        self.metrics.checkpoint_executor_validator_path.inc();

        // Check for fork
        assert_checkpoint_not_forked(
            &locally_built_checkpoint,
            &checkpoint,
            &self.checkpoint_store,
        );

        // Checkpoint builder triggers accumulation of the checkpoint, so this is guaranteed to finish.
        let accumulator = {
            let _metrics_scope =
                mysten_metrics::monitored_scope("CheckpointExecutor::notify_read_accumulator");
            self.epoch_store
                .notify_read_checkpoint_state_accumulator(&[sequence_number])
                .await
                .unwrap()
                .pop()
                .unwrap()
        };

        let checkpoint_contents = self
            .checkpoint_store
            .get_checkpoint_contents(&checkpoint.content_digest)
            .expect("db error")
            .expect("checkpoint contents not found");

        let (tx_digests, fx_digests): (Vec<_>, Vec<_>) = checkpoint_contents
            .iter()
            .map(|digests| (digests.transaction, digests.effects))
            .unzip();

        pipeline_handle
            .skip_to(PipelineStage::FinalizeTransactions)
            .await;

        // Currently this code only runs on validators, where this method call does nothing.
        // But in the future, fullnodes may follow the mysticeti dag and build their own checkpoints.
        self.insert_finalized_transactions(&tx_digests, sequence_number);

        pipeline_handle.skip_to(PipelineStage::BuildDbBatch).await;

        CheckpointExecutionState::new_with_accumulator(
            CheckpointExecutionData {
                checkpoint,
                checkpoint_contents,
                tx_digests,
                fx_digests,
            },
            accumulator,
        )
    }

    #[instrument(level = "info", skip_all)]
    async fn execute_transactions_from_synced_checkpoint(
        &self,
        checkpoint: VerifiedCheckpoint,
        pipeline_handle: &mut PipelineHandle,
    ) -> CheckpointExecutionState {
        let sequence_number = checkpoint.sequence_number;
        let (mut ckpt_state, tx_data, unexecuted_tx_digests) = {
            let _scope =
                mysten_metrics::monitored_scope("CheckpointExecutor::execute_transactions");
            let (ckpt_state, tx_data) = self.load_checkpoint_transactions(checkpoint);
            let unexecuted_tx_digests = self.schedule_transaction_execution(&ckpt_state, &tx_data);
            (ckpt_state, tx_data, unexecuted_tx_digests)
        };

        finish_stage!(pipeline_handle, ExecuteTransactions);

        {
            let _metrics_scope = mysten_metrics::monitored_scope(
                "CheckpointExecutor::notify_read_executed_effects_digests",
            );
            self.transaction_cache_reader
                .notify_read_executed_effects_digests(&unexecuted_tx_digests)
                .await;
        }

        finish_stage!(pipeline_handle, WaitForTransactions);

        if ckpt_state.data.checkpoint.is_last_checkpoint_of_epoch() {
            self.execute_change_epoch_tx(&tx_data).await;
        }

        let _scope = mysten_metrics::monitored_scope("CheckpointExecutor::finalize_checkpoint");

        if self.state.is_fullnode(&self.epoch_store) {
            self.state.congestion_tracker.process_checkpoint_effects(
                &*self.transaction_cache_reader,
                &ckpt_state.data.checkpoint,
                &tx_data.effects,
            );
        }

        self.insert_finalized_transactions(&ckpt_state.data.tx_digests, sequence_number);

        // The early versions of the accumulator (prior to effectsv2) rely on db
        // state, so we must wait until all transactions have been executed
        // before accumulating the checkpoint.
        ckpt_state.accumulator = Some(
            self.accumulator
                .accumulate_checkpoint(&tx_data.effects, sequence_number, &self.epoch_store)
                .expect("epoch cannot have ended"),
        );

        finish_stage!(pipeline_handle, FinalizeTransactions);

        ckpt_state.full_data = self.process_checkpoint_data(&ckpt_state.data, &tx_data);

        finish_stage!(pipeline_handle, ProcessCheckpointData);

        ckpt_state
    }

    fn checkpoint_data_enabled(&self) -> bool {
        self.subscription_service_checkpoint_sender.is_some()
            || self.state.rpc_index.is_some()
            || self.config.data_ingestion_dir.is_some()
    }

    fn insert_finalized_transactions(
        &self,
        tx_digests: &[TransactionDigest],
        sequence_number: CheckpointSequenceNumber,
    ) {
        self.epoch_store
            .insert_finalized_transactions(tx_digests, sequence_number)
            .expect("failed to insert finalized transactions");

        if self.state.is_fullnode(&self.epoch_store) {
            // TODO remove once we no longer need to support this table for read RPC
            self.state
                .get_checkpoint_cache()
                .deprecated_insert_finalized_transactions(
                    tx_digests,
                    self.epoch_store.epoch(),
                    sequence_number,
                );
        }
    }

    #[instrument(level = "info", skip_all)]
    fn process_checkpoint_data(
        &self,
        ckpt_data: &CheckpointExecutionData,
        tx_data: &CheckpointTransactionData,
    ) -> Option<CheckpointData> {
        if !self.checkpoint_data_enabled() {
            return None;
        }

        let checkpoint_data = load_checkpoint_data(
            ckpt_data,
            tx_data,
            self.state.get_object_store(),
            &*self.transaction_cache_reader,
        )
        .expect("failed to load checkpoint data");

        if self.state.rpc_index.is_some() || self.config.data_ingestion_dir.is_some() {
            // Index the checkpoint. this is done out of order and is not written and committed to the
            // DB until later (committing must be done in-order)
            if let Some(rpc_index) = &self.state.rpc_index {
                let mut layout_resolver = self.epoch_store.executor().type_layout_resolver(
                    Box::new(PackageStoreWithFallback::new(
                        self.state.get_backing_package_store(),
                        &checkpoint_data,
                    )),
                );

                rpc_index.index_checkpoint(&checkpoint_data, layout_resolver.as_mut());
            }

            if let Some(path) = &self.config.data_ingestion_dir {
                store_checkpoint_locally(path, &checkpoint_data)
                    .expect("failed to store checkpoint locally");
            }
        }

        Some(checkpoint_data)
    }

    // Load all required transaction and effects data for the checkpoint.
    #[instrument(level = "info", skip_all)]
    fn load_checkpoint_transactions(
        &self,
        checkpoint: VerifiedCheckpoint,
    ) -> (CheckpointExecutionState, CheckpointTransactionData) {
        let seq = checkpoint.sequence_number;
        let epoch = checkpoint.epoch;

        let checkpoint_contents = self
            .checkpoint_store
            .get_checkpoint_contents(&checkpoint.content_digest)
            .expect("db error")
            .expect("checkpoint contents not found");

        // attempt to load full checkpoint contents in bulk
        // Tolerate db error in case of data corruption.
        // We will fall back to loading items one-by-one below in case of error.
        if let Some(full_contents) = self
            .checkpoint_store
            .get_full_checkpoint_contents_by_sequence_number(seq)
            .tap_err(|e| debug_fatal!("Failed to get checkpoint contents from store: {e}"))
            .ok()
            .flatten()
            .tap_some(|_| debug!("loaded full checkpoint contents in bulk for sequence {seq}"))
        {
            let num_txns = full_contents.size();
            let mut tx_digests = Vec::with_capacity(num_txns);
            let mut transactions = Vec::with_capacity(num_txns);
            let mut effects = Vec::with_capacity(num_txns);
            let mut fx_digests = Vec::with_capacity(num_txns);

            full_contents
                .into_iter()
                .zip(checkpoint_contents.iter())
                .for_each(|(execution_data, digests)| {
                    let tx_digest = digests.transaction;
                    let fx_digest = digests.effects;
                    debug_assert_eq!(tx_digest, *execution_data.transaction.digest());
                    debug_assert_eq!(fx_digest, execution_data.effects.digest());

                    tx_digests.push(tx_digest);
                    transactions.push(VerifiedExecutableTransaction::new_from_checkpoint(
                        VerifiedTransaction::new_unchecked(execution_data.transaction),
                        epoch,
                        seq,
                    ));
                    effects.push(execution_data.effects);
                    fx_digests.push(fx_digest);
                });

            let executed_fx_digests = self
                .transaction_cache_reader
                .multi_get_executed_effects_digests(&tx_digests);

            (
                CheckpointExecutionState::new(CheckpointExecutionData {
                    checkpoint,
                    checkpoint_contents,
                    tx_digests,
                    fx_digests,
                }),
                CheckpointTransactionData {
                    transactions,
                    effects,
                    executed_fx_digests,
                },
            )
        } else {
            // load items one-by-one
            // TODO: If we used RocksDbStore in the executor instead,
            // all the logic below could be removed.

            let digests = checkpoint_contents.inner();

            let (tx_digests, fx_digests): (Vec<_>, Vec<_>) =
                digests.iter().map(|d| (d.transaction, d.effects)).unzip();
            let transactions = self
                .transaction_cache_reader
                .multi_get_transaction_blocks(&tx_digests)
                .into_iter()
                .enumerate()
                .map(|(i, tx)| {
                    let tx = tx
                        .unwrap_or_else(|| fatal!("transaction not found for {:?}", tx_digests[i]));
                    let tx = Arc::try_unwrap(tx).unwrap_or_else(|tx| (*tx).clone());
                    VerifiedExecutableTransaction::new_from_checkpoint(tx, epoch, seq)
                })
                .collect();
            let effects = self
                .transaction_cache_reader
                .multi_get_effects(&fx_digests)
                .into_iter()
                .enumerate()
                .map(|(i, effect)| {
                    effect.unwrap_or_else(|| {
                        fatal!("checkpoint effect not found for {:?}", digests[i])
                    })
                })
                .collect();

            let executed_fx_digests = self
                .transaction_cache_reader
                .multi_get_executed_effects_digests(&tx_digests);

            (
                CheckpointExecutionState::new(CheckpointExecutionData {
                    checkpoint,
                    checkpoint_contents,
                    tx_digests,
                    fx_digests,
                }),
                CheckpointTransactionData {
                    transactions,
                    effects,
                    executed_fx_digests,
                },
            )
        }
    }

    // Schedule all unexecuted transactions in the checkpoint for execution
    #[instrument(level = "info", skip_all)]
    fn schedule_transaction_execution(
        &self,
        ckpt_state: &CheckpointExecutionState,
        tx_data: &CheckpointTransactionData,
    ) -> Vec<TransactionDigest> {
        // Find unexecuted transactions and their expected effects digests
        let (unexecuted_tx_digests, unexecuted_txns, unexecuted_effects): (Vec<_>, Vec<_>, Vec<_>) =
            itertools::multiunzip(
                itertools::izip!(
                    tx_data.transactions.iter(),
                    ckpt_state.data.tx_digests.iter(),
                    ckpt_state.data.fx_digests.iter(),
                    tx_data.effects.iter(),
                    tx_data.executed_fx_digests.iter()
                )
                .filter_map(
                    |(txn, tx_digest, expected_fx_digest, effects, executed_fx_digest)| {
                        if let Some(executed_fx_digest) = executed_fx_digest {
                            assert_not_forked(
                                &ckpt_state.data.checkpoint,
                                tx_digest,
                                expected_fx_digest,
                                executed_fx_digest,
                                &*self.transaction_cache_reader,
                            );
                            None
                        } else if txn.transaction_data().is_end_of_epoch_tx() {
                            None
                        } else {
                            Some((tx_digest, (txn.clone(), *expected_fx_digest), effects))
                        }
                    },
                ),
            );

        for ((tx, _), effects) in itertools::izip!(unexecuted_txns.iter(), unexecuted_effects) {
            if tx.contains_shared_object() {
                self.epoch_store
                    .acquire_shared_version_assignments_from_effects(
                        tx,
                        effects,
                        &*self.object_cache_reader,
                    )
                    .expect("failed to acquire shared version assignments");
            }
        }

        // Enqueue unexecuted transactions with their expected effects digests
        self.execution_scheduler
            .enqueue_with_expected_effects_digest(unexecuted_txns, &self.epoch_store);

        unexecuted_tx_digests
    }

    // Execute the change epoch txn
    #[instrument(level = "error", skip_all)]
    async fn execute_change_epoch_tx(&self, tx_data: &CheckpointTransactionData) {
        let change_epoch_tx = tx_data.transactions.last().unwrap();
        let change_epoch_fx = tx_data.effects.last().unwrap();
        assert_eq!(
            change_epoch_tx.digest(),
            change_epoch_fx.transaction_digest()
        );
        assert!(
            change_epoch_tx.transaction_data().is_end_of_epoch_tx(),
            "final txn must be an end of epoch txn"
        );

        // Ordinarily we would assert that the change epoch txn has not been executed yet.
        // However, during crash recovery, it is possible that we already passed this point and
        // the txn has been executed. You can uncomment this assert if you are debugging a problem
        // related to reconfig. If you hit this assert and it is not because of crash-recovery,
        // it may indicate a bug in the checkpoint executor.
        //
        //     if self
        //         .transaction_cache_reader
        //         .get_executed_effects(change_epoch_tx.digest())
        //         .is_some()
        //     {
        //         fatal!(
        //             "end of epoch txn must not have been executed: {:?}",
        //             change_epoch_tx.digest()
        //         );
        //     }

        self.epoch_store
            .acquire_shared_version_assignments_from_effects(
                change_epoch_tx,
                change_epoch_fx,
                self.object_cache_reader.as_ref(),
            )
            .expect("Acquiring shared version assignments for change_epoch tx cannot fail");

        info!(
            "scheduling change epoch txn with digest: {:?}, expected effects digest: {:?}",
            change_epoch_tx.digest(),
            change_epoch_fx.digest()
        );
        self.execution_scheduler
            .enqueue_with_expected_effects_digest(
                vec![(change_epoch_tx.clone(), change_epoch_fx.digest())],
                &self.epoch_store,
            );

        self.transaction_cache_reader
            .notify_read_executed_effects_digests(&[*change_epoch_tx.digest()])
            .await;
    }

    // Increment the highest executed checkpoint watermark and prune old full-checkpoint contents
    #[instrument(level = "debug", skip_all)]
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

    /// If configured, commit the pending index updates for the provided checkpoint as well as
    /// enqueuing the checkpoint to the subscription service
    #[instrument(level = "info", skip_all)]
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
                warn!("unable to send checkpoint to subscription service: {e}");
            }
        }
    }

    // Extract randomness rounds from the checkpoint version-specific data (if available).
    // Otherwise, extract randomness rounds from the first transaction in the checkpoint
    #[instrument(level = "debug", skip_all)]
    fn extract_randomness_rounds(
        &self,
        checkpoint: &VerifiedCheckpoint,
        checkpoint_contents: &CheckpointContents,
    ) -> Vec<RandomnessRound> {
        if let Some(version_specific_data) = checkpoint
            .version_specific_data(self.epoch_store.protocol_config())
            .expect("unable to get version_specific_data")
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
                self.epoch_store
                    .protocol_config()
                    .min_checkpoint_interval_ms_as_option()
                    .unwrap_or_default(),
            );
            if let Some(first_digest) = checkpoint_contents.inner().first() {
                let maybe_randomness_tx = self.transaction_cache_reader.get_transaction_block(&first_digest.transaction)
                .unwrap_or_else(||
                    fatal!(
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
        }
    }
}
