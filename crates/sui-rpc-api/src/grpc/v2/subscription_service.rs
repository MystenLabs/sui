// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::RpcService;
use crate::grpc::v2alpha::subscription_service::subscribe_checkpoints_stable;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2::subscription_service_server::SubscriptionService;
use tonic::codegen::BoxStream;

#[tonic::async_trait]
impl SubscriptionService for RpcService {
    async fn subscribe_checkpoints(
        &self,
        request: tonic::Request<SubscribeCheckpointsRequest>,
    ) -> Result<tonic::Response<BoxStream<SubscribeCheckpointsResponse>>, tonic::Status> {
        subscribe_checkpoints_stable(self, request.into_inner()).await
    }
}
