// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `ConsistentService::available_range` — surface the
//! consistent-read window the service can answer queries for.
//!
//! `min_checkpoint` / `max_checkpoint` come from the live
//! snapshot range on the [`Db`]. The auxiliary fields
//! (`max_epoch`, `total_transactions`, `max_timestamp_ms`) are
//! read off the top snapshot's [`Watermark`], which the
//! [`Synchronizer`] records when it captures the snapshot.
//! `stride` mirrors the configured snapshot stride so clients
//! know which checkpoints they can request.
//!
//! [`Db`]: sui_consistent_store::Db
//! [`Watermark`]: sui_consistent_store::Watermark
//! [`Synchronizer`]: sui_consistent_store::Synchronizer

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;

use crate::consistent_service::State;
use crate::consistent_service::state::Error;

pub(super) fn available_range(state: &State) -> Result<grpc::AvailableRangeResponse, Error> {
    let range = state.db.snapshot_range().ok_or(Error::NoSnapshots)?;
    // The top snapshot's watermark gives us the epoch /
    // tx-count / timestamp for the high-water checkpoint. If it
    // raced with eviction (unlikely between `snapshot_range`
    // and now, but possible under aggressive eviction), report
    // `NoSnapshots` rather than half-populating the response.
    let top = state
        .db
        .at_snapshot(*range.end())
        .ok_or(Error::NoSnapshots)?;
    let watermark = top.watermark();

    Ok(grpc::AvailableRangeResponse {
        min_checkpoint: Some(*range.start()),
        max_checkpoint: Some(*range.end()),
        max_epoch: Some(watermark.epoch_hi_inclusive),
        total_transactions: Some(watermark.tx_hi),
        max_timestamp_ms: Some(watermark.timestamp_ms_hi_inclusive),
        stride: Some(state.consistency.stride),
    })
}
