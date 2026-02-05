// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context::Context;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService;

pub(crate) struct ForkingConsistentStore {
    context: Context,
}

impl ForkingConsistentStore {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

#[async_trait::async_trait]
impl ConsistentService for ForkingConsistentStore {
    async fn available_range(
        &self,
        request: tonic::Request<grpc::AvailableRangeRequest>,
    ) -> Result<tonic::Response<grpc::AvailableRangeResponse>, tonic::Status> {
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = available_range(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn batch_get_balances(
        &self,
        request: tonic::Request<grpc::BatchGetBalancesRequest>,
    ) -> Result<tonic::Response<grpc::BatchGetBalancesResponse>, tonic::Status> {
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = batch_get_balances(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn get_balance(
        &self,
        request: tonic::Request<grpc::GetBalanceRequest>,
    ) -> Result<tonic::Response<grpc::Balance>, tonic::Status> {
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = get_balance(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_balances(
        &self,
        request: tonic::Request<grpc::ListBalancesRequest>,
    ) -> Result<tonic::Response<grpc::ListBalancesResponse>, tonic::Status> {
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = list_balances(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_objects_by_type(
        &self,
        request: tonic::Request<grpc::ListObjectsByTypeRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = list_objects_by_type(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<grpc::ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = list_owned_objects(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn service_config(
        &self,
        request: tonic::Request<grpc::ServiceConfigRequest>,
    ) -> Result<tonic::Response<grpc::ServiceConfigResponse>, tonic::Status> {
        todo!()
        // service_config(self, request.into_inner())
        //     .map(tonic::Response::new)
        //     .map_err(Into::into)
    }
}
