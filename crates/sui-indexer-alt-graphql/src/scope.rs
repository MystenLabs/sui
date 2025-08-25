// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::Context;
use sui_indexer_alt_reader::package_resolver::PackageCache;
use sui_package_resolver::{PackageStore, Resolver};

use crate::{config::Limits, error::RpcError, task::watermark::Watermarks};

/// A way to share information between fields in a request, similar to [Context].
///
/// Unlike [Context], [Scope] is not referenced by every field resolver. Instead, fields must
/// explicitly thread [Scope]-s to where they are needed, and are able to override them when
/// necessary, allowing a nested scope to shadow values in its parent scope.
#[derive(Clone)]
pub(crate) struct Scope {
    /// The checkpoint we are viewing this data at. Queries for the latest versions of an entity
    /// are relative to this checkpoint.
    checkpoint_viewed_at: u64,

    /// Access to packages for type resolution.
    package_store: Arc<dyn PackageStore>,

    /// Limits for package/type resolution.
    resolver_limits: sui_package_resolver::Limits,
}

impl Scope {
    /// Create a new scope at the top-level (initialized by information we have at the root of a
    /// request).
    pub(crate) fn new<E: std::error::Error>(ctx: &Context<'_>) -> Result<Self, RpcError<E>> {
        let watermark: &Arc<Watermarks> = ctx.data()?;
        let package_store: &Arc<PackageCache> = ctx.data()?;
        let limits: &Limits = ctx.data()?;

        Ok(Self {
            checkpoint_viewed_at: watermark.high_watermark().checkpoint(),
            package_store: package_store.clone(),
            resolver_limits: limits.package_resolver(),
        })
    }

    /// Created a nested scope pinned to a past checkpoint. Returns `None` if the checkpoint is in
    /// the future.
    pub(crate) fn with_checkpoint_viewed_at(&self, checkpoint_viewed_at: u64) -> Option<Self> {
        (checkpoint_viewed_at <= self.checkpoint_viewed_at).then(|| Self {
            checkpoint_viewed_at,
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        })
    }

    /// Inclusive upper bound on data visible to request
    pub(crate) fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }

    /// Exclusive upper bound on data visible to request
    pub(crate) fn checkpoint_viewed_at_exclusive_bound(&self) -> u64 {
        self.checkpoint_viewed_at + 1
    }

    /// A package resolver with access to the packages known at this scope.
    pub(crate) fn package_resolver(&self) -> Resolver<Arc<dyn PackageStore>> {
        Resolver::new_with_limits(self.package_store.clone(), self.resolver_limits.clone())
    }
}
