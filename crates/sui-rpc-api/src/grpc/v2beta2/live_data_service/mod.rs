// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::rpc::v2beta2::live_data_service_server::LiveDataService;
use crate::proto::rpc::v2beta2::GetBalanceRequest;
use crate::proto::rpc::v2beta2::GetBalanceResponse;
use crate::proto::rpc::v2beta2::GetCoinInfoRequest;
use crate::proto::rpc::v2beta2::GetCoinInfoResponse;
use crate::proto::rpc::v2beta2::ListBalancesRequest;
use crate::proto::rpc::v2beta2::ListBalancesResponse;
use crate::proto::rpc::v2beta2::ListDynamicFieldsRequest;
use crate::proto::rpc::v2beta2::ListDynamicFieldsResponse;
use crate::proto::rpc::v2beta2::ListOwnedObjectsRequest;
use crate::proto::rpc::v2beta2::ListOwnedObjectsResponse;
use crate::proto::rpc::v2beta2::SimulateTransactionRequest;
use crate::proto::rpc::v2beta2::SimulateTransactionResponse;
use crate::RpcService;

mod get_balance;
mod get_coin_info;
mod list_balances;
mod list_dynamic_fields;
mod list_owned_objects;
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
