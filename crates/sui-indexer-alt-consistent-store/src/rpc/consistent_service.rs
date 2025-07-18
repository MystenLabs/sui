// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    ListOwnedObjectsRequest, ListOwnedObjectsResponse,
};

use crate::schema::Schema;
use crate::store::Store;

#[async_trait::async_trait]
impl ConsistentService for Store<Schema> {
    async fn list_owned_objects(
        &self,
        _request: tonic::Request<ListOwnedObjectsRequest>,
    ) -> Result<tonic::Response<ListOwnedObjectsResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("Not implemented yet"))
    }
}
