// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The checkpoint height a replica serves, and the bound it puts on answers.
//!
//! The indexer's pipelines write their rows independently, so a checkpoint's `checkpoints` row can
//! exist before its `transactions` and `objects` rows do. Only the pipelines' watermarks promise
//! that every row of a checkpoint is written, so every answer that depends on how far the ledger
//! has been indexed is bounded by the lowest of them: the height `GetServiceInfo` reports. A
//! request reads it once, from the service info it was admitted at (see [`crate::admission`]), so
//! one answer never mixes two heights. A caller that has seen a higher checkpoint from another
//! replica asks for it with `x-sui-min-checkpoint`.

use sui_rpc_api::RpcError;

use crate::admission::AdmittedServiceInfo;

/// The highest checkpoint served (inclusive) and the epoch it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ServedHeight {
    pub(crate) checkpoint: u64,
    pub(crate) epoch: u64,
}

impl ServedHeight {
    /// The height `request` is served at, from the service info it was admitted at.
    /// `Unavailable` if the replica had no served height when it admitted the request.
    pub(crate) fn of<T>(request: &tonic::Request<T>) -> Result<Self, RpcError> {
        let info = AdmittedServiceInfo::of(request)?;
        match (
            info.and_then(|info| info.checkpoint_height),
            info.and_then(|info| info.epoch),
        ) {
            (Some(checkpoint), Some(epoch)) => Ok(ServedHeight { checkpoint, epoch }),
            _ => Err(RpcError::new(
                tonic::Code::Unavailable,
                "service info cache missing checkpoint height",
            )),
        }
    }
}
