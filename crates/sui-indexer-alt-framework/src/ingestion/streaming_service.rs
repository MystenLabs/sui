// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::StreamExt;
use sui_rpc::{
    field::{FieldMask, FieldMaskUtil},
    proto::sui::rpc::v2beta2::{
        subscription_service_client::SubscriptionServiceClient, SubscribeCheckpointsRequest,
        SubscribeCheckpointsResponse,
    },
};
use sui_rpc_api::client::checkpoint_data_try_from_proto;
use tonic::{Status, Streaming};

use crate::ingestion::error::{Error, Result};
use crate::types::full_checkpoint_content::CheckpointData;

#[async_trait]
pub trait StreamingService {
    async fn start_streaming(&mut self) -> Result<()>;
    async fn next_checkpoint(&mut self) -> Result<CheckpointData>;
}

pub struct GRPCStreamingService {
    client: Option<SubscriptionServiceClient<tonic::transport::Channel>>,
    stream: Option<Streaming<SubscribeCheckpointsResponse>>,
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
        request.read_mask = Some(FieldMask::from_paths([
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
                checkpoint_data_try_from_proto(&checkpoint).map_err(|e| {
                    Error::StreamingError(format!("Failed to parse checkpoint: {}", e))
                })
            }
            Some(Err(e)) => Err(Error::RpcClientError(e)),
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
            }
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
    }
}
