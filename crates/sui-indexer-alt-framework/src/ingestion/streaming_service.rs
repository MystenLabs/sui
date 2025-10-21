// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::{stream::Peekable, Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use sui_rpc::{
    field::{FieldMask, FieldMaskUtil},
    proto::sui::rpc::v2::{
        subscription_service_client::SubscriptionServiceClient, SubscribeCheckpointsRequest,
    },
};
use tonic::{transport::Uri, Status};

use crate::ingestion::error::{Error, Result};
use crate::types::full_checkpoint_content::CheckpointData;

#[async_trait]
pub trait StreamingService {
    type Stream: PeekableStream + Send;
    async fn connect(&mut self) -> Result<Self::Stream>;
}

#[async_trait]
pub trait PeekableStream: Stream<Item = Result<CheckpointData>> + Unpin {
    async fn peek(&mut self) -> Option<Result<CheckpointData>>;
}

pub struct GRPCStreamingService {
    uri: Uri,
}

/// Wrapper around Peekable stream that implements PeekableStream trait
pub struct GrpcPeekableStream {
    inner: Peekable<Pin<Box<dyn Stream<Item = Result<CheckpointData>> + Send>>>,
}

#[async_trait]
impl StreamingService for GRPCStreamingService {
    type Stream = GrpcPeekableStream;

    async fn connect(&mut self) -> Result<Self::Stream> {
        let mut client = SubscriptionServiceClient::connect(self.uri.clone())
            .await
            .map_err(|err| Error::RpcClientError(Status::from_error(err.into())))?;

        // Request all the fields we need to construct CheckpointData
        let mut request = SubscribeCheckpointsRequest::default();
        request.read_mask = Some(FieldMask::from_paths([
            "sequence_number",
            "summary.bcs",
            "signature",
            "contents.bcs",
            "transactions.transaction.bcs",
            "transactions.effects.bcs",
            "transactions.events.bcs",
            "transactions.input_objects.bcs",
            "transactions.output_objects.bcs",
        ]));

        let stream = client
            .subscribe_checkpoints(request)
            .await
            .map_err(Error::RpcClientError)?
            .into_inner();

        // We need to convert the stream items from the gRPC response type to CheckpointData.
        let converted_stream = stream.map(|result| match result {
            Ok(response) => response
                .checkpoint
                .ok_or_else(|| {
                    Error::StreamingError("Checkpoint data missing in response".to_string())
                })
                .and_then(|checkpoint| {
                    sui_types::full_checkpoint_content::Checkpoint::try_from(&checkpoint)
                        .map(Into::into)
                        .map_err(|e| {
                            Error::StreamingError(format!("Failed to parse checkpoint: {}", e))
                        })
                }),
            Err(e) => Err(Error::RpcClientError(e)),
        });

        let boxed_stream: Pin<Box<dyn Stream<Item = Result<CheckpointData>> + Send>> =
            Box::pin(converted_stream);

        Ok(GrpcPeekableStream {
            inner: boxed_stream.peekable(),
        })
    }
}

impl Stream for GrpcPeekableStream {
    type Item = Result<CheckpointData>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

#[async_trait]
impl PeekableStream for GrpcPeekableStream {
    async fn peek(&mut self) -> Option<Result<CheckpointData>> {
        match Pin::new(&mut self.inner).peek().await {
            Some(Ok(checkpoint)) => Some(Ok(checkpoint.clone())),
            Some(Err(_)) => {
                // Consume the error from the stream
                self.inner.next().await
            }
            None => None,
        }
    }
}

impl GRPCStreamingService {
    pub fn new(uri: Uri) -> Self {
        Self { uri }
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use crate::types::test_checkpoint_data_builder::TestCheckpointDataBuilder;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    enum MockCheckpointOrError {
        Checkpoint(u64),
        Error,
    }

    /// Mock stream that shares state across all instances
    pub struct MockStream {
        state: Arc<Mutex<VecDeque<MockCheckpointOrError>>>,
        peek_failures_remaining: Arc<Mutex<usize>>,
    }

    impl Stream for MockStream {
        type Item = Result<CheckpointData>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let mut state = self.state.lock().unwrap();

            match state.pop_front() {
                Some(MockCheckpointOrError::Checkpoint(sequence_number)) => {
                    let mut builder = TestCheckpointDataBuilder::new(sequence_number);
                    Poll::Ready(Some(Ok(builder.build_checkpoint())))
                }
                Some(MockCheckpointOrError::Error) => Poll::Ready(Some(Err(
                    Error::StreamingError("Failed to stream checkpoint".to_string()),
                ))),
                None => Poll::Ready(None),
            }
        }
    }

    #[async_trait]
    impl PeekableStream for MockStream {
        async fn peek(&mut self) -> Option<Result<CheckpointData>> {
            // Check if we should fail this peek
            let mut failures = self.peek_failures_remaining.lock().unwrap();
            if *failures > 0 {
                *failures -= 1;
                return Some(Err(Error::StreamingError("Mock peek failure".to_string())));
            }

            let state = self.state.lock().unwrap();

            // Look at the front without removing it
            match state.front() {
                Some(MockCheckpointOrError::Checkpoint(sequence_number)) => {
                    let mut builder = TestCheckpointDataBuilder::new(*sequence_number);
                    Some(Ok(builder.build_checkpoint()))
                }
                Some(MockCheckpointOrError::Error) => Some(Err(Error::StreamingError(
                    "Failed to stream checkpoint".to_string(),
                ))),
                None => None,
            }
        }
    }

    /// Mock streaming service for testing
    pub struct MockStreamingService {
        checkpoints_or_errors: Arc<Mutex<VecDeque<MockCheckpointOrError>>>,
        start_streaming_failures_remaining: usize,
        peek_failures_remaining: Arc<Mutex<usize>>,
    }

    impl MockStreamingService {
        pub fn new<I>(checkpoint_range: I) -> Self
        where
            I: IntoIterator<Item = u64>,
        {
            let checkpoints: VecDeque<_> = checkpoint_range
                .into_iter()
                .map(MockCheckpointOrError::Checkpoint)
                .collect();
            Self {
                checkpoints_or_errors: Arc::new(Mutex::new(checkpoints)),
                start_streaming_failures_remaining: 0,
                peek_failures_remaining: Arc::new(Mutex::new(0)),
            }
        }

        /// Make start_streaming fail for the next N calls
        pub fn fail_start_streaming_times(mut self, times: usize) -> Self {
            self.start_streaming_failures_remaining = times;
            self
        }

        /// Make peek fail for the next N calls
        pub fn fail_peek_times(self, times: usize) -> Self {
            *self.peek_failures_remaining.lock().unwrap() = times;
            self
        }

        /// Insert an error at the back of the queue.
        pub fn insert_error(&mut self) {
            self.checkpoints_or_errors
                .lock()
                .unwrap()
                .push_back(MockCheckpointOrError::Error);
        }

        /// Insert a checkpoint at the back of the queue.
        pub fn insert_checkpoint(&mut self, sequence_number: u64) {
            self.checkpoints_or_errors
                .lock()
                .unwrap()
                .push_back(MockCheckpointOrError::Checkpoint(sequence_number));
        }

        pub fn insert_checkpoint_range<I>(&mut self, checkpoint_range: I)
        where
            I: IntoIterator<Item = u64>,
        {
            let mut checkpoints = self.checkpoints_or_errors.lock().unwrap();
            for sequence_number in checkpoint_range {
                checkpoints.push_back(MockCheckpointOrError::Checkpoint(sequence_number));
            }
        }
    }

    #[async_trait]
    impl StreamingService for MockStreamingService {
        type Stream = MockStream;

        async fn connect(&mut self) -> Result<Self::Stream> {
            // Simulate start_streaming failures
            if self.start_streaming_failures_remaining > 0 {
                self.start_streaming_failures_remaining -= 1;
                return Err(Error::StreamingError(
                    "Mock start_streaming failure".to_string(),
                ));
            }

            // Share the checkpoints queue and peek failures counter with the new stream
            Ok(MockStream {
                state: Arc::clone(&self.checkpoints_or_errors),
                peek_failures_remaining: Arc::clone(&self.peek_failures_remaining),
            })
        }
    }
}
