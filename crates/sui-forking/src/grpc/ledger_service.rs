// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetObjectsRequest, BatchGetObjectsResponse, BatchGetTransactionsRequest,
    BatchGetTransactionsResponse, GetCheckpointRequest, GetCheckpointResponse, GetEpochRequest,
    GetEpochResponse, GetObjectRequest, GetObjectResponse, GetServiceInfoRequest,
    GetServiceInfoResponse, GetTransactionRequest, GetTransactionResponse, Object,
    ledger_service_server::LedgerService,
};
use sui_rpc_api::grpc::v2::ledger_service::validate_get_object_requests;
use sui_rpc_api::{ObjectNotFoundError, RpcError};
use sui_types::base_types::{ObjectID, SequenceNumber};

use crate::context::Context;

/// Minimal ledger service for the runnable forking skeleton.
pub struct ForkingLedgerService {
    context: Context,
}

impl ForkingLedgerService {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

#[tonic::async_trait]
impl LedgerService for ForkingLedgerService {
    async fn get_service_info(
        &self,
        _request: tonic::Request<GetServiceInfoRequest>,
    ) -> Result<tonic::Response<GetServiceInfoResponse>, tonic::Status> {
        todo!("get_service_info is not implemented in the runnable skeleton")
    }

    async fn get_object(
        &self,
        request: tonic::Request<GetObjectRequest>,
    ) -> Result<tonic::Response<GetObjectResponse>, tonic::Status> {
        let GetObjectRequest {
            object_id,
            version,
            read_mask,
            ..
        } = request.into_inner();

        let (requests, read_mask) =
            validate_get_object_requests(vec![(object_id, version)], read_mask)
                .map_err(tonic::Status::from)?;
        let (object_id, version) = requests[0];
        let object = self
            .get_object_impl(object_id.into(), version)
            .await
            .map_err(tonic::Status::from)?;

        let mut proto_object = Object::default();
        proto_object.merge(&object, &read_mask);

        Ok(tonic::Response::new(GetObjectResponse::new(proto_object)))
    }

    async fn batch_get_objects(
        &self,
        _request: tonic::Request<BatchGetObjectsRequest>,
    ) -> Result<tonic::Response<BatchGetObjectsResponse>, tonic::Status> {
        todo!("batch_get_objects is not implemented in the runnable skeleton")
    }

    async fn get_transaction(
        &self,
        _request: tonic::Request<GetTransactionRequest>,
    ) -> Result<tonic::Response<GetTransactionResponse>, tonic::Status> {
        todo!("get_transaction is not implemented in the runnable skeleton")
    }

    async fn batch_get_transactions(
        &self,
        _request: tonic::Request<BatchGetTransactionsRequest>,
    ) -> Result<tonic::Response<BatchGetTransactionsResponse>, tonic::Status> {
        todo!("batch_get_transactions is not implemented in the runnable skeleton")
    }

    async fn get_checkpoint(
        &self,
        _request: tonic::Request<GetCheckpointRequest>,
    ) -> Result<tonic::Response<GetCheckpointResponse>, tonic::Status> {
        todo!("get_checkpoint is not implemented in the runnable skeleton")
    }

    async fn get_epoch(
        &self,
        _request: tonic::Request<GetEpochRequest>,
    ) -> Result<tonic::Response<GetEpochResponse>, tonic::Status> {
        todo!("get_epoch is not implemented in the runnable skeleton")
    }
}

impl ForkingLedgerService {
    async fn get_object_impl(
        &self,
        object_id: ObjectID,
        version: Option<u64>,
    ) -> Result<sui_types::object::Object, RpcError> {
        let sim = self.context.simulacrum.read().await;
        let store = sim.store();
        let object = if let Some(version) = version {
            store.get_object_at_version(&object_id, SequenceNumber::from_u64(version))
        } else {
            sui_types::storage::ObjectStore::get_object(store, &object_id)
        };

        object.ok_or_else(|| ObjectNotFoundError::new(object_id.into()).into())
    }
}
