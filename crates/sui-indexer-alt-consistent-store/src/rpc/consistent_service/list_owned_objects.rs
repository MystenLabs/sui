// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    ListOwnedObjectsRequest, ListOwnedObjectsResponse,
};

use crate::schema::Schema;
use crate::store::Store;

pub(super) fn list_owned_objects(
    _store: &Store<Schema>,
    _request: ListOwnedObjectsRequest,
) -> Result<ListOwnedObjectsResponse, tonic::Status> {
    Err(tonic::Status::unimplemented("Not implemented yet"))
}
