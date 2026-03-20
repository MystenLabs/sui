// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context::Context;
use sui_rpc::proto::sui::rpc::v2::state_service_server::StateService;
use sui_rpc_api::proto::sui::rpc::v2 as grpc;

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
        _request: tonic::Request<grpc::GetCoinInfoRequest>,
    ) -> Result<tonic::Response<grpc::GetCoinInfoResponse>, tonic::Status> {
        let _ = &self.context;
        todo!("get_coin_info is not implemented in the runnable skeleton")
    }

    async fn list_dynamic_fields(
        &self,
        _request: tonic::Request<grpc::ListDynamicFieldsRequest>,
    ) -> Result<tonic::Response<grpc::ListDynamicFieldsResponse>, tonic::Status> {
        let _ = &self.context;
        todo!("list_dynamic_fields is not implemented in the runnable skeleton")
    }

    async fn get_balance(
        &self,
        _request: tonic::Request<grpc::GetBalanceRequest>,
    ) -> Result<tonic::Response<grpc::GetBalanceResponse>, tonic::Status> {
        let _ = &self.context;
        todo!("get_balance is not implemented in the runnable skeleton")
    }

    async fn list_balances(
        &self,
        _request: tonic::Request<grpc::ListBalancesRequest>,
    ) -> Result<tonic::Response<grpc::ListBalancesResponse>, tonic::Status> {
        let _ = &self.context;
        todo!("list_balances is not implemented in the runnable skeleton")
    }

    async fn list_owned_objects(
        &self,
        _request: tonic::Request<grpc::ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<grpc::ListOwnedObjectsResponse>, tonic::Status> {
        let _ = &self.context;
        todo!("list_owned_objects is not implemented in the runnable skeleton")
    }
}
