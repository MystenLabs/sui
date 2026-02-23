// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context::Context;

use sui_rpc::proto::sui::rpc::v2::state_service_server::StateService;
use sui_rpc_api::proto::sui::rpc::v2 as grpc;

mod list_owned_objects;

pub(crate) struct ForkingStateService {
    context: Context,
}

impl ForkingStateService {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

#[tonic::async_trait]
impl StateService for ForkingStateService {
    async fn get_coin_info(
        &self,
        request: tonic::Request<grpc::GetCoinInfoRequest>,
    ) -> Result<tonic::Response<grpc::GetCoinInfoResponse>, tonic::Status> {
        let _ = (&self.context, request);
        Err(tonic::Status::unimplemented(
            "get_coin_info is not implemented in sui-forking yet",
        ))
        // let checkpoint = self.checkpoint(&request)?;
        // let response = get_coin(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_dynamic_fields(
        &self,
        request: tonic::Request<grpc::ListDynamicFieldsRequest>,
    ) -> Result<tonic::Response<grpc::ListDynamicFieldsResponse>, tonic::Status> {
        let _ = (&self.context, request);
        Err(tonic::Status::unimplemented(
            "list_dynamic_fields is not implemented in sui-forking yet",
        ))
        // let checkpoint = self.checkpoint(&request)?;
        // let response = list_dynamic_fields(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn get_balance(
        &self,
        request: tonic::Request<grpc::GetBalanceRequest>,
    ) -> Result<tonic::Response<grpc::GetBalanceResponse>, tonic::Status> {
        let _ = (&self.context, request);
        Err(tonic::Status::unimplemented(
            "get_balance is not implemented in sui-forking yet",
        ))
        // let checkpoint = self.checkpoint(&request)?;
        // let response = get_balance(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_balances(
        &self,
        request: tonic::Request<grpc::ListBalancesRequest>,
    ) -> Result<tonic::Response<grpc::ListBalancesResponse>, tonic::Status> {
        let _ = (&self.context, request);
        Err(tonic::Status::unimplemented(
            "list_balances is not implemented in sui-forking yet",
        ))
        // let checkpoint = self.checkpoint(&request)?;
        // let response = list_balances(self, checkpoint, request.into_inner())?;
        // Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<grpc::ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<grpc::ListOwnedObjectsResponse>, tonic::Status> {
        list_owned_objects::list_owned_objects(self, request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}
