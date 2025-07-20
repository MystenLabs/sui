// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    ListOwnedObjectsRequest, ListOwnedObjectsResponse,
};

use crate::rpc::error::RpcError;
use crate::schema::Schema;
use crate::store::Store;

pub(super) fn list_owned_objects(
    _store: &Store<Schema>,
    _request: ListOwnedObjectsRequest,
) -> Result<ListOwnedObjectsResponse, RpcError> {
    Err(RpcError::Unimplemented)
}
