// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! CheckpointExecutor spawns an active process that acts as a Consumer to
//! StateSync for newly synced checkpoints, taking these checkpoints and
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
//! CheckpointExecutor parallelizes checkpoints of the same epoch as much as possible, and
//! handles epoch boundaries by calling to reconfig once all checkpoints of an epoch have finished
//! executing.

use core::panic;
use std::{sync::Arc, time::Duration};

use futures::stream::FuturesOrdered;
use mysten_metrics::spawn_monitored_task;
use sui_types::{
    base_types::{ExecutionDigests, TransactionDigest},
    crypto::AuthorityPublicKeyBytes,
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

type CheckpointExecutionBuffer = FuturesOrdered<
    JoinHandle<(
        u64,
        VerifiedCheckpoint,
        Option<Vec<(AuthorityPublicKeyBytes, u64)>>,
    )>,
>;

pub struct CheckpointExecutor {
    mailbox: broadcast::Receiver<VerifiedCheckpoint>,
    checkpoint_store: Arc<CheckpointStore>,
    authority_state: Arc<AuthorityState>,
    highest_scheduled_checkpoint: Option<CheckpointSequenceNumber>,
    end_of_epoch: bool,
    metrics: Arc<CheckpointMetrics>,
}

impl CheckpointExecutor {
    pub fn new(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        authority_state: Arc<AuthorityState>,
        metrics: Arc<CheckpointMetrics>,
    ) -> Result<Self, TypedStoreError> {
        Ok(Self {
            mailbox,
            checkpoint_store,
            authority_state,
            highest_scheduled_checkpoint: None,
            end_of_epoch: false,
            metrics,
        })
    }

    pub async fn run(mut self) {
        while let Some((last_checkpoint, next_committee)) =
            self.execute_checkpoints_for_epoch().await
        {
            reconfig(next_committee).await;
            self.checkpoint_store
                .update_highest_executed_checkpoint(&last_checkpoint)
                .unwrap();
            self.end_of_epoch = false;
        }
        // Channel closed
    }

    /// Executes all checkpoints for the current epoch. At epoch boundary,
    /// awaits the queue of scheduled checkpoints, and returns the committe
    /// of the next epoch.
    pub async fn execute_checkpoints_for_epoch(
        &mut self,
    ) -> Option<(VerifiedCheckpoint, Vec<(AuthorityPublicKeyBytes, u64)>)> {
        let mut finished: CheckpointExecutionBuffer = FuturesOrdered::new();
        let task_limit = TASKS_PER_CORE * num_cpus::get();

        loop {
            tokio::select! {
                // Check for completed workers and ratchet the highest_checkpoint_executed
                // watermark accordingly. Note that given that checkpoints are guaranteed to
                // be processed (added to FuturesOrdered) in seq_number order, using FuturesOrdered
                // guarantees that we will also ratchet the watermarks in order.
                Some(Ok((seq_num, checkpoint, next_committee))) = finished.next() => {
                    match next_committee {
                        None => {
                            self.metrics.last_executed_checkpoint.set(seq_num as i64);
                            self.checkpoint_store
                                .update_highest_executed_checkpoint(&checkpoint)
                                .unwrap();

                            let highest_synced = self
                                .checkpoint_store
                                .get_highest_synced_checkpoint_seq_number().unwrap()
                                .unwrap_or(0);
                            // If we got here, `highest_scheduled_checkpoint` must have been set,
                            // so safe to unwrap
                            if self.highest_scheduled_checkpoint.unwrap() < highest_synced && !self.end_of_epoch {
                                self.schedule_checkpoint(seq_num.saturating_add(1), &mut finished).await;
                            }
                        }
                        // Last checkpoint of epoch -- Note that we must not update the
                        // highest executed watermark until after reconfig has completed!
                        Some(committee) => return Some((checkpoint, committee)),
                    }
                }
                // Read from StateSync channel if we have capacity to schedule more checkpoints.
                // If we're at end of epoch, skip this branch so that we can effectively await
                // execution of all remaining checkpoints of the epoch (which should be scheduled)
                // and call reconfig.
                received = self.mailbox.recv(), if finished.len() < task_limit && !self.end_of_epoch => match received {
                    Ok(checkpoint) => {
                        let last_to_exec = checkpoint.sequence_number() as u64;
                        let highest_executed_checkpoint = self.checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap_or_else(
                            |err| {
                                panic!("Failed to read highest executed checkpoint sequence number: {:?}", err)
                            }
                        );

                        // Note that either of these can be higher. If the node crashes with many
                        // scheduled tasks, then the in-memory watermark starts as None, but the
                        // persistent watermark is set, hence we start there. If we get a new
                        // messsage with checkpoints tasks scheduled, then the in-memory watermark
                        // will be greater, and hence we start from there.
                        let next_to_exec = std::cmp::max(
                            highest_executed_checkpoint
                                .map(|highest| highest.saturating_add(1))
                                .unwrap_or(0),
                            self.highest_scheduled_checkpoint
                                .map(|highest| highest.saturating_add(1))
                                .unwrap_or(0),
                        );

                        // Schedule as many checkpoints as possible in order to catch up quickly
                        for seq_num in next_to_exec..=last_to_exec {
                            self.schedule_checkpoint(seq_num, &mut finished).await;

                            if finished.len() >= task_limit || self.end_of_epoch {
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
                        return None;
                    }
                },
            }
        }
    }

    async fn schedule_checkpoint(
        &mut self,
        seq_num: CheckpointSequenceNumber,
        finished: &mut CheckpointExecutionBuffer,
    ) {
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
                self.metrics.checkpoint_exec_errors.inc();
                panic!("Failed to fetch checkpoint {:?}: {:?}", seq_num, err);
            }
        };

        // Since StateSync is guaranteed to produce checkpoint sync messages
        // in monotonically increasing order, then a mismatch of epoch would
        // mean that we failed to execute reconfig at the epoch boundary. This
        // is an invalid state that should not happen under normal operation
        assert_eq!(next_checkpoint.epoch(), self.authority_state.epoch());

        let state = self.authority_state.clone();
        let store = self.checkpoint_store.clone();
        let metrics = self.metrics.clone();
        self.highest_scheduled_checkpoint = Some(seq_num);

        let next_committee = next_checkpoint.summary().next_epoch_committee.clone();
        if next_committee.is_some() {
            self.end_of_epoch = true;
        }

        finished.push_back(spawn_monitored_task!(async move {
            while let Err(err) =
                execute_checkpoint(next_checkpoint.clone(), state.clone(), store.clone()).await
            {
                error!(
                    "Error while executing checkpoint, will retry in 1s: {:?}",
                    err
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
                metrics.checkpoint_exec_errors.inc();
            }

            (seq_num, next_checkpoint, next_committee)
        }));
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
        .epoch_store()
        .multi_get_pending_certificate(&tx_digests)?
        .into_iter()
        .map(|tx| tx.unwrap())
        .collect();

    // TODO once https://github.com/MystenLabs/sui/pull/6157 is landed,
    // replace with call to `authority_state.add_pending_certs_and_effects`,
    // which handles reading effects for shared object version numbers
    // and enqueing transactions with TransactionManager.
    authority_state.transaction_manager.enqueue(txns).await?;

    let actual_fx = authority_state.database.notify_read(tx_digests).await?;
    Ok(actual_fx)
}

async fn reconfig(_next_epoch_committee: Vec<(AuthorityPublicKeyBytes, u64)>) {
    // TODO
}
