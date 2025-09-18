// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt,
    fmt::{Debug, Formatter},
    sync::Arc,
};

use async_graphql::Context;
use sui_indexer_alt_reader::package_resolver::PackageCache;
use sui_package_resolver::{PackageStore, Resolver};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    object::Object as NativeObject,
};

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
    ///
    /// None indicates execution context where we're viewing fresh transaction effects not yet indexed.
    checkpoint_viewed_at: Option<u64>,

    /// Root parent object version for dynamic fields.
    ///
    /// This enables consistent dynamic field reads in the case of chained dynamic object fields,
    /// e.g., `Parent -> DOF1 -> DOF2`. In such cases, the object versions may end up like
    /// `Parent >= DOF1, DOF2` but `DOF1 < DOF2`.
    ///
    /// Lamport timestamps of objects are updated for all top-level mutable objects provided as
    /// inputs to a transaction as well as any mutated dynamic child objects. However, any dynamic
    /// child objects that were loaded but not actually mutated don't end up having their versions
    /// updated. So, database queries for nested dynamic fields must be bounded by the version of
    /// the root object, and not the immediate parent.
    root_version: Option<u64>,

    /// Cache of objects available in execution context (freshly executed transaction).
    /// Maps (ObjectID, SequenceNumber) to the actual object data.
    /// This enables any Object GraphQL type to access fresh data without database queries.
    execution_objects: Arc<BTreeMap<(ObjectID, SequenceNumber), NativeObject>>,

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
            checkpoint_viewed_at: Some(watermark.high_watermark().checkpoint()),
            root_version: None,
            execution_objects: Arc::new(BTreeMap::new()),
            package_store: package_store.clone(),
            resolver_limits: limits.package_resolver(),
        })
    }

    /// Create a nested scope pinned to a past checkpoint. Returns `None` if the checkpoint is in
    /// the future, or if the current scope is in execution context (no checkpoint is set).
    pub(crate) fn with_checkpoint_viewed_at(&self, checkpoint_viewed_at: u64) -> Option<Self> {
        let current_cp = self.checkpoint_viewed_at?;
        (checkpoint_viewed_at <= current_cp).then(|| Self {
            checkpoint_viewed_at: Some(checkpoint_viewed_at),
            root_version: self.root_version,
            execution_objects: Arc::clone(&self.execution_objects),
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        })
    }

    /// Create a nested scope for execution context (freshly executed transaction).
    /// This clears the checkpoint context to indicate fresh execution data and
    /// sets execution objects from the transaction output.
    pub(crate) fn with_execution_output<I>(&self, objects: I) -> Self
    where
        I: IntoIterator<Item = NativeObject>,
    {
        let execution_objects = Arc::new(
            objects
                .into_iter()
                .map(|obj| ((obj.id(), obj.version()), obj))
                .collect::<BTreeMap<_, _>>(),
        );

        Self {
            checkpoint_viewed_at: None,
            root_version: self.root_version,
            execution_objects,
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        }
    }

    /// Create a nested scope with a root version set.
    pub(crate) fn with_root_version(&self, root_version: u64) -> Self {
        Self {
            checkpoint_viewed_at: self.checkpoint_viewed_at,
            root_version: Some(root_version),
            execution_objects: Arc::clone(&self.execution_objects),
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        }
    }

    /// Reset the root version.
    pub(crate) fn without_root_version(&self) -> Self {
        Self {
            checkpoint_viewed_at: self.checkpoint_viewed_at,
            root_version: None,
            execution_objects: Arc::clone(&self.execution_objects),
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        }
    }

    /// Get the checkpoint being viewed, if any.
    /// Returns `None` in execution context (freshly executed transaction).
    ///
    /// Call sites must explicitly handle the execution context case and decide whether
    /// their operation makes sense without checkpoint context.
    pub(crate) fn checkpoint_viewed_at(&self) -> Option<u64> {
        self.checkpoint_viewed_at
    }

    /// Root parent object version for dynamic fields.
    pub(crate) fn root_version(&self) -> Option<u64> {
        self.root_version
    }

    /// Get the exclusive checkpoint bound, if any.
    ///
    /// Returns `None` in execution context (freshly executed transaction).
    pub(crate) fn checkpoint_viewed_at_exclusive_bound(&self) -> Option<u64> {
        self.checkpoint_viewed_at.map(|cp| cp + 1)
    }

    /// Get an object from the execution context cache, if available.
    pub(crate) fn execution_output_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<&NativeObject> {
        self.execution_objects.get(&(object_id, version))
    }

    /// A package resolver with access to the packages known at this scope.
    pub(crate) fn package_resolver(&self) -> Resolver<Arc<dyn PackageStore>> {
        Resolver::new_with_limits(self.package_store.clone(), self.resolver_limits.clone())
    }
}

impl Debug for Scope {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scope")
            .field("checkpoint_viewed_at", &self.checkpoint_viewed_at)
            .field("root_version", &self.root_version)
            .field("resolver_limits", &self.resolver_limits)
            .finish()
    }
}
