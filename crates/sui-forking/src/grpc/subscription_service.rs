// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prost_types::FieldMask;
use sui_rpc::field::{FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetObjectsRequest, BatchGetObjectsResponse, BatchGetTransactionsRequest,
    BatchGetTransactionsResponse, ExecutedTransaction, GetCheckpointRequest, GetCheckpointResponse,
    GetEpochRequest, GetEpochResponse, GetObjectRequest, GetObjectResponse, GetObjectResult,
    GetServiceInfoRequest, GetServiceInfoResponse, GetTransactionRequest, GetTransactionResponse,
    GetTransactionResult, Object, Transaction, TransactionEffects, TransactionEvents,
    UserSignature, ledger_service_server::LedgerService,
};
use sui_rpc_api::grpc::v2::ledger_service::validate_get_object_requests;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc_api::{
    CheckpointNotFoundError, ErrorReason, ObjectNotFoundError, RpcError, TransactionNotFoundError,
};
use sui_sdk_types::Digest;
use sui_types::base_types::ObjectID;
use sui_types::digests::{ChainIdentifier, CheckpointDigest};
use tokio::sync::RwLock;

use crate::store::ForkingStore;
use fastcrypto::encoding::{Base58, Encoding};
use simulacrum::EpochState;
use std::pin::Pin;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::subscription_service_server::SubscriptionService;
use sui_rpc::proto::sui::rpc::v2::{
    Checkpoint, Epoch, ProtocolConfig, SubscribeCheckpointsRequest, SubscribeCheckpointsResponse,
};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use tracing::info;

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
