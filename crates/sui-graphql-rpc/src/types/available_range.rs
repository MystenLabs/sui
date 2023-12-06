// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::checkpoint::Checkpoint;
use crate::context_data::db_data_provider::PgManager;
use async_graphql::*;

/// Information about whether epoch changes are using safe mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AvailableRange;

// TODO: do both in one query?
#[Object]
impl AvailableRange {
    async fn first(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_earliest_complete_checkpoint()
            .await
            .extend()
    }

    async fn last(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        ctx.data_unchecked::<PgManager>()
            .fetch_checkpoint(None, None)
            .await
            .extend()
    }
}
