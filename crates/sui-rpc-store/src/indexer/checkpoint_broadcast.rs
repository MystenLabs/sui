// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that re-publishes each ingested checkpoint to
//! a [`tokio::sync::broadcast`] channel, in checkpoint order.
//!
//! Unlike the other pipelines this one writes nothing to the
//! [`RpcStoreSchema`](crate::RpcStoreSchema): its "batch" *is* the
//! [`Checkpoint`] itself (`type Batch = Option<Arc<Checkpoint>>`). The
//! framework's sequential committer drives `commit` in strict
//! checkpoint order, and — because pipelines registered with the
//! [`Synchronizer`](sui_consistent_store::Synchronizer) set
//! `MAX_BATCH_CHECKPOINTS = 1` — each `commit` corresponds to exactly
//! one checkpoint. So sending on the broadcast channel from `commit`
//! yields a gap-free, in-order checkpoint stream.
//!
//! This is the standalone `sui-rpc-node`'s analog of the fullnode's
//! checkpoint-executor broadcast: it lets the node host
//! `sui-rpc-api`'s checkpoint-subscription service over the same
//! checkpoints it indexes. The pipeline is *not* part of
//! [`PipelineLayer`](crate::config::PipelineLayer) — it carries a
//! runtime `broadcast::Sender` rather than a config toggle, so callers
//! register it explicitly via
//! [`Indexer::add_checkpoint_broadcast`](crate::Indexer::add_checkpoint_broadcast).

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::full_checkpoint_content::Checkpoint;
use tokio::sync::broadcast;

use crate::indexer::Schema;
use crate::indexer::Store;

/// Pipeline that broadcasts each committed checkpoint. Holds the
/// send half of the broadcast channel the subscription service reads
/// from.
pub struct CheckpointBroadcast {
    sender: broadcast::Sender<Arc<Checkpoint>>,
}

impl CheckpointBroadcast {
    /// Pipeline name; also the `__watermark` key the synchronizer
    /// tracks this pipeline under.
    pub const NAME: &'static str = "checkpoint_broadcast";

    pub fn new(sender: broadcast::Sender<Arc<Checkpoint>>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl Processor for CheckpointBroadcast {
    const NAME: &'static str = CheckpointBroadcast::NAME;
    type Value = Arc<Checkpoint>;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        // Cheap: clones the `Arc`, not the checkpoint. The payload is
        // carried to `commit` (rather than reloaded from the store)
        // because the framework already has it in hand here.
        Ok(vec![checkpoint.clone()])
    }
}

#[async_trait]
impl sequential::Handler for CheckpointBroadcast {
    type Store = Store;
    /// The "batch" is the checkpoint to broadcast. `Option` because
    /// `Batch` must be `Default`; `MAX_BATCH_CHECKPOINTS = 1` (enforced
    /// at registration) guarantees at most one checkpoint lands here
    /// per commit.
    type Batch = Option<Arc<Checkpoint>>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
        // With one checkpoint per batch there is exactly one value;
        // `last()` also stays correct if batching is ever widened
        // (the highest checkpoint is the one to publish).
        if let Some(checkpoint) = values.last() {
            *batch = Some(checkpoint);
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        _conn: &mut sui_consistent_store::Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        // No CF writes: the framework still advances this pipeline's
        // watermark atomically through the connection, so the
        // synchronizer commits an (otherwise empty) ordered batch.
        let Some(checkpoint) = batch else {
            return Ok(0);
        };
        // `send` errors only when there are no live subscribers, which
        // is the common idle case — not a failure. Drop it.
        let _ = self.sender.send(checkpoint.clone());
        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use sui_indexer_alt_framework::pipeline::sequential::Handler as _;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_emits_the_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(7).build_checkpoint());
        let sender = broadcast::channel(16).0;
        let values = CheckpointBroadcast::new(sender)
            .process(&checkpoint)
            .await
            .unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].summary.sequence_number, 7);
    }

    #[tokio::test]
    async fn batch_keeps_the_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(9).build_checkpoint());
        let handler = CheckpointBroadcast::new(broadcast::channel(16).0);
        let mut batch = None;
        handler.batch(&mut batch, vec![checkpoint].into_iter());
        assert_eq!(batch.unwrap().summary.sequence_number, 9);
    }
}
