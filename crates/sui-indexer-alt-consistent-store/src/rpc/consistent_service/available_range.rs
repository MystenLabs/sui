// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;

use crate::rpc::consistent_service::State;
use crate::rpc::error::RpcError;

pub(super) fn available_range(
    state: &State,
    checkpoint: u64,
    grpc::AvailableRangeRequest {}: grpc::AvailableRangeRequest,
) -> Result<grpc::AvailableRangeResponse, RpcError> {
    let range = state.store.db().snapshot_range(checkpoint);
    let min_checkpoint = range.as_ref().map(|r| {
        r.start().checkpoint_hi.checked_sub(1).unwrap_or_else(|| {
            panic!("Snapshot range start checkpoint_hi underflow checkpoint={checkpoint}")
        })
    });
    let max_checkpoint = range.as_ref().map(|r| {
        r.end().checkpoint_hi.checked_sub(1).unwrap_or_else(|| {
            panic!("Snapshot range end checkpoint_hi underflow checkpoint={checkpoint}")
        })
    });
    Ok(grpc::AvailableRangeResponse {
        min_checkpoint,
        max_checkpoint,
        max_epoch: range.as_ref().map(|r| r.end().epoch_hi_inclusive),
        total_transactions: range.as_ref().map(|r| r.end().tx_hi),
        max_timestamp_ms: range.as_ref().map(|r| r.end().timestamp_ms_hi_inclusive),
        stride: Some(state.consistency_config.stride),
    })
}
