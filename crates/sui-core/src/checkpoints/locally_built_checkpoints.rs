// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Publishes the checkpoints a validator builds as soon as it builds them, before the network
//! certifies them, and the ones it executes without having built them first.

use std::sync::Arc;

use async_trait::async_trait;
use mysten_common::ZipDebugEqIteratorExt;
use mysten_metrics::spawn_monitored_task;
use sui_types::error::{SuiErrorKind, SuiResult};
use sui_types::full_checkpoint_content::LocallyBuiltCheckpoint;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, CheckpointSummary,
};
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

use super::checkpoint_executor::load_executed_transactions;
use super::{CheckpointOutput, CheckpointStore};
use crate::authority::AuthorityState;
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;

/// Checkpoints that can wait to be loaded and published. Neither the checkpoint builder nor the
/// executor waits for the publisher: a checkpoint that does not fit is dropped, which
/// subscribers see as a gap.
const MAX_PENDING_CHECKPOINTS: usize = 128;

/// Hands the summary of every checkpoint this validator builds or executes to a background task,
/// which loads the checkpoint's contents and full data and broadcasts it to subscribers. A
/// checkpoint reaches the task twice, once when built and once when executed after
/// certification; it is published the first time. Clones share the task, which stops once every
/// clone is dropped.
#[derive(Clone)]
pub struct LocallyBuiltCheckpointPublisher {
    sender: mpsc::Sender<CheckpointSummary>,
}

impl LocallyBuiltCheckpointPublisher {
    /// Published checkpoints have strictly increasing sequence numbers. Checkpoints up to
    /// `highest_executed_checkpoint`, the highest checkpoint this node had executed when it
    /// started, are never published: after a restart the builder can rebuild checkpoints this
    /// node already executed, and their objects may already be pruned. Checkpoints rebuilt above
    /// it are published, so a client that connects right after a restart can receive checkpoints
    /// that were already streamed before it.
    pub fn spawn(
        state: Arc<AuthorityState>,
        highest_executed_checkpoint: Option<CheckpointSequenceNumber>,
        subscribers: broadcast::Sender<Arc<LocallyBuiltCheckpoint>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(MAX_PENDING_CHECKPOINTS);
        spawn_monitored_task!(publish_checkpoints(
            receiver,
            state,
            highest_executed_checkpoint,
            subscribers,
        ));
        Self { sender }
    }

    /// Wraps `output` so that every checkpoint it handles is also published.
    pub fn wrap_output(&self, output: Box<dyn CheckpointOutput>) -> Box<dyn CheckpointOutput> {
        Box::new(PublishLocallyBuiltCheckpoint {
            output,
            publisher: self.clone(),
        })
    }

    pub(crate) fn publish(&self, summary: &CheckpointSummary) {
        match self.sender.try_send(summary.clone()) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => warn!(
                checkpoint = summary.sequence_number,
                "locally built checkpoint publisher is backed up; dropping checkpoint"
            ),
            Err(mpsc::error::TrySendError::Closed(_)) => warn!(
                checkpoint = summary.sequence_number,
                "locally built checkpoint publisher stopped; dropping checkpoint"
            ),
        }
    }
}

struct PublishLocallyBuiltCheckpoint {
    output: Box<dyn CheckpointOutput>,
    publisher: LocallyBuiltCheckpointPublisher,
}

#[async_trait]
impl CheckpointOutput for PublishLocallyBuiltCheckpoint {
    async fn checkpoint_created(
        &self,
        summary: &CheckpointSummary,
        contents: &CheckpointContents,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        checkpoint_store: &Arc<CheckpointStore>,
    ) -> SuiResult {
        // The wrapped output runs first, so a validator submits its checkpoint signature before
        // spending anything on publishing.
        self.output
            .checkpoint_created(summary, contents, epoch_store, checkpoint_store)
            .await?;
        self.publisher.publish(summary);
        Ok(())
    }
}

async fn publish_checkpoints(
    mut receiver: mpsc::Receiver<CheckpointSummary>,
    state: Arc<AuthorityState>,
    mut last_published: Option<CheckpointSequenceNumber>,
    subscribers: broadcast::Sender<Arc<LocallyBuiltCheckpoint>>,
) {
    while let Some(summary) = receiver.recv().await {
        let sequence_number = summary.sequence_number;
        if last_published.is_some_and(|last| sequence_number <= last) {
            continue;
        }
        // Set before loading, so a checkpoint that fails to load is skipped rather than retried.
        last_published = Some(sequence_number);

        match load_locally_built_checkpoint(&state, summary).await {
            Ok(checkpoint) => {
                // A send error only means that the subscription service has stopped.
                let _ = subscribers.send(Arc::new(checkpoint));
            }
            Err(error) => warn!(
                checkpoint = sequence_number,
                ?error,
                "failed to load locally built checkpoint; subscribers will see a gap"
            ),
        }
    }
}

/// Waits until every transaction in the checkpoint has executed, then reads its contents,
/// transactions, effects, events, and objects. The builder includes the epoch's advance-epoch
/// transaction without executing it; the checkpoint executor executes it after the network
/// certifies the epoch's last checkpoint. Every other transaction has executed by the time its
/// checkpoint is built, so this only waits on the last checkpoint of an epoch, which is therefore
/// published after certification.
async fn load_locally_built_checkpoint(
    state: &AuthorityState,
    summary: CheckpointSummary,
) -> SuiResult<LocallyBuiltCheckpoint> {
    let contents: CheckpointContents = state.get_checkpoint_contents(summary.content_digest)?;
    let tx_digests: Vec<_> = contents.iter().map(|digests| digests.transaction).collect();

    let cache = state.get_transaction_cache_reader();
    let effects = cache
        .notify_read_executed_effects_may_fail(
            "LocallyBuiltCheckpointPublisher::load_checkpoint",
            &tx_digests,
        )
        .await?;
    let transactions = cache
        .multi_get_transaction_blocks(&tx_digests)
        .into_iter()
        .zip_debug_eq(&tx_digests)
        .map(|(transaction, digest)| {
            transaction.ok_or_else(|| SuiErrorKind::TransactionNotFound { digest: *digest }.into())
        })
        .collect::<SuiResult<Vec<_>>>()?;

    let (transactions, object_set) = load_executed_transactions(
        transactions.iter().map(|transaction| transaction.data()),
        &effects,
        state.get_object_store().as_ref(),
        cache.as_ref(),
    )?;

    Ok(LocallyBuiltCheckpoint {
        summary,
        contents,
        transactions,
        object_set,
    })
}
