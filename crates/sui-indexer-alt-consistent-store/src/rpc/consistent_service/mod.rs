// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    AvailableRangeRequest, AvailableRangeResponse, Balance, BatchGetBalancesRequest,
    BatchGetBalancesResponse, GetBalanceRequest, ListBalancesRequest, ListBalancesResponse,
    ListObjectsByTypeRequest, ListObjectsResponse, ListOwnedObjectsRequest, ServiceConfigRequest,
    ServiceConfigResponse,
};

use super::state::{checkpointed_response, State};

use self::available_range::available_range;
use self::balances::{batch_get_balances, get_balance, list_balances};
use self::list_objects_by_type::list_objects_by_type;
use self::list_owned_objects::list_owned_objects;
use self::service_config::service_config;

mod available_range;
mod balances;
mod list_objects_by_type;
mod list_owned_objects;
mod service_config;

#[async_trait::async_trait]
impl ConsistentService for State {
    async fn available_range(
        &self,
        request: tonic::Request<AvailableRangeRequest>,
    ) -> Result<tonic::Response<AvailableRangeResponse>, tonic::Status> {
        available_range(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn batch_get_balances(
        &self,
        request: tonic::Request<BatchGetBalancesRequest>,
    ) -> Result<tonic::Response<BatchGetBalancesResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = batch_get_balances(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn get_balance(
        &self,
        request: tonic::Request<GetBalanceRequest>,
    ) -> Result<tonic::Response<Balance>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = get_balance(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_balances(
        &self,
        request: tonic::Request<ListBalancesRequest>,
    ) -> Result<tonic::Response<ListBalancesResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = list_balances(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_objects_by_type(
        &self,
        request: tonic::Request<ListObjectsByTypeRequest>,
    ) -> Result<tonic::Response<ListObjectsResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = list_objects_by_type(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<ListObjectsResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = list_owned_objects(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn service_config(
        &self,
        request: tonic::Request<ServiceConfigRequest>,
    ) -> Result<tonic::Response<ServiceConfigResponse>, tonic::Status> {
        service_config(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}
