// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;

use crate::subscription::SubscriptionServiceHandle;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2beta2::subscription_service_server::SubscriptionService;
use sui_rpc::proto::sui::rpc::v2beta2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2beta2::SubscribeCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2beta2::SubscribeCheckpointsResponse;

#[tonic::async_trait]
impl SubscriptionService for SubscriptionServiceHandle {
    /// Server streaming response type for the SubscribeCheckpoints method.
    type SubscribeCheckpointsStream = Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<SubscribeCheckpointsResponse, tonic::Status>>
                + Send,
        >,
    >;

    async fn subscribe_checkpoints(
        &self,
        request: tonic::Request<SubscribeCheckpointsRequest>,
    ) -> Result<tonic::Response<Self::SubscribeCheckpointsStream>, tonic::Status> {
        let read_mask = request.into_inner().read_mask.unwrap_or_default();
        let read_mask = FieldMaskTree::from(read_mask);

        let Some(mut receiver) = self.register_subscription().await else {
            return Err(tonic::Status::unavailable(
                "too many existing subscriptions",
            ));
        };

        let response = Box::pin(async_stream::stream! {
            while let Some(checkpoint) = receiver.recv().await {
                let cursor = checkpoint.checkpoint_summary.sequence_number;

                let checkpoint = Checkpoint::merge_from(
                    checkpoint.as_ref().to_owned(), // TODO optimize so checkpoint isn't cloned
                    &read_mask
                );

                let mut response = SubscribeCheckpointsResponse::default();
                response.cursor = Some(cursor);
                response.checkpoint = Some(checkpoint);

                yield Ok(response);
            }
        });

        Ok(tonic::Response::new(response))
    }
}
