// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_rpc::proto::sui::rpc::v2beta2::{
    ledger_service_server::LedgerService, BatchGetObjectsRequest, BatchGetObjectsResponse,
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, GetCheckpointRequest,
    GetCheckpointResponse, GetEpochRequest, GetEpochResponse, GetObjectRequest, GetObjectResponse,
    GetServiceInfoRequest, GetServiceInfoResponse, GetTransactionRequest, GetTransactionResponse,
};
use sui_rpc_api::proto::timestamp_ms_to_proto;
use sui_rpc_api::{CheckpointNotFoundError, RpcError, ServerVersion};
use sui_sdk_types::Digest;
use sui_types::digests::ChainIdentifier;
use sui_types::message_envelope::Message;

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

impl KvRpcServer {
    pub async fn new(
        instance_id: String,
        app_profile_id: Option<String>,
        server_version: Option<ServerVersion>,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let mut client = BigTableClient::new_remote(
            instance_id,
            false,
            None,
            "sui-kv-rpc".to_string(),
            Some(registry),
            app_profile_id,
        )
        .await?;
        let genesis = client
            .get_checkpoints(&[0])
            .await?
            .pop()
            .expect("failed to fetch genesis checkpoint from the KV store");
        let chain_id = ChainIdentifier::from(genesis.summary.digest());
        Ok(Self {
            chain_id,
            client,
            server_version,
        })
    }
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
    ) -> Result<tonic::Response<GetObjectResponse>, tonic::Status> {
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
    ) -> Result<tonic::Response<GetTransactionResponse>, tonic::Status> {
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
    ) -> Result<tonic::Response<GetCheckpointResponse>, tonic::Status> {
        get_checkpoint::get_checkpoint(self.client.clone(), request.into_inner())
            .await
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn get_epoch(
        &self,
        request: tonic::Request<GetEpochRequest>,
    ) -> Result<tonic::Response<GetEpochResponse>, tonic::Status> {
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
    let Some(checkpoint) = client.get_latest_checkpoint_summary().await? else {
        return Err(CheckpointNotFoundError::sequence_number(0).into());
    };
    let mut message = GetServiceInfoResponse::default();
    message.chain_id = Some(Digest::new(chain_id.as_bytes().to_owned()).to_string());
    message.chain = Some(chain_id.chain().as_str().into());
    message.epoch = Some(checkpoint.epoch);
    message.checkpoint_height = Some(checkpoint.sequence_number);
    message.timestamp = Some(timestamp_ms_to_proto(checkpoint.timestamp_ms));
    message.lowest_available_checkpoint = Some(0);
    message.lowest_available_checkpoint_objects = Some(0);
    message.server = server_version.as_ref().map(ToString::to_string);
    Ok(message)
}
