// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;

use crate::field_mask::FieldMaskTree;
use crate::message::MessageMergeFrom;
use crate::proto::rpc::v2alpha::live_data_service_server::LiveDataService;
use crate::proto::rpc::v2alpha::move_package_service_server::MovePackageService;
use crate::proto::rpc::v2alpha::signature_verification_service_server::SignatureVerificationService;
use crate::proto::rpc::v2alpha::subscription_service_server::SubscriptionService;
use crate::proto::rpc::v2alpha::GetBalanceRequest;
use crate::proto::rpc::v2alpha::GetBalanceResponse;
use crate::proto::rpc::v2alpha::GetCoinInfoRequest;
use crate::proto::rpc::v2alpha::GetCoinInfoResponse;
use crate::proto::rpc::v2alpha::ListBalancesRequest;
use crate::proto::rpc::v2alpha::ListBalancesResponse;
use crate::proto::rpc::v2alpha::ListDynamicFieldsRequest;
use crate::proto::rpc::v2alpha::ListDynamicFieldsResponse;
use crate::proto::rpc::v2alpha::ListOwnedObjectsRequest;
use crate::proto::rpc::v2alpha::ListOwnedObjectsResponse;
use crate::proto::rpc::v2alpha::SimulateTransactionRequest;
use crate::proto::rpc::v2alpha::SimulateTransactionResponse;
use crate::proto::rpc::v2alpha::SubscribeCheckpointsRequest;
use crate::proto::rpc::v2alpha::SubscribeCheckpointsResponse;
use crate::proto::rpc::v2alpha::VerifySignatureRequest;
use crate::proto::rpc::v2alpha::VerifySignatureResponse;
use crate::proto::rpc::v2alpha::{
    GetDatatypeRequest, GetDatatypeResponse, GetFunctionRequest, GetFunctionResponse,
    GetModuleRequest, GetModuleResponse, GetPackageRequest, GetPackageResponse,
    ListPackageVersionsRequest, ListPackageVersionsResponse,
};
use crate::proto::rpc::v2beta::Checkpoint;
use crate::subscription::SubscriptionServiceHandle;
use crate::RpcService;

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
                let Some(cursor) = checkpoint.sequence_number else {
                    yield Err(tonic::Status::internal("unable to determine cursor"));
                    break;
                };

                let checkpoint = Checkpoint::merge_from(checkpoint.as_ref(), &read_mask);
                let response = SubscribeCheckpointsResponse {
                    cursor: Some(cursor),
                    checkpoint: Some(checkpoint),
                };

                yield Ok(response);
            }
        });

        Ok(tonic::Response::new(response))
    }
}

mod get_balance;
mod get_coin_info;
mod list_balances;
mod list_dynamic_fields;
mod list_owned_objects;
mod move_package;
mod simulate;

#[tonic::async_trait]
impl LiveDataService for RpcService {
    async fn list_dynamic_fields(
        &self,
        request: tonic::Request<ListDynamicFieldsRequest>,
    ) -> Result<tonic::Response<ListDynamicFieldsResponse>, tonic::Status> {
        list_dynamic_fields::list_dynamic_fields(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<ListOwnedObjectsResponse>, tonic::Status> {
        list_owned_objects::list_owned_objects(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_coin_info(
        &self,
        request: tonic::Request<GetCoinInfoRequest>,
    ) -> Result<tonic::Response<GetCoinInfoResponse>, tonic::Status> {
        get_coin_info::get_coin_info(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_balance(
        &self,
        request: tonic::Request<GetBalanceRequest>,
    ) -> Result<tonic::Response<GetBalanceResponse>, tonic::Status> {
        get_balance::get_balance(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_balances(
        &self,
        request: tonic::Request<ListBalancesRequest>,
    ) -> Result<tonic::Response<ListBalancesResponse>, tonic::Status> {
        list_balances::list_balances(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn simulate_transaction(
        &self,
        request: tonic::Request<SimulateTransactionRequest>,
    ) -> Result<tonic::Response<SimulateTransactionResponse>, tonic::Status> {
        simulate::simulate_transaction(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

mod verify_signature;

#[tonic::async_trait]
impl SignatureVerificationService for RpcService {
    async fn verify_signature(
        &self,
        request: tonic::Request<VerifySignatureRequest>,
    ) -> Result<tonic::Response<VerifySignatureResponse>, tonic::Status> {
        verify_signature::verify_signature(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

#[tonic::async_trait]
impl MovePackageService for RpcService {
    async fn get_package(
        &self,
        request: tonic::Request<GetPackageRequest>,
    ) -> Result<tonic::Response<GetPackageResponse>, tonic::Status> {
        move_package::get_package::get_package(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_module(
        &self,
        request: tonic::Request<GetModuleRequest>,
    ) -> Result<tonic::Response<GetModuleResponse>, tonic::Status> {
        move_package::get_module::get_module(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_datatype(
        &self,
        request: tonic::Request<GetDatatypeRequest>,
    ) -> Result<tonic::Response<GetDatatypeResponse>, tonic::Status> {
        move_package::get_datatype::get_datatype(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_function(
        &self,
        request: tonic::Request<GetFunctionRequest>,
    ) -> Result<tonic::Response<GetFunctionResponse>, tonic::Status> {
        move_package::get_function::get_function(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_package_versions(
        &self,
        request: tonic::Request<ListPackageVersionsRequest>,
    ) -> Result<tonic::Response<ListPackageVersionsResponse>, tonic::Status> {
        move_package::list_package_versions::list_package_versions(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}
