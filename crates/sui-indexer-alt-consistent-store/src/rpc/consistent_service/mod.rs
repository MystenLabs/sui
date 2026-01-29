// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService;

use crate::rpc::consistent_service::available_range::available_range;
use crate::rpc::consistent_service::balances::batch_get_balances;
use crate::rpc::consistent_service::balances::get_balance;
use crate::rpc::consistent_service::balances::list_balances;
use crate::rpc::consistent_service::list_objects_by_type::list_objects_by_type;
use crate::rpc::consistent_service::list_owned_objects::list_owned_objects;
use crate::rpc::consistent_service::service_config::service_config;
use crate::rpc::state::State;
use crate::rpc::state::checkpointed_response;

mod available_range;
mod balances;
mod list_objects_by_type;
mod list_owned_objects;
mod service_config;

#[async_trait::async_trait]
impl ConsistentService for State {
    async fn available_range(
        &self,
        request: tonic::Request<grpc::AvailableRangeRequest>,
    ) -> Result<tonic::Response<grpc::AvailableRangeResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = available_range(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn batch_get_balances(
        &self,
        request: tonic::Request<grpc::BatchGetBalancesRequest>,
    ) -> Result<tonic::Response<grpc::BatchGetBalancesResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = batch_get_balances(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn get_balance(
        &self,
        request: tonic::Request<grpc::GetBalanceRequest>,
    ) -> Result<tonic::Response<grpc::Balance>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = get_balance(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_balances(
        &self,
        request: tonic::Request<grpc::ListBalancesRequest>,
    ) -> Result<tonic::Response<grpc::ListBalancesResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = list_balances(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_objects_by_type(
        &self,
        request: tonic::Request<grpc::ListObjectsByTypeRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = list_objects_by_type(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<grpc::ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = list_owned_objects(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn service_config(
        &self,
        request: tonic::Request<grpc::ServiceConfigRequest>,
    ) -> Result<tonic::Response<grpc::ServiceConfigResponse>, tonic::Status> {
        service_config(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}
