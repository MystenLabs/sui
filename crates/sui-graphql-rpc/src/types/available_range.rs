// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::checkpoint::Checkpoint;
use crate::context_data::db_data_provider::PgManager;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AvailableRange {
    pub first: u64,
    pub last: u64,
}

// TODO: do both in one query?
#[Object]
impl AvailableRange {
    async fn first(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_checkpoint(None, Some(self.first))
            .await
            .extend()
    }

    async fn last(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_checkpoint(None, Some(self.last))
            .await
            .extend()
    }
}
