// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use sui_rpc::proto::sui::rpc::v2::{
    SubscribeCheckpointsRequest, subscription_service_client::SubscriptionServiceClient,
};
use sui_rpc_api::client::checkpoint_data_field_mask;
use tonic::{Status, transport::Uri};

use crate::ingestion::error::{Error, Result};
use crate::types::full_checkpoint_content::Checkpoint;

/// Type alias for a stream of checkpoint data.
pub type CheckpointStream = Pin<Box<dyn Stream<Item = Result<Checkpoint>> + Send>>;

/// Trait representing a client for streaming checkpoint data.
#[async_trait]
pub trait CheckpointStreamingClient {
    async fn connect(&mut self) -> Result<CheckpointStream>;
}

/// gRPC-based implementation of the CheckpointStreamingClient trait.
pub struct GrpcStreamingClient {
    uri: Uri,
}

impl GrpcStreamingClient {
    pub fn new(uri: Uri) -> Self {
        Self { uri }
    }
}

#[async_trait]
impl CheckpointStreamingClient for GrpcStreamingClient {
    async fn connect(&mut self) -> Result<CheckpointStream> {
        let mut client = SubscriptionServiceClient::connect(self.uri.clone())
            .await
            .map_err(|err| Error::RpcClientError(Status::from_error(err.into())))?;

        let mut request = SubscribeCheckpointsRequest::default();
        request.read_mask = Some(checkpoint_data_field_mask());

        let stream = client
            .subscribe_checkpoints(request)
            .await
            .map_err(Error::RpcClientError)?
            .into_inner();

        let converted_stream = stream.map(|result| match result {
            Ok(response) => response
                .checkpoint
                .context("Checkpoint data missing in response")
                .and_then(|checkpoint| {
                    sui_types::full_checkpoint_content::Checkpoint::try_from(&checkpoint)
                        .context("Failed to parse checkpoint")
                })
                .map_err(Error::StreamingError),
            Err(e) => Err(Error::RpcClientError(e)),
        });

        Ok(Box::pin(converted_stream))
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use crate::types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use std::sync::{Arc, Mutex};

    struct MockStreamState {
        checkpoints: Arc<Mutex<Vec<Result<u64>>>>,
    }

    impl Stream for MockStreamState {
        type Item = Result<Checkpoint>;

        fn poll_next(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            let mut checkpoints = self.checkpoints.lock().unwrap();
            if checkpoints.is_empty() {
                return std::task::Poll::Ready(None);
            }
            let result = checkpoints.remove(0);
            std::task::Poll::Ready(Some(result.map(|seq| {
                let mut builder = TestCheckpointBuilder::new(seq);
                builder.build_checkpoint()
            })))
        }
    }

    /// Mock streaming client for testing with predefined checkpoints.
    pub struct MockStreamingClient {
        checkpoints: Arc<Mutex<Vec<Result<u64>>>>,
        connection_failures_remaining: usize,
    }

    impl MockStreamingClient {
        pub fn new<I>(checkpoint_range: I) -> Self
        where
            I: IntoIterator<Item = u64>,
        {
            Self {
                checkpoints: Arc::new(Mutex::new(checkpoint_range.into_iter().map(Ok).collect())),
                connection_failures_remaining: 0,
            }
        }

        /// Make `connect` fail for the next N calls
        pub fn fail_connection_times(mut self, times: usize) -> Self {
            self.connection_failures_remaining = times;
            self
        }

        /// Insert an error at the back of the queue.
        pub fn insert_error(&mut self) {
            self.checkpoints
                .lock()
                .unwrap()
                .push(Err(Error::StreamingError(anyhow::anyhow!(
                    "Mock streaming error"
                ))));
        }

        /// Insert a checkpoint at the back of the queue.
        pub fn insert_checkpoint(&mut self, sequence_number: u64) {
            self.insert_checkpoint_range([sequence_number])
        }

        pub fn insert_checkpoint_range<I>(&mut self, checkpoint_range: I)
        where
            I: IntoIterator<Item = u64>,
        {
            let mut checkpoints = self.checkpoints.lock().unwrap();
            for sequence_number in checkpoint_range {
                checkpoints.push(Ok(sequence_number));
            }
        }
    }

    #[async_trait]
    impl CheckpointStreamingClient for MockStreamingClient {
        async fn connect(&mut self) -> Result<CheckpointStream> {
            if self.connection_failures_remaining > 0 {
                self.connection_failures_remaining -= 1;
                return Err(Error::StreamingError(anyhow::anyhow!(
                    "Mock connection failure"
                )));
            }
            let stream = MockStreamState {
                checkpoints: Arc::clone(&self.checkpoints),
            };

            Ok(Box::pin(stream))
        }
    }
}
