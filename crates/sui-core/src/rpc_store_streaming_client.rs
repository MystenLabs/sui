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
use tokio::sync::broadcast;

/// A [`CheckpointStreamingClient`] that subscribes to the executor's checkpoint
/// broadcast channel. Each `connect` takes a fresh subscription, so the stream
/// only carries checkpoints published after the call.
pub struct BroadcastStreamingClient {
    sender: broadcast::Sender<Arc<Checkpoint>>,
    chain_id: ChainIdentifier,
}

impl BroadcastStreamingClient {
    pub fn new(sender: broadcast::Sender<Arc<Checkpoint>>, chain_id: ChainIdentifier) -> Self {
        Self { sender, chain_id }
    }
}

#[async_trait]
impl CheckpointStreamingClient for BroadcastStreamingClient {
    async fn connect(&mut self) -> Result<CheckpointStream, Error> {
        let receiver = self.sender.subscribe();

        // A `Lagged` receiver missed checkpoints, so we surface a stream error
        // rather than silently skipping ahead: the framework reconnects and
        // fills the gap from the ingestion client. `Closed` ends the stream.
        let stream = stream::unfold(receiver, |mut receiver| async move {
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
        })
        .boxed();

        Ok(CheckpointStream {
            stream: tokio_stream::StreamExt::peekable(stream),
            chain_id: self.chain_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use sui_types::digests::CheckpointDigest;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    fn test_chain_id() -> ChainIdentifier {
        CheckpointDigest::new([3u8; 32]).into()
    }

    fn checkpoint(sequence_number: u64) -> Arc<Checkpoint> {
        Arc::new(TestCheckpointBuilder::new(sequence_number).build_checkpoint())
    }

    #[tokio::test]
    async fn streams_published_checkpoints_in_order() {
        let (sender, _keep) = broadcast::channel(16);
        let mut client = BroadcastStreamingClient::new(sender.clone(), test_chain_id());

        let mut connected = client.connect().await.unwrap();
        assert_eq!(connected.chain_id, test_chain_id());

        sender.send(checkpoint(7)).unwrap();
        sender.send(checkpoint(8)).unwrap();

        let first = connected.stream.next().await.unwrap().unwrap();
        let second = connected.stream.next().await.unwrap().unwrap();
        assert_eq!(*first.summary.sequence_number(), 7);
        assert_eq!(*second.summary.sequence_number(), 8);
    }

    #[tokio::test]
    async fn lagging_receiver_yields_a_stream_error() {
        // Capacity 2: publishing four checkpoints before reading drops the two
        // oldest, so the first read observes `Lagged`.
        let (sender, _keep) = broadcast::channel(2);
        let mut client = BroadcastStreamingClient::new(sender.clone(), test_chain_id());
        let mut connected = client.connect().await.unwrap();

        for seq in 0..4 {
            sender.send(checkpoint(seq)).unwrap();
        }

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
        let mut client = BroadcastStreamingClient::new(sender.clone(), test_chain_id());
        let mut connected = client.connect().await.unwrap();

        sender.send(checkpoint(1)).unwrap();
        // Drop every sender (the original plus the clone the client holds) so
        // the channel closes once the buffered checkpoint is drained.
        drop(sender);
        drop(client);

        assert_eq!(
            *connected
                .stream
                .next()
                .await
                .unwrap()
                .unwrap()
                .summary
                .sequence_number(),
            1
        );
        assert!(connected.stream.next().await.is_none());
    }
}
