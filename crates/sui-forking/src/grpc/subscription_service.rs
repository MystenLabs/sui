// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;

use sui_rpc::proto::sui::rpc::v2::subscription_service_server::SubscriptionService;
use sui_rpc::proto::sui::rpc::v2::{SubscribeCheckpointsRequest, SubscribeCheckpointsResponse};

/// Minimal subscription service placeholder for the runnable forking skeleton.
pub struct ForkingSubscriptionService {
    context: crate::context::Context,
}

impl ForkingSubscriptionService {
    pub fn new(context: crate::context::Context) -> Self {
        Self { context }
    }
}

#[tonic::async_trait]
impl SubscriptionService for ForkingSubscriptionService {
    type SubscribeCheckpointsStream = Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<SubscribeCheckpointsResponse, tonic::Status>>
                + Send,
        >,
    >;

    async fn subscribe_checkpoints(
        &self,
        _request: tonic::Request<SubscribeCheckpointsRequest>,
    ) -> Result<tonic::Response<Self::SubscribeCheckpointsStream>, tonic::Status> {
        let _ = &self.context;
        todo!("subscribe_checkpoints is not implemented in the runnable skeleton")
    }
}
