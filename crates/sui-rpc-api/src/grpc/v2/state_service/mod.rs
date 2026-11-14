// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::RpcService;
use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;
use sui_rpc::proto::sui::rpc::v2::GetBalanceResponse;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoResponse;
use sui_rpc::proto::sui::rpc::v2::ListBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::ListBalancesResponse;
use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsRequest;
use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsResponse;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::state_service_server::StateService;

mod get_balance;
mod get_coin_info;
mod list_balances;
mod list_dynamic_fields;
mod list_owned_objects;

#[tonic::async_trait]
impl StateService for RpcService {
    async fn list_dynamic_fields(
        &self,
        request: tonic::Request<ListDynamicFieldsRequest>,
    ) -> Result<tonic::Response<ListDynamicFieldsResponse>, tonic::Status> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            list_dynamic_fields::list_dynamic_fields(&service, request.into_inner())
        })
        .await
        .map_err(|e| tonic::Status::internal(format!("list_dynamic_fields task failed: {e}")))?
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<ListOwnedObjectsResponse>, tonic::Status> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            list_owned_objects::list_owned_objects(&service, request.into_inner())
        })
        .await
        .map_err(|e| tonic::Status::internal(format!("list_owned_objects task failed: {e}")))?
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
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            list_balances::list_balances(&service, request.into_inner())
        })
        .await
        .map_err(|e| tonic::Status::internal(format!("list_balances task failed: {e}")))?
        .map(tonic::Response::new)
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::{NotServedHereCause, assert_not_served_here};
    use crate::stub_store::StubStore;

    /// Every `StateService` method reads the rpc indexes, so a node without them refuses each
    /// call as not served here, before it looks at the request.
    #[test]
    fn every_method_is_not_served_without_indexes() {
        let service = StubStore::new().into_service();
        let no_rpc_indexes = NotServedHereCause::NoRpcIndexes;

        assert_not_served_here(
            get_balance::get_balance(&service, GetBalanceRequest::default()).unwrap_err(),
            no_rpc_indexes,
        );
        assert_not_served_here(
            get_coin_info::get_coin_info(&service, GetCoinInfoRequest::default()).unwrap_err(),
            no_rpc_indexes,
        );
        assert_not_served_here(
            list_balances::list_balances(&service, ListBalancesRequest::default()).unwrap_err(),
            no_rpc_indexes,
        );
        assert_not_served_here(
            list_dynamic_fields::list_dynamic_fields(&service, ListDynamicFieldsRequest::default())
                .unwrap_err(),
            no_rpc_indexes,
        );
        assert_not_served_here(
            list_owned_objects::list_owned_objects(&service, ListOwnedObjectsRequest::default())
                .unwrap_err(),
            no_rpc_indexes,
        );
    }
}
