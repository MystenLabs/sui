// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::KvRpcServer;
use crate::proto::sui::rpc::kv::v2alpha::ListEventsRequest;
use crate::proto::sui::rpc::kv::v2alpha::ListEventsResponse;
use crate::proto::sui::rpc::kv::v2alpha::ListTransactionsRequest;
use crate::proto::sui::rpc::kv::v2alpha::ListTransactionsResponse;
use crate::proto::sui::rpc::kv::v2alpha::list_service_server::ListService;

mod filter;
mod list_events;
mod list_transactions;

#[tonic::async_trait]
impl ListService for KvRpcServer {
    async fn list_transactions(
        &self,
        request: tonic::Request<ListTransactionsRequest>,
    ) -> Result<tonic::Response<ListTransactionsResponse>, tonic::Status> {
        list_transactions::list_transactions(
            self.client.clone(),
            request.into_inner(),
            &self.package_resolver,
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn list_events(
        &self,
        request: tonic::Request<ListEventsRequest>,
    ) -> Result<tonic::Response<ListEventsResponse>, tonic::Status> {
        list_events::list_events(
            self.client.clone(),
            request.into_inner(),
            &self.package_resolver,
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }
}
