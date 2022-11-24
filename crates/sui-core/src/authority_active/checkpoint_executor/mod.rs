// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc, time::Duration};

use itertools::Itertools;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, TransactionEffectsDigest},
    committee::StakeUnit,
    error::{SuiError, SuiResult},
    message_envelope::Message,
    messages::{TransactionEffects, VerifiedCertificate},
    messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint},
};
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{error, info, warn};
use typed_store::rocks::TypedStoreError;

use crate::{
    authority::{AuthorityState, EffectsNotifyRead},
    checkpoints::{CheckpointMetrics, CheckpointStore},
};

pub struct CheckpointExecutor {
    mailbox: Box<broadcast::Receiver<VerifiedCheckpoint>>,
    checkpoint_store: Arc<CheckpointStore>,
    authority_state: Arc<AuthorityState>,
    highest_executed_checkpoint: Box<Option<CheckpointSequenceNumber>>,
    metrics: Arc<CheckpointMetrics>,
}

impl CheckpointExecutor {
    pub fn new(
        state_sync_handle: &sui_network::state_sync::Handle,
        checkpoint_store: Arc<CheckpointStore>,
        // (Note to Self): this can be passed in from AuthorityState in SuiNode::start()
        // transaction_manager: Arc<Mutex<TransactionManager>>,
        authority_state: Arc<AuthorityState>,
        metrics: Arc<CheckpointMetrics>,
    ) -> Result<Self, TypedStoreError> {
        let mailbox = Box::new(state_sync_handle.subscribe_to_synced_checkpoints());
        let highest_executed_checkpoint =
            Box::new(checkpoint_store.get_highest_executed_checkpoint_seq_number()?);

        Ok(Self {
            mailbox,
            checkpoint_store,
            highest_executed_checkpoint,
            authority_state,
            metrics,
        })
    }

    pub async fn run(mut self) {
        loop {
            match self.mailbox.recv().await {
                Ok(checkpoint) => {
                    let last_to_exec = checkpoint.sequence_number() as u64;
                    let next_to_exec = self.highest_executed_checkpoint.unwrap_or(0) + 1;

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
                        assert!(
                            next_checkpoint.epoch()
                                != self
                                    .authority_state
                                    .committee_store()
                                    .get_latest_committee()
                                    .epoch()
                        );

                        while let Err(err) = self.execute_checkpoint(&next_checkpoint).await {
                            error!(
                                "Error while executing checkpoint, will retry in 1s: {:?}",
                                err
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            self.metrics.checkpoint_exec_errors.inc();
                            continue;
                        }
                        *self.highest_executed_checkpoint =
                            Some(seq_num as CheckpointSequenceNumber);
                        self.metrics.last_executed_checkpoint.set(seq_num as i64);
                        self.update_lag_metric(seq_num)
                            .unwrap_or_else(|err| error!("Update lag metric error: {:?}", err));

                        // Last checkpoint of epoch
                        if let Some(next_committee) = next_checkpoint.next_epoch_committee() {
                            // Unwrap ok because reconfig failure is unrecoverable
                            self.reconfig(next_committee).await.unwrap();
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
            }
        }
    }

    pub async fn execute_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> SuiResult<Vec<TransactionEffects>> {
        let effects = if let Some(checkpoint_contents) = self
            .checkpoint_store
            .get_checkpoint_contents(&checkpoint.content_digest())
            .map_err(SuiError::from)?
        {
            let seq_number = checkpoint.summary().sequence_number;
            let txes = checkpoint_contents.into_inner();
            self.execute_and_verify_transactions(txes, seq_number)
                .await?
        } else {
            warn!(
                "Checkpoint contents empty: {:?}",
                checkpoint.content_digest()
            );
            Vec::new()
        };

        self.checkpoint_store
            .update_highest_executed_checkpoint(checkpoint)
            .map_err(SuiError::from)?;
        Ok(effects)
    }

    async fn execute_and_verify_transactions(
        &self,
        transactions: Vec<ExecutionDigests>,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult<Vec<TransactionEffects>> {
        let (tx_digests, expected_fx_digests): (Vec<_>, Vec<_>) = transactions
            .iter()
            .map(|tx| (tx.transaction, tx.effects))
            .unzip();

        // TODO Potentially need to aquire shared object locks for these txes?
        let mut verified_certs = Vec::<VerifiedCertificate>::new();
        for digest in tx_digests.iter() {
            match self.authority_state.database.read_certificate(digest)? {
                Some(cert) => verified_certs.push(cert),
                None => return Err(SuiError::TransactionNotFound { digest: *digest }),
            }
        }

        {
            let mut tm_guard = self.authority_state.transaction_manager.lock().await;
            tm_guard.enqueue(verified_certs).await?;
        }

        let actual_fx = self
            .authority_state
            .database
            .notify_read(tx_digests)
            .await?;
        let actual_fx_digests = actual_fx.iter().map(|fx| fx.digest());

        self.confirm_effects(
            HashSet::from_iter(expected_fx_digests.into_iter()),
            HashSet::from_iter(actual_fx_digests),
            sequence_number,
        )?;
        Ok(actual_fx)
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
        // TODO may want to consider asserting >= 0
        let diff = highest_synced - highest_executed;
        self.metrics.checkpoint_exec_lag.set(diff as i64);
        Ok(())
    }

    fn confirm_effects(
        &self,
        expected_fx_digests: HashSet<TransactionEffectsDigest>,
        actual_fx_digests: HashSet<TransactionEffectsDigest>,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult {
        let diff: HashSet<_> = actual_fx_digests.difference(&expected_fx_digests).collect();
        if diff.is_empty() {
            Ok(())
        } else {
            Err(SuiError::InvalidTransactionEffects {
                effects_digests: diff.into_iter().copied().collect_vec(),
                checkpoint: sequence_number,
            })
        }
    }
}
