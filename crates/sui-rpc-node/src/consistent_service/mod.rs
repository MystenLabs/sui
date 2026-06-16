// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! gRPC implementation of the
//! [`sui.rpc.consistent.v1alpha`][grpc] service over the
//! `sui-rpc-store` schema and `sui-consistent-store` snapshots.
//!
//! Mirrors the API surface served by
//! `sui-indexer-alt-consistent-store::rpc::consistent_service`,
//! so clients of the alt-consistent-store keep working when
//! they're pointed at a `sui-rpc-node` instead. The wire
//! protocol comes from
//! [`sui_indexer_alt_consistent_api`][crate]; the handler
//! implementations here are bespoke — our schema layout
//! (rpc-store) differs enough from the alt-consistent-store's
//! to make handler reuse impractical, but the read shape is
//! the same and the snapshot semantics (one consistent view
//! per response, anchored at a checkpoint) carry over.
//!
//! [grpc]: sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha
//! [crate]: sui_indexer_alt_consistent_api

mod available_range;
mod balances;
mod objects;
mod pagination;
mod service_config;
mod state;

pub(crate) use crate::consistent_service::state::State;

use async_trait::async_trait;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService;

#[async_trait]
impl ConsistentService for State {
    async fn available_range(
        &self,
        request: tonic::Request<grpc::AvailableRangeRequest>,
    ) -> Result<tonic::Response<grpc::AvailableRangeResponse>, tonic::Status> {
        // Validate the request's checkpoint header even though
        // we don't use the resolved value to compute the
        // response — a malformed or out-of-range header should
        // fail loudly here (with the bounds still stamped on
        // the error metadata via `checkpointed_response`)
        // rather than silently being ignored. Matches
        // alt-consistent-store behaviour.
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|_| Ok(available_range::available_range(self)?)),
        )
    }

    async fn batch_get_balances(
        &self,
        request: tonic::Request<grpc::BatchGetBalancesRequest>,
    ) -> Result<tonic::Response<grpc::BatchGetBalancesResponse>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| {
                    Ok(balances::batch_get_balances(
                        self,
                        cp,
                        request.into_inner(),
                    )?)
                }),
        )
    }

    async fn get_balance(
        &self,
        request: tonic::Request<grpc::GetBalanceRequest>,
    ) -> Result<tonic::Response<grpc::Balance>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| Ok(balances::get_balance(self, cp, request.into_inner())?)),
        )
    }

    async fn list_balances(
        &self,
        request: tonic::Request<grpc::ListBalancesRequest>,
    ) -> Result<tonic::Response<grpc::ListBalancesResponse>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| Ok(balances::list_balances(self, cp, request.into_inner())?)),
        )
    }

    async fn list_objects_by_type(
        &self,
        request: tonic::Request<grpc::ListObjectsByTypeRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| {
                    Ok(objects::list_objects_by_type(
                        self,
                        cp,
                        request.into_inner(),
                    )?)
                }),
        )
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<grpc::ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<grpc::ListObjectsResponse>, tonic::Status> {
        self.checkpointed_response(
            self.checkpoint(&request)
                .map_err(tonic::Status::from)
                .and_then(|cp| Ok(objects::list_owned_objects(self, cp, request.into_inner())?)),
        )
    }

    async fn service_config(
        &self,
        _request: tonic::Request<grpc::ServiceConfigRequest>,
    ) -> Result<tonic::Response<grpc::ServiceConfigResponse>, tonic::Status> {
        self.checkpointed_response(Ok(service_config::service_config(self)))
    }
}
