// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::Context;

use crate::{error::RpcError, task::watermark::Watermarks};

/// A way to share information between fields in a request, similar to [Context].
///
/// Unlike [Context], [Scope] is not referenced by every field resolver. Instead, fields must
/// explicitly thread [Scope]-s to where they are needed, and are able to override them when
/// necessary, allowing a nested scope to shadow values in its parent scope.
#[derive(Clone, Debug)]
pub(crate) struct Scope {
    /// The checkpoint we are viewing this data at. Queries for the latest versions of an entity
    /// are relative to this checkpoint.
    checkpoint_viewed_at: u64,
}

impl Scope {
    /// Create a new scope at the top-level (initialized by information we have at the root of a
    /// request).
    pub(crate) fn new<E: std::error::Error>(ctx: &Context<'_>) -> Result<Self, RpcError<E>> {
        let watermark: &Arc<Watermarks> = ctx.data()?;

        Ok(Self {
            checkpoint_viewed_at: watermark.high_watermark().checkpoint(),
        })
    }

    /// Created a nested scope pinned to a past checkpoint. Returns `None` if the checkpoint is in
    /// the future.
    pub(crate) fn with_checkpoint_viewed_at(&self, checkpoint_viewed_at: u64) -> Option<Self> {
        (checkpoint_viewed_at <= self.checkpoint_viewed_at).then_some(Self {
            checkpoint_viewed_at,
        })
    }

    // inclusive upper bound on data visible to request
    pub(crate) fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }

    // exclusive upper bound on data visible to request
    pub(crate) fn checkpoint_viewed_at_exclusive_bound(&self) -> u64 {
        self.checkpoint_viewed_at + 1
    }
}
