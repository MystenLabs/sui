// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::KvRpcServer;
use crate::operation::OperationSpec;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_server::LedgerService;
use tonic::codegen::BoxStream;

mod list_checkpoints;
mod list_events;
mod list_transactions;

// Per-RPC hard request timeout (from `ListApiConfig`). The outer
// `operation::with_deadline` wrapper drops the response stream with
// `DeadlineExceeded` when this fires; debounced intermediate `Watermark` frames
// let the client resume from wherever it got to.
#[tonic::async_trait]
impl LedgerService for KvRpcServer {
    async fn list_checkpoints(
        &self,
        request: tonic::Request<ListCheckpointsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListCheckpointsResponse>>, tonic::Status> {
        self.serve_query_stream(
            OperationSpec::new("list_checkpoints", self.list_api.list_checkpoints.timeout),
            request,
            list_checkpoints::list_checkpoints,
        )
        .await
    }

    async fn list_transactions(
        &self,
        request: tonic::Request<ListTransactionsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListTransactionsResponse>>, tonic::Status> {
        self.serve_query_stream(
            OperationSpec::new("list_transactions", self.list_api.list_transactions.timeout),
            request,
            list_transactions::list_transactions,
        )
        .await
    }

    async fn list_events(
        &self,
        request: tonic::Request<ListEventsRequest>,
    ) -> Result<tonic::Response<BoxStream<ListEventsResponse>>, tonic::Status> {
        self.serve_query_stream(
            OperationSpec::new("list_events", self.list_api.list_events.timeout),
            request,
            list_events::list_events,
        )
        .await
    }
}
