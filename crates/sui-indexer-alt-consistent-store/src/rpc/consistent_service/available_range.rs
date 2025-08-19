// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;

use crate::rpc::error::RpcError;

use super::State;

pub(super) fn available_range(
    state: &State,
    checkpoint: u64,
    grpc::AvailableRangeRequest {}: grpc::AvailableRangeRequest,
) -> Result<grpc::AvailableRangeResponse, RpcError> {
    let range = state.store.db().snapshot_range(checkpoint);
    Ok(grpc::AvailableRangeResponse {
        min_checkpoint: range.as_ref().map(|r| r.start().checkpoint_hi_inclusive),
        max_checkpoint: range.as_ref().map(|r| r.end().checkpoint_hi_inclusive),
        max_epoch: range.as_ref().map(|r| r.end().epoch_hi_inclusive),
        total_transactions: range.as_ref().map(|r| r.end().tx_hi),
        max_timestamp_ms: range.as_ref().map(|r| r.end().timestamp_ms_hi_inclusive),
        stride: Some(state.consistency_config.stride),
    })
}
