// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::{stream::Peekable, StreamExt};
use sui_rpc::{
    field::{FieldMask, FieldMaskUtil},
    proto::sui::rpc::v2::{
        subscription_service_client::SubscriptionServiceClient, SubscribeCheckpointsRequest,
        SubscribeCheckpointsResponse,
    },
};
use tonic::{Status, Streaming};

use crate::ingestion::error::{Error, Result};
use crate::types::full_checkpoint_content::CheckpointData;

#[async_trait]
pub trait StreamingService {
    async fn start_streaming(&mut self) -> Result<()>;
    async fn next_checkpoint(&mut self) -> Result<CheckpointData>;
    async fn peek_next_checkpoint(&mut self) -> Result<u64>;
}

pub struct GRPCStreamingService {
    client: Option<SubscriptionServiceClient<tonic::transport::Channel>>,
    stream: Option<Peekable<Streaming<SubscribeCheckpointsResponse>>>,
    endpoint: String,
}

impl GRPCStreamingService {
    pub fn new(endpoint: String) -> Self {
        Self {
            client: None,
            stream: None,
            endpoint,
        }
    }
}

#[async_trait]
impl StreamingService for GRPCStreamingService {
    async fn start_streaming(&mut self) -> Result<()> {
        let mut client = SubscriptionServiceClient::connect(self.endpoint.clone())
            .await
            .map_err(|err| Error::RpcClientError(Status::from_error(err.into())))?;

        // Request all the fields we need to construct CheckpointData
        let mut request = SubscribeCheckpointsRequest::default();
        // TODO: we probably don't need all of these fields, trim down later.
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
            .into_inner()
            .peekable();

        self.client = Some(client);
        self.stream = Some(stream);
        Ok(())
    }

    async fn next_checkpoint(&mut self) -> Result<CheckpointData> {
        let stream = self.stream.as_mut().ok_or_else(|| {
            Error::StreamingError("Stream not initialized. Call start_streaming first.".to_string())
        })?;

        match stream.next().await {
            Some(Ok(response)) => {
                let checkpoint = response.checkpoint.ok_or_else(|| {
                    Error::StreamingError("Checkpoint data missing in response".to_string())
                })?;

                // Use the conversion function from sui-rpc-api
                sui_types::full_checkpoint_content::Checkpoint::try_from(&checkpoint)
                    .map(Into::into)
                    .map_err(|e| {
                        Error::StreamingError(format!("Failed to parse checkpoint: {}", e))
                    })
            }
            Some(Err(e)) => Err(Error::RpcClientError(e)),
            None => Err(Error::StreamingError(
                "Stream ended unexpectedly".to_string(),
            )),
        }
    }

    async fn peek_next_checkpoint(&mut self) -> Result<u64> {
        use std::pin::Pin;

        let stream = self.stream.as_mut().ok_or_else(|| {
            Error::StreamingError("Stream not initialized. Call start_streaming first.".to_string())
        })?;

        match Pin::new(stream).peek().await {
            Some(Ok(response)) => {
                let checkpoint = response.checkpoint.as_ref().ok_or_else(|| {
                    Error::StreamingError("Checkpoint data missing in response".to_string())
                })?;

                let sequence_number = checkpoint.sequence_number.ok_or_else(|| {
                    Error::StreamingError("Checkpoint sequence number missing".to_string())
                })?;

                Ok(sequence_number)
            }
            Some(Err(e)) => Err(Error::StreamingError(format!("Error in stream: {}", e))),
            None => Err(Error::StreamingError(
                "Stream ended unexpectedly".to_string(),
            )),
        }
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use crate::types::test_checkpoint_data_builder::TestCheckpointDataBuilder;
    use std::collections::VecDeque;

    enum MockCheckpointOrError {
        Checkpoint(u64),
        Error,
    }

    /// Mock streaming service for testing
    pub struct MockStreamingService {
        checkpoints_or_errors: VecDeque<MockCheckpointOrError>,
        start_streaming_failures_remaining: usize,
        peek_should_fail_once: bool,
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
                checkpoints_or_errors: checkpoints,
                start_streaming_failures_remaining: 0,
                peek_should_fail_once: false,
            }
        }

        /// Make start_streaming fail for the next N calls
        pub fn fail_start_streaming_times(mut self, times: usize) -> Self {
            self.start_streaming_failures_remaining = times;
            self
        }

        /// Make peek_next_checkpoint fail once
        pub fn fail_peek_once(mut self) -> Self {
            self.peek_should_fail_once = true;
            self
        }

        /// Insert an error at the back of the queue.
        pub fn insert_error(&mut self) {
            self.checkpoints_or_errors
                .push_back(MockCheckpointOrError::Error);
        }

        /// Insert a checkpoint at the back of the queue.
        pub fn insert_checkpoint(&mut self, sequence_number: u64) {
            self.checkpoints_or_errors
                .push_back(MockCheckpointOrError::Checkpoint(sequence_number));
        }

        pub fn insert_checkpoint_range<I>(&mut self, checkpoint_range: I)
        where
            I: IntoIterator<Item = u64>,
        {
            for sequence_number in checkpoint_range {
                self.checkpoints_or_errors
                    .push_back(MockCheckpointOrError::Checkpoint(sequence_number));
            }
        }
    }

    #[async_trait]
    impl StreamingService for MockStreamingService {
        async fn start_streaming(&mut self) -> Result<()> {
            if self.start_streaming_failures_remaining > 0 {
                self.start_streaming_failures_remaining -= 1;
                return Err(Error::StreamingError(
                    "Mock start_streaming failure".to_string(),
                ));
            }
            Ok(())
        }

        async fn next_checkpoint(&mut self) -> Result<CheckpointData> {
            match self.checkpoints_or_errors.pop_front() {
                Some(MockCheckpointOrError::Checkpoint(sequence_number)) => {
                    // Create a builder with the desired checkpoint number and build it
                    let mut builder = TestCheckpointDataBuilder::new(sequence_number);
                    Ok(builder.build_checkpoint())
                }
                Some(MockCheckpointOrError::Error) => Err(Error::StreamingError(
                    "Failed to stream checkpoint".to_string(),
                )),
                None => Err(Error::StreamingError("No more checkpoints".to_string())),
            }
        }

        async fn peek_next_checkpoint(&mut self) -> Result<u64> {
            if self.peek_should_fail_once {
                self.peek_should_fail_once = false;
                return Err(Error::StreamingError(
                    "Mock peek_next_checkpoint failure".to_string(),
                ));
            }

            match self.checkpoints_or_errors.front() {
                Some(MockCheckpointOrError::Checkpoint(sequence_number)) => Ok(*sequence_number),
                Some(MockCheckpointOrError::Error) => Err(Error::StreamingError(
                    "Failed to stream checkpoint".to_string(),
                )),
                None => Err(Error::StreamingError("No more checkpoints".to_string())),
            }
        }
    }
}
