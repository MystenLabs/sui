// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_rpc_api::proto::rpc::v2beta::{
    ledger_service_server::LedgerService, BatchGetObjectsRequest, BatchGetObjectsResponse,
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, Checkpoint, Epoch,
    ExecutedTransaction, GetCheckpointRequest, GetEpochRequest, GetObjectRequest,
    GetServiceInfoRequest, GetServiceInfoResponse, GetTransactionRequest, Object,
};
use sui_rpc_api::proto::timestamp_ms_to_proto;
use sui_rpc_api::{CheckpointNotFoundError, RpcError, ServerVersion};
use sui_sdk_types::CheckpointDigest;
use sui_types::digests::ChainIdentifier;

mod get_checkpoint;
mod get_epoch;
mod get_object;
mod get_transaction;

#[derive(Clone)]
pub struct KvRpcServer {
    chain_id: ChainIdentifier,
    client: BigTableClient,
    server_version: Option<ServerVersion>,
}

#[tonic::async_trait]
impl LedgerService for KvRpcServer {
    async fn get_service_info(
        &self,
        _: tonic::Request<GetServiceInfoRequest>,
    ) -> Result<tonic::Response<GetServiceInfoResponse>, tonic::Status> {
        get_service_info(
            self.client.clone(),
            self.chain_id,
            self.server_version.clone(),
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }

    async fn get_object(
        &self,
        request: tonic::Request<GetObjectRequest>,
    ) -> Result<tonic::Response<Object>, tonic::Status> {
        get_object::get_object(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn batch_get_objects(
        &self,
        request: tonic::Request<BatchGetObjectsRequest>,
    ) -> Result<tonic::Response<BatchGetObjectsResponse>, tonic::Status> {
        get_object::batch_get_objects(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_transaction(
        &self,
        request: tonic::Request<GetTransactionRequest>,
    ) -> Result<tonic::Response<ExecutedTransaction>, tonic::Status> {
        get_transaction::get_transaction(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn batch_get_transactions(
        &self,
        request: tonic::Request<BatchGetTransactionsRequest>,
    ) -> Result<tonic::Response<BatchGetTransactionsResponse>, tonic::Status> {
        get_transaction::batch_get_transactions(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_checkpoint(
        &self,
        request: tonic::Request<GetCheckpointRequest>,
    ) -> Result<tonic::Response<Checkpoint>, tonic::Status> {
        get_checkpoint::get_checkpoint(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_epoch(
        &self,
        request: tonic::Request<GetEpochRequest>,
    ) -> Result<tonic::Response<Epoch>, tonic::Status> {
        get_epoch::get_epoch(
            self.client.clone(),
            request.into_inner(),
            self.chain_id.chain(),
        )
        .await
        .map(tonic::Response::new)
        .map_err(Into::into)
    }
}

async fn get_service_info(
    mut client: BigTableClient,
    chain_id: ChainIdentifier,
    server_version: Option<ServerVersion>,
) -> Result<GetServiceInfoResponse, RpcError> {
    let seq_number = client.get_latest_checkpoint().await?;
    let checkpoint = client.get_checkpoints(&[seq_number]).await?.pop();
    let Some(checkpoint) = checkpoint else {
        return Err(CheckpointNotFoundError::sequence_number(seq_number).into());
    };
    Ok(GetServiceInfoResponse {
        chain_id: Some(CheckpointDigest::new(chain_id.as_bytes().to_owned()).to_string()),
        chain: Some(chain_id.chain().as_str().into()),
        epoch: Some(checkpoint.summary.epoch),
        checkpoint_height: Some(seq_number),
        timestamp: Some(timestamp_ms_to_proto(checkpoint.summary.timestamp_ms)),
        lowest_available_checkpoint: Some(0),
        lowest_available_checkpoint_objects: Some(0),
        server_version: server_version.as_ref().map(ToString::to_string),
    })
}
