// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use futures::stream::FuturesOrdered;
use sui_metrics::spawn_monitored_task;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, TransactionDigest},
    committee::StakeUnit,
    error::{SuiError, SuiResult},
    messages::{TransactionEffects, VerifiedCertificate},
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
    mailbox: broadcast::Receiver<VerifiedCheckpoint>,
    checkpoint_store: Arc<CheckpointStore>,
    authority_state: Arc<AuthorityState>,
    highest_executed_checkpoint: Option<CheckpointSequenceNumber>,
    metrics: Arc<CheckpointMetrics>,
}

impl CheckpointExecutor {
    pub fn new(
        state_sync_handle: &sui_network::state_sync::Handle,
        checkpoint_store: Arc<CheckpointStore>,
        authority_state: Arc<AuthorityState>,
        metrics: Arc<CheckpointMetrics>,
    ) -> Result<Self, TypedStoreError> {
        let mailbox = state_sync_handle.subscribe_to_synced_checkpoints();
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
                        let next_to_exec = self
                            .highest_executed_checkpoint
                            .map(|highest| highest + 1)
                            .unwrap_or(0);

                        for seq_num in next_to_exec..=last_to_exec {
                            let next_checkpoint = match self
                                .checkpoint_store
                                .get_checkpoint_by_sequence_number(seq_num)
                            {
                                Ok(Some(new_checkpoint)) => new_checkpoint,
                                Ok(None) => {
                                    error!(
                                        "Checkpoint {:?} does not exist in checkpoint store",
                                        seq_num,
                                    );
                                    self.metrics.checkpoint_exec_errors.inc();
                                    continue;
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
    let effects = if let Some(checkpoint_contents) = checkpoint_store
        .get_checkpoint_contents(&checkpoint.content_digest())
        .map_err(SuiError::from)?
    {
        let txes = checkpoint_contents.into_inner();
        execute_and_verify_transactions(txes, authority_state).await?
    } else {
        warn!(
            "Checkpoint contents empty: {:?}",
            checkpoint.content_digest()
        );
        Vec::new()
    };
    Ok(effects)
}

async fn execute_and_verify_transactions(
    transactions: Vec<ExecutionDigests>,
    authority_state: Arc<AuthorityState>,
) -> SuiResult<Vec<TransactionEffects>> {
    let tx_digests: Vec<TransactionDigest> = transactions.iter().map(|tx| tx.transaction).collect();
    let mut verified_certs = Vec::<VerifiedCertificate>::new();
    for digest in tx_digests.iter() {
        match authority_state.database.read_certificate(digest)? {
            Some(cert) => verified_certs.push(cert),
            None => return Err(SuiError::TransactionNotFound { digest: *digest }),
        }
    }

    {
        // TODO first extract effects and call `store_pending_certs_and_effects`
        // after https://github.com/MystenLabs/sui/pull/6157 in order to write the
        // transaction effects from consensus to authority store so that
        // TransactionManager can read
        let mut tm_guard = authority_state.transaction_manager.lock().await;
        tm_guard.enqueue(verified_certs).await?;
    }

    let actual_fx = authority_state.database.notify_read(tx_digests).await?;
    Ok(actual_fx)
}
