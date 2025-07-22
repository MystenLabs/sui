// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;

use crate::rpc::error::RpcError;

use super::State;

pub(super) fn available_range(
    state: &State,
    grpc::AvailableRangeRequest {}: grpc::AvailableRangeRequest,
) -> Result<grpc::AvailableRangeResponse, RpcError> {
    let range = state.store.db().snapshot_range();
    Ok(grpc::AvailableRangeResponse {
        min_checkpoint: range.as_ref().map(|r| *r.start()),
        max_checkpoint: range.as_ref().map(|r| *r.end()),
    })
}
