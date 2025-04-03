// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::rpc::v2beta::ledger_service_server::LedgerService;
use crate::proto::rpc::v2beta::BatchGetObjectsRequest;
use crate::proto::rpc::v2beta::BatchGetObjectsResponse;
use crate::proto::rpc::v2beta::BatchGetTransactionsRequest;
use crate::proto::rpc::v2beta::BatchGetTransactionsResponse;
use crate::proto::rpc::v2beta::Checkpoint;
use crate::proto::rpc::v2beta::Epoch;
use crate::proto::rpc::v2beta::ExecutedTransaction;
use crate::proto::rpc::v2beta::GetCheckpointRequest;
use crate::proto::rpc::v2beta::GetEpochRequest;
use crate::proto::rpc::v2beta::GetObjectRequest;
use crate::proto::rpc::v2beta::GetServiceInfoRequest;
use crate::proto::rpc::v2beta::GetServiceInfoResponse;
use crate::proto::rpc::v2beta::GetTransactionRequest;
use crate::proto::rpc::v2beta::Object;
use crate::RpcService;

pub(crate) mod get_checkpoint;
mod get_epoch;
mod get_object;
mod get_service_info;
mod get_transaction;

#[tonic::async_trait]
impl LedgerService for RpcService {
    async fn get_service_info(
        &self,
        _request: tonic::Request<GetServiceInfoRequest>,
    ) -> Result<tonic::Response<GetServiceInfoResponse>, tonic::Status> {
        get_service_info::get_service_info(self)
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_object(
        &self,
        request: tonic::Request<GetObjectRequest>,
    ) -> Result<tonic::Response<Object>, tonic::Status> {
        get_object::get_object(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn batch_get_objects(
        &self,
        request: tonic::Request<BatchGetObjectsRequest>,
    ) -> Result<tonic::Response<BatchGetObjectsResponse>, tonic::Status> {
        get_object::batch_get_objects(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_transaction(
        &self,
        request: tonic::Request<GetTransactionRequest>,
    ) -> Result<tonic::Response<ExecutedTransaction>, tonic::Status> {
        get_transaction::get_transaction(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn batch_get_transactions(
        &self,
        request: tonic::Request<BatchGetTransactionsRequest>,
    ) -> Result<tonic::Response<BatchGetTransactionsResponse>, tonic::Status> {
        get_transaction::batch_get_transactions(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_checkpoint(
        &self,
        request: tonic::Request<GetCheckpointRequest>,
    ) -> Result<tonic::Response<Checkpoint>, tonic::Status> {
        get_checkpoint::get_checkpoint(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_epoch(
        &self,
        request: tonic::Request<GetEpochRequest>,
    ) -> Result<tonic::Response<Epoch>, tonic::Status> {
        get_epoch::get_epoch(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}
