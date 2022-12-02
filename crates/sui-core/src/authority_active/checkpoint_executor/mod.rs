// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::panic;
use std::{sync::Arc, time::Duration};

use broadcast::Receiver;
use futures::stream::FuturesOrdered;
use sui_metrics::spawn_monitored_task;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, TransactionDigest},
    committee::StakeUnit,
    error::{SuiError, SuiResult},
    messages::TransactionEffects,
    messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint},
};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    task::JoinHandle,
};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};
use typed_store::rocks::TypedStoreError;

use crate::{
    authority::{AuthorityState, EffectsNotifyRead},
    checkpoints::{CheckpointMetrics, CheckpointStore},
};

#[cfg(test)]
pub(crate) mod tests;

const TASKS_PER_CORE: usize = 1;

pub struct CheckpointExecutor {
    mailbox: Receiver<VerifiedCheckpoint>,
    checkpoint_store: Arc<CheckpointStore>,
    authority_state: Arc<AuthorityState>,
    highest_executed_checkpoint: Option<CheckpointSequenceNumber>,
    metrics: Arc<CheckpointMetrics>,
}

impl CheckpointExecutor {
    pub fn new(
        mailbox: Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        authority_state: Arc<AuthorityState>,
        metrics: Arc<CheckpointMetrics>,
    ) -> Result<Self, TypedStoreError> {
        let highest_executed_checkpoint =
            checkpoint_store.get_highest_executed_checkpoint_seq_number()?;

        Ok(Self {
            mailbox,
            checkpoint_store,
            highest_executed_checkpoint,
            authority_state,
            metrics,
        })
    }

    pub async fn run(mut self) {
        let mut finished: FuturesOrdered<JoinHandle<(u64, VerifiedCheckpoint)>> =
            FuturesOrdered::new();
        let task_limit = TASKS_PER_CORE * num_cpus::get();

        // In memory only "watermark" to keep track of the highest checkpoint for which we've
        // spun up an execution task. This is to prevent us from rescheduling the same checkpoints
        // in the case where we receive a synced checkpoint message before scheduled checkpoints have
        // completed, and hence have had the chance to update the on-disk watermark `highest_executed_checkpoint`.
        let mut highest_scheduled_checkpoint: Option<u64> = None;

        loop {
            tokio::select! {
                // Check for completed workers and ratchet the highest_checkpoint_executed
                // watermark accordingly. Note that given that checkpoints are guaranteed to
                // be processed (added to FuturesOrdered) in seq_number order, using FuturesOrdered
                // guarantees that we will also ratchet the watermarks in order.
                Some(Ok((seq_num, checkpoint))) = finished.next() => {
                    self.highest_executed_checkpoint = Some(seq_num as CheckpointSequenceNumber);
                    self.metrics.last_executed_checkpoint.set(seq_num as i64);
                    self.update_lag_metric(seq_num)
                        .unwrap_or_else(|err| error!("Update lag metric error: {:?}", err));

                    self.checkpoint_store
                        .update_highest_executed_checkpoint(&checkpoint)
                        .unwrap();
                },
                // Limits to `task_limit` worker tasks. If we are at capacity, reloop and either
                // check again or attempt to process and consume the FuturesOrdered queue for
                // finished tasks
                received = self.mailbox.recv(), if finished.len() < task_limit => match received {
                    Ok(checkpoint) => {
                        let last_to_exec = checkpoint.sequence_number() as u64;

                        // Note that either of these can be higher. If the node crashes with many
                        // scheduled tasks, then the in-memory watermark starts as None, but the
                        // persistent watermark is set, hence we start there. If we get a new
                        // messsage with checkpoints tasks scheduled, then the in-memory watermark
                        // will be greater, and hence we start from there.
                        let next_to_exec = std::cmp::max(
                            self
                                .highest_executed_checkpoint
                                .map(|highest| highest + 1)
                                .unwrap_or(0),
                            highest_scheduled_checkpoint
                                .map(|highest| highest + 1)
                                .unwrap_or(0),
                        );

                        for seq_num in next_to_exec..=last_to_exec {
                            let next_checkpoint = match self
                                .checkpoint_store
                                .get_checkpoint_by_sequence_number(seq_num)
                            {
                                Ok(Some(new_checkpoint)) => new_checkpoint,
                                Ok(None) => {
                                    self.metrics.checkpoint_exec_errors.inc();
                                    panic!(
                                        "Checkpoint {:?} does not exist in checkpoint store",
                                        seq_num,
                                    );
                                }
                                Err(err) => {
                                    error!("Failed to fetch checkpoint {:?}: {:?}", seq_num, err);
                                    self.metrics.checkpoint_exec_errors.inc();
                                    continue;
                                }
                            };

                            // Since StateSync is guaranteed to produce checkpoint sync messages
                            // in monotonically increasing order, then a mismatch of epoch would
                            // mean that we failed to execute reconfig at the epoch boundary.
                            assert!(next_checkpoint.epoch() == self.authority_state.epoch());

                            let state = self.authority_state.clone();
                            let store = self.checkpoint_store.clone();
                            let next_checkpoint_clone = next_checkpoint.clone();
                            let metrics = self.metrics.clone();
                            highest_scheduled_checkpoint = Some(seq_num);

                            finished.push_back(spawn_monitored_task!(async move {
                                while let Err(err) = execute_checkpoint(
                                    next_checkpoint_clone.clone(),
                                    state.clone(),
                                    store.clone(),
                                ).await {
                                    error!(
                                        "Error while executing checkpoint, will retry in 1s: {:?}",
                                        err
                                    );
                                    tokio::time::sleep(Duration::from_secs(1)).await;
                                    metrics.checkpoint_exec_errors.inc();
                                }

                                (seq_num, next_checkpoint_clone)
                            }));

                            // Last checkpoint of epoch
                            if let Some(next_committee) = next_checkpoint.next_epoch_committee() {
                                // Unwrap ok because reconfig failure is unrecoverable
                                self.reconfig(next_committee).await.unwrap();
                            }

                            if finished.len() >= task_limit {
                                break;
                            }
                        }
                    }
                    // In this case, messages in the mailbox have been overwritten
                    // as a result of lagging too far behind. We can simply continue the loop,
                    // as the next call to recv() should return the overwritten checkpoint and we
                    // will attempt to execute all checkpoints from (highest_executed_checkpoint + 1)
                    // up to the result of the next recv() call.
                    Err(RecvError::Lagged(num_skipped)) => {
                        warn!(
                            "Checkpoint Execution Recv channel overflowed {:?} messages",
                            num_skipped,
                        );
                        self.metrics
                            .checkpoint_exec_recv_channel_overflow
                            .inc_by(num_skipped);
                    }
                    Err(RecvError::Closed) => {
                        info!("Checkpoint Execution Sender (StateSync) closed channel");
                        break;
                    }
                },
            }
        }
    }

    async fn reconfig(&self, _next_epoch_committee: &[(AuthorityName, StakeUnit)]) -> SuiResult {
        // TODO(william) call into Reconfig
        Ok(())
    }

    fn update_lag_metric(&self, highest_executed: CheckpointSequenceNumber) -> SuiResult {
        let highest_synced = self
            .checkpoint_store
            .get_highest_synced_checkpoint_seq_number()?
            .unwrap_or(0);

        let diff = highest_synced - highest_executed;
        self.metrics.checkpoint_exec_lag.set(diff as i64);
        Ok(())
    }
}

pub async fn execute_checkpoint(
    checkpoint: VerifiedCheckpoint,
    authority_state: Arc<AuthorityState>,
    checkpoint_store: Arc<CheckpointStore>,
) -> SuiResult<Vec<TransactionEffects>> {
    let txes = checkpoint_store
        .get_checkpoint_contents(&checkpoint.content_digest())
        .map_err(SuiError::from)?
        .unwrap_or_else(|| {
            panic!(
                "Checkpoint contents for digest {:?} does not exist",
                checkpoint.content_digest()
            )
        })
        .into_inner();

    let effects = execute_and_verify_transactions(txes, authority_state).await?;
    Ok(effects)
}

async fn execute_and_verify_transactions(
    transactions: Vec<ExecutionDigests>,
    authority_state: Arc<AuthorityState>,
) -> SuiResult<Vec<TransactionEffects>> {
    let tx_digests: Vec<TransactionDigest> = transactions.iter().map(|tx| tx.transaction).collect();
    let txns = authority_state
        .database
        .multi_get_pending_certificate(&tx_digests)?
        .into_iter()
        .map(|tx| tx.unwrap())
        .collect();

    // TODO once https://github.com/MystenLabs/sui/pull/6157 is landed,
    // replace with call to `authority_state.add_pending_certs_and_effects`,
    // which handles reading effects for shared object version numbers
    // and enqueing transactions with TransactionManager.
    {
        let mut tm_guard = authority_state.transaction_manager.lock().await;
        tm_guard.enqueue(txns).await?;
    }

    let actual_fx = authority_state.database.notify_read(tx_digests).await?;
    Ok(actual_fx)
}
