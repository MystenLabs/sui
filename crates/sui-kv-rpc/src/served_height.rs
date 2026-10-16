// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The checkpoint height a replica serves, and the bound it puts on answers.
//!
//! The indexer's pipelines write their rows independently, so a checkpoint's `checkpoints` row can
//! exist before its `transactions` and `objects` rows do. Only the pipelines' watermarks promise
//! that every row of a checkpoint is written, so every answer that depends on how far the ledger
//! has been indexed is bounded by the lowest of them: the height `GetServiceInfo` reports, read
//! from the replica's cached service info. A caller that has seen a higher checkpoint from another
//! replica asks for it with `x-sui-min-checkpoint` (see [`crate::monotonic_read`]).

use sui_rpc_api::RpcError;

use crate::KvRpcServer;

/// The highest checkpoint served (inclusive) and the epoch it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ServedHeight {
    pub(crate) checkpoint: u64,
    pub(crate) epoch: u64,
}

impl KvRpcServer {
    /// This replica's served height, from its cached service info. `Unavailable` before the
    /// first refresh.
    pub(crate) fn served_height(&self) -> Result<ServedHeight, RpcError> {
        let cache = self.cache.borrow();
        let info = cache.as_ref();
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
