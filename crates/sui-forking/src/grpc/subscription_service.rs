// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;

// use prost_types::FieldMask;

// use sui_rpc::field::{FieldMaskTree, FieldMaskUtil};
// use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::subscription_service_server::SubscriptionService;
// use sui_rpc::proto::sui::rpc::v2::{
//     BatchGetObjectsRequest, BatchGetObjectsResponse, BatchGetTransactionsRequest,
//     BatchGetTransactionsResponse, ExecutedTransaction, GetCheckpointRequest, GetCheckpointResponse,
//     GetEpochRequest, GetEpochResponse, GetObjectRequest, GetObjectResponse, GetObjectResult,
//     GetServiceInfoRequest, GetServiceInfoResponse, GetTransactionRequest, GetTransactionResponse,
//     GetTransactionResult, Object, Transaction, TransactionEffects, TransactionEvents,
//     UserSignature, ledger_service_server::LedgerService,
// };
use sui_rpc::proto::sui::rpc::v2::{SubscribeCheckpointsRequest, SubscribeCheckpointsResponse};
// use sui_rpc_api::grpc::v2::ledger_service::validate_get_object_requests;

const READ_MASK_DEFAULT: &str = "digest";

/// A SubscriptionService implementation backed by the ForkingStore/Simulacrum.
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
        println!("SubscribeCheckpoints: request={:?}", request);
        let mut response = SubscribeCheckpointsResponse::default();
        response.cursor = None;
        response.checkpoint = None;

        Ok(tonic::Response::new(Box::pin(tokio_stream::iter(vec![
            Ok(response),
        ]))))

        // let subscription_service_handle = self
        //     .subscription_service_handle
        //     .as_ref()
        //     .ok_or_else(|| tonic::Status::unimplemented("subscription service not enabled"))?;
        // let read_mask = request.into_inner().read_mask.unwrap_or_default();
        // let read_mask = FieldMaskTree::from(read_mask);
        //
        // let Some(mut receiver) = subscription_service_handle.register_subscription().await else {
        //     return Err(tonic::Status::unavailable(
        //         "too many existing subscriptions",
        //     ));
        // };
    }
}
