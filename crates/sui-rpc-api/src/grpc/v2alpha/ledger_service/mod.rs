// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_server::LedgerService;
use tonic::codegen::BoxStream;

use crate::RpcService;

mod bitmap_scan;
mod chunked_scan;
mod ledger_read;
mod list_checkpoints;
mod list_events;
mod list_transactions;
mod query_end;
mod stream;

use stream::serve_list_stream;

#[tonic::async_trait]
impl LedgerService for RpcService {
    async fn list_checkpoints(
        &self,
        request: tonic::Request<ListCheckpointsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListCheckpointsResponse>>, tonic::Status> {
        serve_list_stream(
            "list_checkpoints",
            self.config.ledger_history().list_checkpoints().timeout,
            list_checkpoints::list_checkpoints(self.clone(), request.into_inner()),
        )
        .await
    }

    async fn list_transactions(
        &self,
        request: tonic::Request<ListTransactionsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListTransactionsResponse>>, tonic::Status> {
        serve_list_stream(
            "list_transactions",
            self.config.ledger_history().list_transactions().timeout,
            list_transactions::list_transactions(self.clone(), request.into_inner()),
        )
        .await
    }

    async fn list_events(
        &self,
        request: tonic::Request<ListEventsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListEventsResponse>>, tonic::Status> {
        serve_list_stream(
            "list_events",
            self.config.ledger_history().list_events().timeout,
            list_events::list_events(self.clone(), request.into_inner()),
        )
        .await
    }
}
