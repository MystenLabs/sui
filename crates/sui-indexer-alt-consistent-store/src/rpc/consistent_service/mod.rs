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
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| Ok(available_range(self, cp, request.into_inner())?)),
        )
    }

    async fn batch_get_balances(
        &self,
        request: tonic::Request<grpc::BatchGetBalancesRequest>,
    ) -> Result<tonic::Response<grpc::BatchGetBalancesResponse>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| Ok(batch_get_balances(self, cp, request.into_inner())?)),
        )
    }

    async fn get_balance(
        &self,
        request: tonic::Request<grpc::GetBalanceRequest>,
    ) -> Result<tonic::Response<grpc::Balance>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| Ok(get_balance(self, cp, request.into_inner())?)),
        )
    }

    async fn list_balances(
        &self,
        request: tonic::Request<grpc::ListBalancesRequest>,
    ) -> Result<tonic::Response<grpc::ListBalancesResponse>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| Ok(list_balances(self, cp, request.into_inner())?)),
        )
    }

    async fn list_objects_by_type(
        &self,
        request: tonic::Request<grpc::ListObjectsByTypeRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| Ok(list_objects_by_type(self, cp, request.into_inner())?)),
        )
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<grpc::ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| Ok(list_owned_objects(self, cp, request.into_inner())?)),
        )
    }

    async fn service_config(
        &self,
        request: tonic::Request<grpc::ServiceConfigRequest>,
    ) -> Result<tonic::Response<grpc::ServiceConfigResponse>, tonic::Status> {
        self.checkpointed_response(
            service_config(self, request.into_inner()).map_err(tonic::Status::from),
        )
    }
}
