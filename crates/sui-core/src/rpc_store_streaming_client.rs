// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`CheckpointStreamingClient`] backed by the checkpoint executor's broadcast
//! stream.
//!
//! The embedded `sui-rpc-store` indexer consumes the tip of the chain from the
//! same `tokio::sync::broadcast` channel the executor publishes to (see
//! `CheckpointExecutor`), avoiding a gRPC round-trip to the node's own
//! subscription endpoint. The ingestion framework only engages the streaming
//! client once the indexer is within catch-up range of the tip; any gap (for
//! example after the receiver lags) surfaces as a stream error, which the
//! framework fills from the [`PerpetualStoreIngestionClient`] instead.
//!
//! [`CheckpointStreamingClient`]:
//!     sui_indexer_alt_framework::ingestion::streaming_client::CheckpointStreamingClient
//! [`PerpetualStoreIngestionClient`]:
//!     crate::rpc_store_ingestion_client::PerpetualStoreIngestionClient

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use futures::StreamExt;
use futures::stream;
use sui_indexer_alt_framework::ingestion::error::Error;
use sui_indexer_alt_framework::ingestion::streaming_client::CheckpointStream;
use sui_indexer_alt_framework::ingestion::streaming_client::CheckpointStreamingClient;
use sui_types::digests::ChainIdentifier;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::storage::ReadStore;
use tokio::sync::broadcast;

/// A [`CheckpointStreamingClient`] that subscribes to the executor's checkpoint
/// broadcast channel. `connect` seeds the stream with the current tip read from
/// the local store, then follows the broadcast for checkpoints published after
/// the call.
///
/// The tip seed is essential: a fresh `tokio::sync::broadcast` subscription only
/// carries checkpoints published *after* it is taken, so on an idle chain the
/// stream would be empty and the framework's `peek()` -- which it uses to learn
/// the network tip before starting ingestion -- would block forever. Reading the
/// tip from the local store mirrors a gRPC stream that opens at the latest
/// checkpoint, letting ingestion fill `[start, tip)` immediately while streaming
/// takes over from `tip`.
///
/// `R` is the local checkpoint store (the fullnode's [`RocksDbStore`] in
/// production); the generic bound keeps it unit-testable against an in-memory
/// store.
///
/// [`RocksDbStore`]: crate::storage::RocksDbStore
pub struct BroadcastStreamingClient<R> {
    sender: broadcast::Sender<Arc<Checkpoint>>,
    chain_id: ChainIdentifier,
    store: R,
}

impl<R> BroadcastStreamingClient<R> {
    pub fn new(
        sender: broadcast::Sender<Arc<Checkpoint>>,
        chain_id: ChainIdentifier,
        store: R,
    ) -> Self {
        Self {
            sender,
            chain_id,
            store,
        }
    }
}

impl<R: ReadStore> BroadcastStreamingClient<R> {
    /// Read the current tip's full checkpoint from the local store.
    fn current_tip(&self) -> Result<Checkpoint, Error> {
        let seq = self
            .store
            .get_latest_checkpoint_sequence_number()
            .map_err(|e| Error::StreamingError(e.into()))?;
        let summary = self
            .store
            .get_checkpoint_by_sequence_number(seq)
            .ok_or_else(|| Error::StreamingError(anyhow!("checkpoint {seq} summary missing")))?;
        let contents = self
            .store
            .get_checkpoint_contents_by_digest(&summary.content_digest)
            .ok_or_else(|| Error::StreamingError(anyhow!("checkpoint {seq} contents missing")))?;
        self.store
            .get_checkpoint_data(summary, contents)
            .map_err(|e| Error::StreamingError(e.into()))
    }
}

#[async_trait]
impl<R: ReadStore + Send + Sync + 'static> CheckpointStreamingClient
    for BroadcastStreamingClient<R>
{
    async fn connect(&self) -> Result<CheckpointStream, Error> {
        // Subscribe before reading the tip so no checkpoint published between
        // the two is missed (the broadcast only carries checkpoints published
        // after `subscribe`).
        let receiver = self.sender.subscribe();

        // Seed the stream with the current tip so `peek()` resolves immediately
        // even on an idle chain (see the type-level docs).
        let tip = self.current_tip()?;

        // A `Lagged` receiver missed checkpoints, so we surface a stream error
        // rather than silently skipping ahead: the framework reconnects and
        // fills the gap from the ingestion client. `Closed` ends the stream.
        let live = stream::unfold(receiver, |mut receiver| async move {
            match receiver.recv().await {
                Ok(checkpoint) => Some((Ok((*checkpoint).clone()), receiver)),
                Err(broadcast::error::RecvError::Lagged(skipped)) => Some((
                    Err(Error::StreamingError(anyhow!(
                        "broadcast stream lagged by {skipped} checkpoints"
                    ))),
                    receiver,
                )),
                Err(broadcast::error::RecvError::Closed) => None,
            }
        });

        // The broadcaster skips checkpoints below its watermark and breaks to
        // ingestion on a gap, so a tip that duplicates or precedes the first
        // live checkpoint needs no special handling here.
        let stream = stream::once(async move { Ok(tip) }).chain(live).boxed();

        Ok(CheckpointStream {
            stream: tokio_stream::StreamExt::peekable(stream),
            chain_id: self.chain_id,
        })
    }

    async fn latest_checkpoint_number(&self) -> Result<u64, Error> {
        // Read the local store directly; the default trait impl peeks the
        // stream, which blocks on an idle chain (see `connect`).
        self.store
            .get_latest_checkpoint_sequence_number()
            .map_err(|e| Error::StreamingError(e.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc_store_test_utils::checkpoint;
    use crate::rpc_store_test_utils::store_with;
    use crate::rpc_store_test_utils::test_chain_id;

    #[tokio::test]
    async fn seeds_tip_then_streams_published_checkpoints_in_order() {
        let (sender, _keep) = broadcast::channel(16);
        // The local store is at checkpoint 5; the broadcast then publishes 6, 7.
        let client =
            BroadcastStreamingClient::new(sender.clone(), test_chain_id(), store_with([5]));

        let mut connected = client.connect().await.unwrap();
        assert_eq!(connected.chain_id, test_chain_id());

        sender.send(checkpoint(6)).unwrap();
        sender.send(checkpoint(7)).unwrap();

        // The stream opens at the local tip, then follows the broadcast.
        let tip = connected.stream.next().await.unwrap().unwrap();
        let second = connected.stream.next().await.unwrap().unwrap();
        let third = connected.stream.next().await.unwrap().unwrap();
        assert_eq!(*tip.summary.sequence_number(), 5);
        assert_eq!(*second.summary.sequence_number(), 6);
        assert_eq!(*third.summary.sequence_number(), 7);
    }

    #[tokio::test]
    async fn latest_checkpoint_number_reads_the_local_tip() {
        let (sender, _keep) = broadcast::channel(16);
        let client = BroadcastStreamingClient::new(sender, test_chain_id(), store_with([0, 4, 9]));
        assert_eq!(client.latest_checkpoint_number().await.unwrap(), 9);
    }

    #[tokio::test]
    async fn lagging_receiver_yields_a_stream_error() {
        // Capacity 2: publishing four checkpoints before reading drops the two
        // oldest, so the first live read observes `Lagged`.
        let (sender, _keep) = broadcast::channel(2);
        // Tip 9 is distinct from the published 0..4 so we can tell them apart.
        let client =
            BroadcastStreamingClient::new(sender.clone(), test_chain_id(), store_with([9]));
        let mut connected = client.connect().await.unwrap();

        for seq in 0..4 {
            sender.send(checkpoint(seq)).unwrap();
        }

        // The seeded tip comes first, then the lag surfaces on the live stream.
        let tip = connected.stream.next().await.unwrap().unwrap();
        assert_eq!(*tip.summary.sequence_number(), 9);
        assert!(matches!(
            connected.stream.next().await,
            Some(Err(Error::StreamingError(_)))
        ));
        // After the lag the still-buffered checkpoints are delivered in order.
        let next = connected.stream.next().await.unwrap().unwrap();
        assert_eq!(*next.summary.sequence_number(), 2);
    }

    #[tokio::test]
    async fn stream_ends_when_all_senders_dropped() {
        let (sender, _initial_rx) = broadcast::channel(16);
        let client =
            BroadcastStreamingClient::new(sender.clone(), test_chain_id(), store_with([5]));
        let mut connected = client.connect().await.unwrap();

        sender.send(checkpoint(6)).unwrap();
        // Drop every sender (the original plus the clone the client holds) so
        // the channel closes once the buffered checkpoint is drained.
        drop(sender);
        drop(client);

        // The seeded tip, then the buffered live checkpoint, then end-of-stream.
        let tip = connected.stream.next().await.unwrap().unwrap();
        assert_eq!(*tip.summary.sequence_number(), 5);
        let live = connected.stream.next().await.unwrap().unwrap();
        assert_eq!(*live.summary.sequence_number(), 6);
        assert!(connected.stream.next().await.is_none());
    }
}
