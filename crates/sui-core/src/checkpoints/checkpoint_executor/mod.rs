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

use std::{cmp::Ordering, collections::HashMap, sync::Arc, time::Duration};

use futures::stream::FuturesOrdered;
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use sui_types::{
    base_types::{ExecutionDigests, TransactionDigest, TransactionEffectsDigest},
    committee::Committee,
    crypto::AuthorityPublicKeyBytes,
    error::SuiResult,
    messages::{TransactionEffects, VerifiedCertificate},
    messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint},
};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    task::JoinHandle,
    time::timeout,
};
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};
use typed_store::{rocks::TypedStoreError, Map};

use crate::{
    authority::{AuthorityState, EffectsNotifyRead},
    checkpoints::CheckpointStore,
};

use self::metrics::CheckpointExecutorMetrics;

mod metrics;
#[cfg(test)]
pub(crate) mod tests;

const TASKS_PER_CORE: usize = 1;
const END_OF_EPOCH_BROADCAST_CHANNEL_CAPACITY: usize = 2;
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(10);

type CheckpointExecutionBuffer = FuturesOrdered<
    JoinHandle<(
        VerifiedCheckpoint,
        Option<Vec<(AuthorityPublicKeyBytes, u64)>>,
    )>,
>;

pub struct CheckpointExecutor {
    mailbox: broadcast::Receiver<VerifiedCheckpoint>,
    checkpoint_store: Arc<CheckpointStore>,
    authority_state: Arc<AuthorityState>,
    metrics: Arc<CheckpointExecutorMetrics>,
}

impl CheckpointExecutor {
    pub fn new(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        authority_state: Arc<AuthorityState>,
        prometheus_registry: &Registry,
    ) -> Self {
        Self {
            mailbox,
            checkpoint_store,
            authority_state,
            metrics: CheckpointExecutorMetrics::new(prometheus_registry),
        }
    }

    pub fn new_for_tests(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        authority_state: Arc<AuthorityState>,
    ) -> Self {
        Self {
            mailbox,
            checkpoint_store,
            authority_state,
            metrics: CheckpointExecutorMetrics::new_for_tests(),
        }
    }

    pub fn start(self) -> Result<(Handle, broadcast::Receiver<Committee>), TypedStoreError> {
        let Self {
            mailbox,
            checkpoint_store,
            authority_state,
            metrics,
        } = self;

        let (end_of_epoch_event_sender, _receiver) =
            broadcast::channel::<Committee>(END_OF_EPOCH_BROADCAST_CHANNEL_CAPACITY);

        let executor = CheckpointExecutorEventLoop::new(
            mailbox,
            end_of_epoch_event_sender.clone(),
            checkpoint_store,
            authority_state,
            metrics,
        )?;

        // Return a single pre-subscribed recv channel for end of
        // epoch before starting to prevent race condition
        let end_of_epoch_recv_channel = end_of_epoch_event_sender.subscribe();

        let event_loop_handle = tokio::spawn(executor.run());
        Ok((
            Handle {
                end_of_epoch_event_sender,
                event_loop_handle,
            },
            end_of_epoch_recv_channel,
        ))
    }
}

pub struct Handle {
    end_of_epoch_event_sender: broadcast::Sender<Committee>,
    event_loop_handle: JoinHandle<()>,
}

impl Handle {
    pub async fn join(self) -> std::result::Result<(), tokio::task::JoinError> {
        self.event_loop_handle.await
    }

    pub fn event_loop_handle(self) -> JoinHandle<()> {
        self.event_loop_handle
    }

    pub fn subscribe_to_end_of_epoch(&self) -> broadcast::Receiver<Committee> {
        self.end_of_epoch_event_sender.subscribe()
    }
}

pub struct CheckpointExecutorEventLoop {
    mailbox: broadcast::Receiver<VerifiedCheckpoint>,
    end_of_epoch_event_sender: broadcast::Sender<Committee>,
    checkpoint_store: Arc<CheckpointStore>,
    authority_state: Arc<AuthorityState>,
    highest_scheduled_seq_num: Option<CheckpointSequenceNumber>,
    latest_synced_checkpoint: Option<VerifiedCheckpoint>,
    /// end_of_epoch is set to true once the last checkpoint
    /// of the current epoch has been scheduled for execution.
    /// It is used as a marker that no more checkpoints may be
    /// scheduled for execution (until reset).
    /// It is then reset only after reconfig has been run
    /// successfully. In the event of crash recovery between
    /// executing the final checkpoint and successfully completing
    /// reconfig, CheckpointExecutor will start with end_of_epoch == false.
    /// This is ok, as in such a case, the execution watermark for
    /// the final checkpoint will not have been set, thus CheckpointExecutor
    /// will reschedule the last checkpoint and correctly set end_of_epoch.
    end_of_epoch: bool,
    task_limit: usize,
    metrics: Arc<CheckpointExecutorMetrics>,
}

impl CheckpointExecutorEventLoop {
    fn new(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        end_of_epoch_event_sender: broadcast::Sender<Committee>,
        checkpoint_store: Arc<CheckpointStore>,
        authority_state: Arc<AuthorityState>,
        metrics: Arc<CheckpointExecutorMetrics>,
    ) -> Result<Self, TypedStoreError> {
        Ok(Self {
            mailbox,
            end_of_epoch_event_sender,
            checkpoint_store,
            authority_state,
            highest_scheduled_seq_num: None,
            latest_synced_checkpoint: None,
            end_of_epoch: false,
            task_limit: TASKS_PER_CORE * num_cpus::get(),
            metrics,
        })
    }

    pub async fn run(mut self) {
        self.handle_crash_recovery().await.unwrap();

        while let Some((last_checkpoint, next_committee)) =
            self.execute_checkpoints_for_epoch().await
        {
            self.reconfig(next_committee, last_checkpoint.epoch()).await;
            self.end_of_epoch = false;
        }
        // Channel closed
    }

    pub async fn handle_crash_recovery(&self) -> SuiResult {
        let local_epoch = self.authority_state.epoch();

        match self.checkpoint_store.get_highest_executed_checkpoint()? {
            // TODO this invariant may no longer hold once we introduce snapshots
            None => assert_eq!(local_epoch, 0),

            Some(last_checkpoint) => {
                match last_checkpoint.next_epoch_committee() {
                    // Make sure there was not an epoch change in this case
                    None => assert_eq!(local_epoch, last_checkpoint.epoch()),
                    // Last executed checkpoint before shutdown was last of epoch.
                    // Make sure reconfig was successful, otherwise redo
                    Some(committee) => {
                        if last_checkpoint.epoch() == local_epoch {
                            info!(
                                "Handling end of epoch pre-reconfig crash recovery for epoch {:?} -> {:?}",
                                local_epoch,
                                local_epoch + 1
                            );
                            self.reconfig(committee.to_vec(), local_epoch).await;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Executes all checkpoints for the current epoch. At epoch boundary,
    /// awaits the queue of scheduled checkpoints and returns the committee
    /// of the next epoch.
    pub async fn execute_checkpoints_for_epoch(
        &mut self,
    ) -> Option<(VerifiedCheckpoint, Vec<(AuthorityPublicKeyBytes, u64)>)> {
        let mut pending: CheckpointExecutionBuffer = FuturesOrdered::new();

        loop {
            if !self.end_of_epoch {
                self.schedule_synced_checkpoints(&mut pending)
                    .unwrap_or_else(|err| {
                        self.metrics.checkpoint_exec_errors.inc();
                        panic!(
                            "Failed to schedule synced checkpoints for execution: {:?}",
                            err
                        );
                    });
            }

            tokio::select! {
                // Check for completed workers and ratchet the highest_checkpoint_executed
                // watermark accordingly. Note that given that checkpoints are guaranteed to
                // be processed (added to FuturesOrdered) in seq_number order, using FuturesOrdered
                // guarantees that we will also ratchet the watermarks in order.
                Some(Ok((checkpoint, next_committee))) = pending.next() => {
                    match next_committee {
                        None => {
                            // Ensure that we are not skipping checkpoints at any point
                            if let Some(prev_highest) = self.checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap() {
                                assert_eq!(prev_highest + 1, checkpoint.sequence_number());
                            } else {
                                assert_eq!(checkpoint.sequence_number(), 0);
                            }

                            let new_highest = checkpoint.sequence_number();
                            debug!(
                                "Bumping highest_executed_checkpoint watermark to {:?}",
                                new_highest,
                            );
                            self.metrics.last_executed_checkpoint.set(new_highest as i64);
                            self.checkpoint_store
                                .update_highest_executed_checkpoint(&checkpoint)
                                .unwrap();
                        }
                        Some(committee) => {
                            debug!(
                                "Last checkpoint ({:?}) of epoch {:?} has finished execution",
                                checkpoint.sequence_number(),
                                checkpoint.epoch(),
                            );
                            self.checkpoint_store
                                .update_highest_executed_checkpoint(&checkpoint)
                                .unwrap();
                            return Some((checkpoint, committee));
                        }
                    }
                }
                // Check for newly synced checkpoints from StateSync.
                received = self.mailbox.recv() => match received {
                    Ok(checkpoint) => {
                        debug!(
                            "Received new synced checkpoint message for checkpoint {:?}",
                            checkpoint.sequence_number(),
                        );
                        self.latest_synced_checkpoint = Some(checkpoint);
                    }
                    // In this case, messages in the mailbox have been overwritten
                    // as a result of lagging too far behind. In this case, we need to
                    // nullify self.latest_synced_checkpoint as the latest synced needs to
                    // be read from the watermark
                    Err(RecvError::Lagged(num_skipped)) => {
                        self.latest_synced_checkpoint = None;

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

    fn schedule_synced_checkpoints(
        &mut self,
        pending: &mut CheckpointExecutionBuffer,
    ) -> SuiResult {
        let latest_synced_checkpoint = match self.latest_synced_checkpoint.clone() {
            Some(checkpoint) => checkpoint,
            // Either nothing to sync or we have lagged too far behind in the recv channel
            // and `self.latest_synced_checkpoint` is stale. Need to read watermark
            None => {
                if let Some(checkpoint) = self
                    .checkpoint_store
                    .get_highest_synced_checkpoint()
                    .unwrap_or_else(|err| {
                        panic!("Failed to read highest synced checkpoint: {:?}", err)
                    })
                {
                    self.latest_synced_checkpoint = Some(checkpoint.clone());
                    checkpoint
                } else {
                    return Ok(());
                }
            }
        };

        let highest_executed_seq_num = self
            .checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .unwrap_or_else(|err| {
                panic!(
                    "Failed to read highest executed checkpoint sequence number: {:?}",
                    err
                )
            });

        // Note that either of these can be higher. If the node crashes with many
        // scheduled tasks, then the in-memory watermark starts as None, but the
        // persistent watermark is set, hence we start there. If we get a new
        // message with checkpoints tasks scheduled, then the in-memory watermark
        // will be greater, and hence we start from there.
        let next_to_exec = std::cmp::max(
            highest_executed_seq_num
                .map(|highest| highest.saturating_add(1))
                .unwrap_or(0),
            self.highest_scheduled_seq_num
                .map(|highest| highest.saturating_add(1))
                .unwrap_or(0),
        );

        match next_to_exec.cmp(&latest_synced_checkpoint.sequence_number()) {
            // fully caught up case
            Ordering::Greater => return Ok(()),
            // follow case. Avoid reading from DB and used checkpoint passed
            // from StateSync
            Ordering::Equal => return self.schedule_checkpoint(latest_synced_checkpoint, pending),
            // Need to catch up more than 1. Read from store
            Ordering::Less => {
                for i in next_to_exec..=latest_synced_checkpoint.sequence_number() {
                    if pending.len() >= self.task_limit || self.end_of_epoch {
                        break;
                    }
                    let checkpoint = self
                        .checkpoint_store
                        .get_checkpoint_by_sequence_number(i)?
                        .unwrap_or_else(|| {
                            panic!(
                                "Checkpoint sequence number {:?} does not exist in checkpoint store",
                                i
                            )
                        });
                    self.schedule_checkpoint(checkpoint, pending)?;
                }
            }
        }

        Ok(())
    }

    fn schedule_checkpoint(
        &mut self,
        checkpoint: VerifiedCheckpoint,
        pending: &mut CheckpointExecutionBuffer,
    ) -> SuiResult {
        debug!(
            "Scheduling checkpoint {:?} for execution",
            checkpoint.sequence_number()
        );
        // Mismatch between node epoch and checkpoint epoch after startup
        // crash recovery is invalid
        let checkpoint_epoch = checkpoint.epoch();
        let node_epoch = self.authority_state.epoch();
        assert_eq!(
            checkpoint_epoch, node_epoch,
            "Epoch mismatch after startup recovery. checkpoint epoch: {:?}, node epoch: {:?}",
            checkpoint_epoch, node_epoch,
        );

        let next_committee = checkpoint.summary().next_epoch_committee.clone();

        if next_committee.is_some() {
            self.end_of_epoch = true;
        }

        let highest_scheduled = checkpoint.sequence_number();
        let state = self.authority_state.clone();
        let store = self.checkpoint_store.clone();
        let metrics = self.metrics.clone();

        pending.push_back(spawn_monitored_task!(async move {
            while let Err(err) =
                execute_checkpoint(checkpoint.clone(), state.clone(), store.clone()).await
            {
                error!(
                    "Error while executing checkpoint, will retry in 1s: {:?}",
                    err
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
                metrics.checkpoint_exec_errors.inc();
            }

            (checkpoint, next_committee)
        }));

        self.highest_scheduled_seq_num = Some(highest_scheduled);

        Ok(())
    }

    async fn reconfig(
        &self,
        next_committee: Vec<(AuthorityPublicKeyBytes, u64)>,
        current_epoch: u64,
    ) {
        info!("Notifying end of epoch {:?}", current_epoch);

        let next_epoch = current_epoch + 1;
        let end_of_epoch_message = Committee::new(next_epoch, next_committee.into_iter().collect())
            .unwrap_or_else(|err| panic!("Failed to create new committee object: {:?}", err));
        let _ = self.end_of_epoch_event_sender.send(end_of_epoch_message);
        self.authority_state
            .epoch_store()
            .wait_epoch_terminated()
            .await;

        self.metrics.current_local_epoch.set(next_epoch as i64);
        info!(
            "Reconfig complete. New epoch: {:?}. Resuming checkpoint execution",
            next_epoch
        );
    }
}

pub async fn execute_checkpoint(
    checkpoint: VerifiedCheckpoint,
    authority_state: Arc<AuthorityState>,
    checkpoint_store: Arc<CheckpointStore>,
) -> SuiResult {
    debug!(
        "Scheduling checkpoint {:?} for execution",
        checkpoint.sequence_number(),
    );
    let txes = checkpoint_store
        .get_checkpoint_contents(&checkpoint.content_digest())?
        .unwrap_or_else(|| {
            panic!(
                "Checkpoint contents for digest {:?} does not exist",
                checkpoint.content_digest()
            )
        })
        .into_inner();

    execute_transactions(txes, authority_state).await
}

async fn execute_transactions(
    execution_digests: Vec<ExecutionDigests>,
    authority_state: Arc<AuthorityState>,
) -> SuiResult {
    let all_tx_digests: Vec<TransactionDigest> =
        execution_digests.iter().map(|tx| tx.transaction).collect();

    let synced_txns: Vec<VerifiedCertificate> = authority_state
        .database
        .perpetual_tables
        .synced_transactions
        .multi_get(&all_tx_digests)?
        .into_iter()
        .flatten()
        .map(|tx| tx.into())
        .collect();

    let effects_digests: Vec<TransactionEffectsDigest> = execution_digests
        .iter()
        .map(|digest| digest.effects)
        .collect();
    let digest_to_effects: HashMap<TransactionDigest, TransactionEffects> = authority_state
        .database
        .perpetual_tables
        .effects
        .multi_get(effects_digests)?
        .into_iter()
        .map(|fx| {
            if fx.is_none() {
                panic!("Transaction effects do not exist in effects table");
            }
            let fx = fx.unwrap();
            (fx.transaction_digest, fx)
        })
        .collect();

    for tx in synced_txns.clone() {
        if tx.contains_shared_object() {
            authority_state
                .database
                .acquire_shared_locks_from_effects(&tx, digest_to_effects.get(tx.digest()).unwrap())
                .await?;
        }
    }
    authority_state
        .database
        .epoch_store()
        .insert_pending_certificates(&synced_txns)?;

    authority_state
        .transaction_manager()
        .enqueue(synced_txns)
        .await?;

    // Once synced_txns have been awaited, all txns should have effects committed.
    let mut periods = 1;
    loop {
        let effects_future = authority_state
            .database
            .notify_read_effects(all_tx_digests.clone());

        match timeout(LOCAL_EXECUTION_TIMEOUT, effects_future).await {
            Err(_elapsed) => {
                warn!(
                    "Transaction effects for checkpoint not present within {:?}. ",
                    LOCAL_EXECUTION_TIMEOUT * periods
                );
                periods += 1;
            }
            Ok(Err(err)) => return Err(err),
            Ok(Ok(_)) => return Ok(()),
        }
    }
}
