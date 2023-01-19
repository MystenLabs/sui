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

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::stream::FuturesOrdered;
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use sui_config::node::CheckpointExecutorConfig;
use sui_types::{
    base_types::{ExecutionDigests, TransactionDigest, TransactionEffectsDigest},
    committee::{Committee, EpochId},
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
use tracing::{debug, error, info, instrument, warn};
use typed_store::Map;

use crate::authority::AuthorityStore;
use crate::authority::{
    authority_per_epoch_store::AuthorityPerEpochStore, authority_store::EffectsStore,
};
use crate::transaction_manager::TransactionManager;
use crate::{authority::EffectsNotifyRead, checkpoints::CheckpointStore};

use self::metrics::CheckpointExecutorMetrics;

mod metrics;
#[cfg(test)]
pub(crate) mod tests;

type CheckpointExecutionBuffer = FuturesOrdered<
    JoinHandle<(
        VerifiedCheckpoint,
        Option<Vec<(AuthorityPublicKeyBytes, u64)>>,
    )>,
>;

pub struct CheckpointExecutor {
    mailbox: broadcast::Receiver<VerifiedCheckpoint>,
    checkpoint_store: Arc<CheckpointStore>,
    authority_store: Arc<AuthorityStore>,
    tx_manager: Arc<TransactionManager>,
    config: CheckpointExecutorConfig,
    highest_scheduled_seq_num: Option<CheckpointSequenceNumber>,
    latest_synced_checkpoint: Option<VerifiedCheckpoint>,
    // If true, need to run crash recovery
    cold_start: bool,
    metrics: Arc<CheckpointExecutorMetrics>,
}

impl CheckpointExecutor {
    pub fn new(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        authority_store: Arc<AuthorityStore>,
        tx_manager: Arc<TransactionManager>,
        config: CheckpointExecutorConfig,
        prometheus_registry: &Registry,
    ) -> Self {
        Self {
            mailbox,
            checkpoint_store,
            authority_store,
            tx_manager,
            config,
            highest_scheduled_seq_num: None,
            latest_synced_checkpoint: None,
            cold_start: true,
            metrics: CheckpointExecutorMetrics::new(prometheus_registry),
        }
    }

    pub fn new_for_tests(
        mailbox: broadcast::Receiver<VerifiedCheckpoint>,
        checkpoint_store: Arc<CheckpointStore>,
        authority_store: Arc<AuthorityStore>,
        tx_manager: Arc<TransactionManager>,
    ) -> Self {
        Self {
            mailbox,
            checkpoint_store,
            authority_store,
            tx_manager,
            config: Default::default(),
            highest_scheduled_seq_num: None,
            latest_synced_checkpoint: None,
            cold_start: true,
            metrics: CheckpointExecutorMetrics::new_for_tests(),
        }
    }

    pub async fn run_epoch(&mut self, epoch_store: Arc<AuthorityPerEpochStore>) -> Committee {
        self.metrics
            .current_local_epoch
            .set(epoch_store.epoch() as i64);

        if self.cold_start {
            let end_of_epoch_recovery = self
                .handle_crash_recovery(epoch_store.epoch())
                .await
                .unwrap();
            self.cold_start = false;

            if let Some(committee) = end_of_epoch_recovery {
                return committee;
            }
        }

        self.run_epoch_impl(epoch_store).await
    }

    async fn handle_crash_recovery(&self, epoch: EpochId) -> SuiResult<Option<Committee>> {
        let mut highest_executed_metric = 0;

        match self.checkpoint_store.get_highest_executed_checkpoint()? {
            // TODO this invariant may no longer hold once we introduce snapshots
            None => assert_eq!(epoch, 0),

            Some(last_checkpoint) => {
                highest_executed_metric = last_checkpoint.sequence_number();

                match last_checkpoint.next_epoch_committee() {
                    // Make sure there was not an epoch change in this case
                    None => assert_eq!(epoch, last_checkpoint.epoch()),
                    // Last executed checkpoint before shutdown was last of epoch.
                    // Make sure reconfig was successful, otherwise redo
                    Some(committee) => {
                        if last_checkpoint.epoch() == epoch {
                            info!(
                                old_epoch = epoch,
                                new_epoch = epoch.saturating_add(1),
                                "Handling end of epoch pre-reconfig crash recovery",
                            );

                            self.metrics
                                .last_executed_checkpoint
                                .set(highest_executed_metric as i64);

                            return Ok(Some(self.create_committee_object(
                                last_checkpoint.clone(),
                                committee.to_vec(),
                            )));
                        }
                    }
                }
            }
        }

        self.metrics
            .last_executed_checkpoint
            .set(highest_executed_metric as i64);
        Ok(None)
    }

    /// Executes all checkpoints for the current epoch and returns the next committee
    async fn run_epoch_impl(&mut self, epoch_store: Arc<AuthorityPerEpochStore>) -> Committee {
        let mut pending: CheckpointExecutionBuffer = FuturesOrdered::new();

        loop {
            self.schedule_synced_checkpoints(&mut pending, epoch_store.clone())
                .unwrap_or_else(|err| {
                    self.metrics.checkpoint_exec_errors.inc();
                    panic!(
                        "Failed to schedule synced checkpoints for execution: {:?}",
                        err
                    );
                });

            tokio::select! {
                // Check for completed workers and ratchet the highest_checkpoint_executed
                // watermark accordingly. Note that given that checkpoints are guaranteed to
                // be processed (added to FuturesOrdered) in seq_number order, using FuturesOrdered
                // guarantees that we will also ratchet the watermarks in order.
                Some(Ok((checkpoint, next_committee))) = pending.next() => {
                    // Ensure that we are not skipping checkpoints at any point
                    if let Some(prev_highest) = self.checkpoint_store.get_highest_executed_checkpoint_seq_number().unwrap() {
                        assert_eq!(prev_highest + 1, checkpoint.sequence_number());
                    } else {
                        assert_eq!(checkpoint.sequence_number(), 0);
                    }

                    match next_committee {
                        None => {
                            let new_highest = checkpoint.sequence_number();
                            debug!(
                                "Bumping highest_executed_checkpoint watermark to {:?}",
                                new_highest,
                            );

                            self.checkpoint_store
                                .update_highest_executed_checkpoint(&checkpoint)
                                .unwrap();
                            self.metrics.last_executed_checkpoint.set(new_highest as i64);
                        }
                        Some(committee) => {
                            info!(
                                ended_epoch = checkpoint.epoch(),
                                last_checkpoint=checkpoint.sequence_number(),
                                "Reached end of epoch",
                            );
                            self.checkpoint_store
                                .update_highest_executed_checkpoint(&checkpoint)
                                .unwrap();
                            self.metrics.last_executed_checkpoint.set(checkpoint.sequence_number() as i64);

                            // be extra careful to ensure we don't have orphans
                            assert!(
                                pending.is_empty(),
                                "Pending checkpoint execution buffer should be empty after processing last checkpoint of epoch",
                            );

                            return self.create_committee_object(checkpoint, committee);
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
                        panic!("Checkpoint Execution Sender (StateSync) closed channel unexpectedly");
                    }
                },
            }
        }
    }

    fn schedule_synced_checkpoints(
        &mut self,
        pending: &mut CheckpointExecutionBuffer,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        let latest_synced_checkpoint = match self.latest_synced_checkpoint.clone() {
            Some(checkpoint) => checkpoint,
            // Either nothing to sync or we have lagged too far behind in the recv channel
            // and `self.latest_synced_checkpoint` is stale. Need to read watermark
            None => {
                if let Some(checkpoint) = self
                    .checkpoint_store
                    .get_highest_synced_checkpoint()
                    .expect("Failed to read highest synced checkpoint")
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

        // If self.highest_scheduled_seq_num is None, we have just started up, and should
        // use the highest executed watermark
        let next_to_exec = self
            .highest_scheduled_seq_num
            .map(|highest| highest.saturating_add(1))
            .unwrap_or_else(|| {
                highest_executed_seq_num
                    .map(|highest| highest.saturating_add(1))
                    .unwrap_or(0)
            });

        let mut i = next_to_exec;

        while i < latest_synced_checkpoint.sequence_number() {
            let checkpoint = self
                .checkpoint_store
                .get_checkpoint_by_sequence_number(i)?
                .unwrap_or_else(|| {
                    panic!(
                        "Checkpoint sequence number {:?} does not exist in checkpoint store",
                        i
                    )
                });

            if !self.should_schedule_checkpoint(&checkpoint, epoch_store.epoch(), pending) {
                return Ok(());
            }

            self.schedule_checkpoint(checkpoint, pending, epoch_store.clone())?;
            i += 1;
        }

        if i == latest_synced_checkpoint.sequence_number()
            && self.should_schedule_checkpoint(
                &latest_synced_checkpoint,
                epoch_store.epoch(),
                pending,
            )
        {
            self.schedule_checkpoint(latest_synced_checkpoint, pending, epoch_store)?;
        }

        Ok(())
    }

    fn should_schedule_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
        epoch: EpochId,
        pending: &CheckpointExecutionBuffer,
    ) -> bool {
        epoch == checkpoint.epoch()
            && pending.len() < self.config.checkpoint_execution_max_concurrency
    }

    fn schedule_checkpoint(
        &mut self,
        checkpoint: VerifiedCheckpoint,
        pending: &mut CheckpointExecutionBuffer,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        debug!(
            "Scheduling checkpoint {:?} for execution",
            checkpoint.sequence_number()
        );
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

        let next_committee = checkpoint.summary().next_epoch_committee.clone();
        let highest_scheduled = checkpoint.sequence_number();
        let metrics = self.metrics.clone();
        let local_execution_timeout_sec = self.config.local_execution_timeout_sec;
        let authority_store = self.authority_store.clone();
        let checkpoint_store = self.checkpoint_store.clone();
        let tx_manager = self.tx_manager.clone();

        pending.push_back(spawn_monitored_task!(async move {
            while let Err(err) = execute_checkpoint(
                checkpoint.clone(),
                authority_store.clone(),
                checkpoint_store.clone(),
                epoch_store.clone(),
                tx_manager.clone(),
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

            (checkpoint, next_committee)
        }));

        self.highest_scheduled_seq_num = Some(highest_scheduled);

        Ok(())
    }

    fn create_committee_object(
        &self,
        last_checkpoint: VerifiedCheckpoint,
        next_committee: Vec<(AuthorityPublicKeyBytes, u64)>,
    ) -> Committee {
        let next_epoch = last_checkpoint.epoch().saturating_add(1);
        Committee::new(next_epoch, next_committee.into_iter().collect())
            .unwrap_or_else(|err| panic!("Failed to create new committee object: {:?}", err))
    }
}

#[instrument(level = "error", skip_all, fields(seq = ?checkpoint.sequence_number(), epoch = ?epoch_store.epoch()))]
pub async fn execute_checkpoint(
    checkpoint: VerifiedCheckpoint,
    authority_store: Arc<AuthorityStore>,
    checkpoint_store: Arc<CheckpointStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    transaction_manager: Arc<TransactionManager>,
    local_execution_timeout_sec: u64,
    metrics: &Arc<CheckpointExecutorMetrics>,
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

    let tx_count = txes.len();
    debug!(
        epoch=?epoch_store.epoch(),
        checkpoint_sequence=?checkpoint.sequence_number(),
        "Number of transactions in the checkpoint: {:?}",
        tx_count
    );
    metrics.checkpoint_transaction_count.report(tx_count as u64);

    execute_transactions(
        txes,
        authority_store,
        epoch_store,
        transaction_manager,
        local_execution_timeout_sec,
        checkpoint.sequence_number(),
    )
    .await
}

async fn execute_transactions(
    execution_digests: Vec<ExecutionDigests>,
    authority_store: Arc<AuthorityStore>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    transaction_manager: Arc<TransactionManager>,
    log_timeout_sec: u64,
    checkpoint_sequence: CheckpointSequenceNumber,
) -> SuiResult {
    let all_tx_digests: Vec<TransactionDigest> =
        execution_digests.iter().map(|tx| tx.transaction).collect();

    let synced_txns: Vec<VerifiedCertificate> = authority_store
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
            (fx.transaction_digest, fx)
        })
        .collect();

    for tx in synced_txns.clone() {
        if tx.contains_shared_object() {
            epoch_store
                .acquire_shared_locks_from_effects(
                    &tx,
                    digest_to_effects.get(tx.digest()).unwrap(),
                    &authority_store,
                )
                .await?;
        }
    }
    epoch_store.insert_pending_certificates(&synced_txns)?;

    transaction_manager.enqueue(synced_txns, &epoch_store)?;

    // Once synced_txns have been awaited, all txns should have effects committed.
    let mut periods = 1;
    let log_timeout_sec = Duration::from_secs(log_timeout_sec);

    loop {
        let effects_future = authority_store.notify_read_effects(all_tx_digests.clone());

        match timeout(log_timeout_sec, effects_future).await {
            Err(_elapsed) => {
                let missing_digests: Vec<TransactionDigest> =
                    EffectsStore::get_effects(&authority_store, all_tx_digests.clone().iter())
                        .expect("Failed to get effects")
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

                warn!(
                    "Transaction effects for tx digests {:?} checkpoint not present within {:?}. ",
                    missing_digests,
                    log_timeout_sec * periods,
                );
                periods += 1;
            }
            Ok(Err(err)) => return Err(err),
            Ok(Ok(_)) => {
                authority_store.insert_executed_transactions(
                    &all_tx_digests,
                    epoch_store.epoch(),
                    checkpoint_sequence,
                )?;
                return Ok(());
            }
        }
    }
}
