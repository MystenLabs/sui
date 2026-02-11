// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::context::Context;
use crate::grpc::error::RpcError;

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService;
use sui_types::base_types::SuiAddress;

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
        println!("available_range: request={:?}", request);
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = available_range(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn batch_get_balances(
        &self,
        request: tonic::Request<grpc::BatchGetBalancesRequest>,
    ) -> Result<tonic::Response<grpc::BatchGetBalancesResponse>, tonic::Status> {
        println!("batch get balances {:?}", request);
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = batch_get_balances(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn get_balance(
        &self,
        request: tonic::Request<grpc::GetBalanceRequest>,
    ) -> Result<tonic::Response<grpc::Balance>, tonic::Status> {
        println!("get balance: request={:?}", request);
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = get_balance(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_balances(
        &self,
        request: tonic::Request<grpc::ListBalancesRequest>,
    ) -> Result<tonic::Response<grpc::ListBalancesResponse>, tonic::Status> {
        println!("list balances: request={:?}", request);
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = list_balances(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_objects_by_type(
        &self,
        request: tonic::Request<grpc::ListObjectsByTypeRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        println!("list_objects_by_type: request={:?}", request);
        todo!()
        // let checkpoint = self.checkpoint(&request)?;
        // let response = list_objects_by_type(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<grpc::ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        let sim = self.context.simulacrum.read().await;
        let data_store = sim.store_static();

        let owner = request.into_inner().owner;
        let Some(owner) = owner else {
            return Err(tonic::Status::invalid_argument("owner is required"));
        };

        let address = owner.address();
        let sui_address = SuiAddress::from_str(address).unwrap();
        let _objects = data_store.owned_objects(sui_address);
        todo!()
    }

    async fn service_config(
        &self,
        request: tonic::Request<grpc::ServiceConfigRequest>,
    ) -> Result<tonic::Response<grpc::ServiceConfigResponse>, tonic::Status> {
        println!("Service config");
        service_config(request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

// TODO: this is just a hack
pub(super) fn service_config(
    grpc::ServiceConfigRequest {}: grpc::ServiceConfigRequest,
) -> Result<grpc::ServiceConfigResponse, RpcError> {
    Ok(grpc::ServiceConfigResponse {
        default_page_size: Some(10),
        max_batch_size: Some(10),
        max_page_size: Some(20),
    })
}
