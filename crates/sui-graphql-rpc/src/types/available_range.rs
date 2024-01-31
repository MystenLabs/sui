// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::checkpoint::{Checkpoint, CheckpointId};
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AvailableRange {
    pub first: u64,
    pub last: u64,
}

// TODO: do both in one query?
/// Range of checkpoints that the RPC is guaranteed to produce a consistent response for.
#[Object]
impl AvailableRange {
    async fn first(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        Checkpoint::query(
            ctx.data_unchecked(),
            CheckpointId::by_seq_num(self.first),
            Some(self.last),
        )
        .await
        .extend()
    }

    async fn last(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        Checkpoint::query(
            ctx.data_unchecked(),
            CheckpointId::by_seq_num(self.last),
            Some(self.last),
        )
        .await
        .extend()
    }
}
