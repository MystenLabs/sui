// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::RpcService;
use crate::grpc::v2::ledger_service::serve_list_stream;
use sui_rpc::proto::sui::rpc::v2::BatchGetBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetBalancesResponse;
use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;
use sui_rpc::proto::sui::rpc::v2::GetBalanceResponse;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetCoinInfoResponse;
use sui_rpc::proto::sui::rpc::v2::ListBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::ListBalancesResponse;
use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsRequest;
use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsResponse;
use sui_rpc::proto::sui::rpc::v2::ListObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::ListObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::state_service_server::StateService;
use tonic::codegen::BoxStream;

mod checkpoint;
mod cursor;
mod filter;
mod get_balance;
mod get_coin_info;
mod list_balances;
mod list_dynamic_fields;
mod list_objects;
mod list_owned_objects;

const DEFAULT_LIST_OBJECTS_TIMEOUT: Duration = Duration::from_secs(60);
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

    async fn list_objects(
        &self,
        request: tonic::Request<ListObjectsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListObjectsResponse>>, tonic::Status> {
        let timeout = self
            .config
            .grpc_timeout()
            .unwrap_or(DEFAULT_LIST_OBJECTS_TIMEOUT);
        serve_list_stream(
            "list_objects",
            timeout,
            list_objects::list_objects(self.clone(), request.into_inner()),
        )
        .await
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

    async fn batch_get_balances(
        &self,
        request: tonic::Request<BatchGetBalancesRequest>,
    ) -> Result<tonic::Response<BatchGetBalancesResponse>, tonic::Status> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            get_balance::batch_get_balances(&service, request.into_inner())
        })
        .await
        .map_err(|e| tonic::Status::internal(format!("batch_get_balances task failed: {e}")))?
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
    #[tokio::test]
    async fn every_method_is_not_served_without_indexes() {
        let service = StubStore::new().into_service();
        let no_rpc_indexes = NotServedHereCause::NoRpcIndexes;

        assert_not_served_here(
            get_balance::get_balance(&service, GetBalanceRequest::default()).unwrap_err(),
            no_rpc_indexes,
        );
        assert_not_served_here(
            get_balance::batch_get_balances(&service, BatchGetBalancesRequest::default())
                .unwrap_err(),
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
        assert_not_served_here(
            list_objects::list_objects(service.clone(), ListObjectsRequest::default())
                .await
                .err()
                .unwrap(),
            no_rpc_indexes,
        );
    }
}
