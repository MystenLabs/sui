// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    ListOwnedObjectsRequest, ListOwnedObjectsResponse, ServiceConfigRequest, ServiceConfigResponse,
};

use super::state::State;

mod list_owned_objects;
mod service_config;

#[async_trait::async_trait]
impl ConsistentService for State {
    async fn list_owned_objects(
        &self,
        request: tonic::Request<ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<ListOwnedObjectsResponse>, tonic::Status> {
        list_owned_objects::list_owned_objects(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn service_config(
        &self,
        request: tonic::Request<ServiceConfigRequest>,
    ) -> Result<tonic::Response<ServiceConfigResponse>, tonic::Status> {
        service_config::service_config(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}
