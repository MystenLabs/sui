// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    AvailableRangeRequest, AvailableRangeResponse, ListOwnedObjectsRequest,
    ListOwnedObjectsResponse, ServiceConfigRequest, ServiceConfigResponse,
};

use super::state::{checkpointed_response, State};

use self::available_range::available_range;
use self::list_owned_objects::list_owned_objects;
use self::service_config::service_config;

mod available_range;
mod list_owned_objects;
mod service_config;

#[async_trait::async_trait]
impl ConsistentService for State {
    async fn available_range(
        &self,
        request: tonic::Request<AvailableRangeRequest>,
    ) -> Result<tonic::Response<AvailableRangeResponse>, tonic::Status> {
        available_range(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn list_owned_objects(
        &self,
        request: tonic::Request<ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<ListOwnedObjectsResponse>, tonic::Status> {
        let checkpoint = self.checkpoint(&request)?;
        let response = list_owned_objects(self, checkpoint, request.into_inner())?;
        Ok(checkpointed_response(checkpoint, response)?)
    }

    async fn service_config(
        &self,
        request: tonic::Request<ServiceConfigRequest>,
    ) -> Result<tonic::Response<ServiceConfigResponse>, tonic::Status> {
        service_config(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}
