// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;
use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use sui_rpc::proto::sui::rpc::v2::{
    SubscribeCheckpointsRequest, subscription_service_client::SubscriptionServiceClient,
};
use sui_rpc_api::client::checkpoint_data_field_mask;
use tonic::{
    Status,
    transport::{Endpoint, Uri},
};

use crate::ingestion::MAX_GRPC_MESSAGE_SIZE_BYTES;
use crate::ingestion::error::{Error, Result};
use crate::types::full_checkpoint_content::Checkpoint;

/// Type alias for a stream of checkpoint data.
pub type CheckpointStream = Pin<Box<dyn Stream<Item = Result<Checkpoint>> + Send>>;

/// Trait representing a client for streaming checkpoint data.
#[async_trait]
pub trait CheckpointStreamingClient {
    async fn connect(&mut self) -> Result<CheckpointStream>;
}

#[derive(clap::Args, Clone, Debug, Default)]
pub struct StreamingClientArgs {
    /// gRPC endpoint for streaming checkpoints
    #[clap(long, env)]
    pub streaming_url: Option<Uri>,
}

/// gRPC-based implementation of the CheckpointStreamingClient trait.
pub struct GrpcStreamingClient {
    uri: Uri,
    connection_timeout: Duration,
}

impl GrpcStreamingClient {
    pub fn new(uri: Uri, connection_timeout: Duration) -> Self {
        Self {
            uri,
            connection_timeout,
        }
    }
}

#[async_trait]
impl CheckpointStreamingClient for GrpcStreamingClient {
    async fn connect(&mut self) -> Result<CheckpointStream> {
        let endpoint = Endpoint::from(self.uri.clone()).connect_timeout(self.connection_timeout);

        let mut client = SubscriptionServiceClient::connect(endpoint)
            .await
            .map_err(|err| Error::RpcClientError(Status::from_error(err.into())))?
            .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE_BYTES);

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
    use std::time::{Duration, Instant};

    enum StreamAction {
        Checkpoint(u64),
        Error,
        Timeout {
            deadline: Option<Instant>,
            duration: Duration,
        },
    }

    struct MockStreamState {
        actions: Arc<Mutex<Vec<StreamAction>>>,
    }

    impl Stream for MockStreamState {
        type Item = Result<Checkpoint>;

        fn poll_next(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            let mut actions = self.actions.lock().unwrap();
            if actions.is_empty() {
                return std::task::Poll::Ready(None);
            }

            match &actions[0] {
                StreamAction::Checkpoint(seq) => {
                    let seq = *seq;
                    actions.remove(0);
                    let mut builder = TestCheckpointBuilder::new(seq);
                    std::task::Poll::Ready(Some(Ok(builder.build_checkpoint())))
                }
                StreamAction::Error => {
                    actions.remove(0);
                    std::task::Poll::Ready(Some(Err(Error::StreamingError(anyhow::anyhow!(
                        "Mock streaming error"
                    )))))
                }
                StreamAction::Timeout { deadline, duration } => match deadline {
                    None => {
                        let deadline = Instant::now() + *duration;
                        actions[0] = StreamAction::Timeout {
                            deadline: Some(deadline),
                            duration: *duration,
                        };
                        std::task::Poll::Pending
                    }
                    Some(deadline_instant) => {
                        if Instant::now() >= *deadline_instant {
                            actions.remove(0);
                            drop(actions);
                            self.poll_next(_cx)
                        } else {
                            std::task::Poll::Pending
                        }
                    }
                },
            }
        }
    }

    /// Mock streaming client for testing with predefined checkpoints.
    pub struct MockStreamingClient {
        actions: Arc<Mutex<Vec<StreamAction>>>,
        connection_failures_remaining: usize,
        connection_timeouts_remaining: usize,
        timeout_duration: Duration,
    }

    impl MockStreamingClient {
        pub fn new<I>(checkpoint_range: I, timeout_duration: Option<Duration>) -> Self
        where
            I: IntoIterator<Item = u64>,
        {
            Self {
                actions: Arc::new(Mutex::new(
                    checkpoint_range
                        .into_iter()
                        .map(StreamAction::Checkpoint)
                        .collect(),
                )),
                connection_failures_remaining: 0,
                connection_timeouts_remaining: 0,
                timeout_duration: timeout_duration.unwrap_or(Duration::from_secs(5)),
            }
        }

        /// Make `connect` fail for the next N calls
        pub fn fail_connection_times(mut self, times: usize) -> Self {
            self.connection_failures_remaining = times;
            self
        }

        /// Make `connect` timeout for the next N calls
        pub fn fail_connection_with_timeout(mut self, times: usize) -> Self {
            self.connection_timeouts_remaining = times;
            self
        }

        /// Insert an error at the back of the queue.
        pub fn insert_error(&mut self) {
            self.actions.lock().unwrap().push(StreamAction::Error);
        }

        /// Insert a timeout at the back of the queue (causes poll_next to return Pending).
        pub fn insert_timeout(&mut self) {
            self.insert_timeout_with_duration(self.timeout_duration)
        }

        /// Insert a timeout with custom duration.
        pub fn insert_timeout_with_duration(&mut self, duration: Duration) {
            self.actions.lock().unwrap().push(StreamAction::Timeout {
                deadline: None,
                duration,
            });
        }

        /// Insert a checkpoint at the back of the queue.
        pub fn insert_checkpoint(&mut self, sequence_number: u64) {
            self.insert_checkpoint_range([sequence_number])
        }

        pub fn insert_checkpoint_range<I>(&mut self, checkpoint_range: I)
        where
            I: IntoIterator<Item = u64>,
        {
            let mut actions = self.actions.lock().unwrap();
            for sequence_number in checkpoint_range {
                actions.push(StreamAction::Checkpoint(sequence_number));
            }
        }
    }

    #[async_trait]
    impl CheckpointStreamingClient for MockStreamingClient {
        async fn connect(&mut self) -> Result<CheckpointStream> {
            if self.connection_timeouts_remaining > 0 {
                self.connection_timeouts_remaining -= 1;
                // Simulate a connection timeout
                tokio::time::sleep(self.timeout_duration).await;
                return Err(Error::StreamingError(anyhow::anyhow!(
                    "Mock connection timeout"
                )));
            }
            if self.connection_failures_remaining > 0 {
                self.connection_failures_remaining -= 1;
                return Err(Error::StreamingError(anyhow::anyhow!(
                    "Mock connection failure"
                )));
            }
            let stream = MockStreamState {
                actions: Arc::clone(&self.actions),
            };

            Ok(Box::pin(stream))
        }
    }
}
