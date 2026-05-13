// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;

use async_graphql::Context;
use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use sui_indexer_alt_reader::package_resolver::PackageCache;
use sui_package_resolver::Package;
use sui_package_resolver::PackageStore;
use sui_package_resolver::Resolver;
use sui_package_resolver::error::Error as PackageResolverError;
use sui_rpc::proto::sui::rpc::v2 as grpc;
use sui_rpc::proto::sui::rpc::v2::changed_object::OutputObjectState;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;
use sui_types::object::Object as NativeObject;

use crate::config::Limits;
use crate::error::RpcError;
use crate::task::streaming::ProcessedCheckpoint;
use crate::task::streaming::ProcessedTransaction;
use crate::task::streaming::StreamingPackageStore;
use crate::task::watermark::Watermarks;

/// A map of objects from an executed transaction, keyed by (ObjectID, SequenceNumber).
/// None values indicate tombstones for deleted/wrapped objects.
pub(crate) type ExecutionObjectMap =
    Arc<BTreeMap<(ObjectID, SequenceNumber), Option<NativeObject>>>;

/// Where in-memory lookups (objects, transaction contents, etc.) in this scope draw their data
/// from. Encodes the mutually exclusive modes a [`Scope`] can be in. Indexed mode hits the
/// database/kv_loader (no in-memory payload). Executed mode is the mutation/simulate path, with a
/// freshly executed transaction's outputs. Streamed mode is the subscription path, with an
/// in-memory checkpoint payload.
#[derive(Clone)]
pub(crate) enum DataSource {
    /// Reads go through the indexed-checkpoint path (kv_loader / DB). No in-memory payload.
    Indexed,
    /// A freshly executed transaction's input/output objects.
    Executed {
        execution_objects: ExecutionObjectMap,
    },
    /// A streamed checkpoint with all per-tx contents and a checkpoint-wide execution-objects
    /// map. If the scope is anchored to a particular transaction in the checkpoint, its digest
    /// is in [`Scope::active_transaction_digest`].
    Streamed {
        checkpoint: Arc<ProcessedCheckpoint>,
    },
}

/// Root object bound for consistent dynamic field reads.
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
///
/// The bound can be expressed either in terms of a specific object version or a checkpoint.
#[derive(Clone, Copy, Debug)]
pub(crate) enum RootBound {
    Version(u64),
    Checkpoint(u64),
}

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

    /// The specific transaction this scope is anchored to, identified by digest. Set by
    /// resolvers that render data for a single transaction (e.g. the streaming `transactions`
    /// and `events` subscriptions) so downstream fields like `Address.asTransactionObject`
    /// know which transaction's effects to scan. `None` when no specific transaction is in
    /// scope.
    ///
    /// This is *not* a "view bound" up to and including a transaction; it identifies one
    /// transaction. Object visibility is end-of-checkpoint regardless of this field.
    active_transaction_digest: Option<TransactionDigest>,

    /// Root object bound for dynamic fields.
    ///
    /// This can be expressed either in terms of a specific object version or a checkpoint.
    root_bound: Option<RootBound>,

    /// Where in-memory lookups in this scope draw their data from. See [`DataSource`].
    data_source: DataSource,

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
            active_transaction_digest: None,
            root_bound: None,
            data_source: DataSource::Indexed,
            package_store: package_store.clone(),
            resolver_limits: limits.package_resolver(),
        })
    }

    /// Create a scope for streamed checkpoint data. Sets `checkpoint_viewed_at` to `None`
    /// because streamed data is resolved from memory, not bounded by an indexed checkpoint.
    pub(crate) fn for_streamed_checkpoint(
        package_store: Arc<StreamingPackageStore>,
        resolver_limits: sui_package_resolver::Limits,
        streamed_checkpoint: Arc<ProcessedCheckpoint>,
    ) -> Self {
        Self {
            checkpoint_viewed_at: None,
            active_transaction_digest: None,
            root_bound: None,
            data_source: DataSource::Streamed {
                checkpoint: streamed_checkpoint,
            },
            package_store,
            resolver_limits,
        }
    }

    /// Anchor a nested scope to a specific transaction by digest. Used by resolvers that
    /// render a single transaction's data so downstream fields can identify which
    /// transaction's effects to consult. Does not change object visibility.
    pub(crate) fn with_active_transaction_digest(&self, digest: TransactionDigest) -> Self {
        Self {
            checkpoint_viewed_at: self.checkpoint_viewed_at,
            active_transaction_digest: Some(digest),
            root_bound: self.root_bound,
            data_source: self.data_source.clone(),
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        }
    }

    /// Create a scope instance for tests with no package data.
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        #[derive(Clone)]
        struct EmptyPackageStore;

        #[async_trait]
        impl PackageStore for EmptyPackageStore {
            async fn fetch(
                &self,
                id: AccountAddress,
            ) -> Result<Arc<Package>, PackageResolverError> {
                Err(PackageResolverError::PackageNotFound(id))
            }
        }

        Self {
            checkpoint_viewed_at: Some(0),
            active_transaction_digest: None,
            root_bound: None,
            data_source: DataSource::Indexed,
            package_store: Arc::new(EmptyPackageStore),
            resolver_limits: Limits::default().package_resolver(),
        }
    }

    /// Create a nested scope pinned to a checkpoint. Returns `None` if the checkpoint is in
    /// the future, or if the current scope is in execution context (no checkpoint is set).
    pub(crate) fn with_checkpoint_viewed_at(
        &self,
        ctx: &Context<'_>,
        checkpoint_viewed_at: u64,
    ) -> Option<Self> {
        let watermark: &Arc<Watermarks> = ctx.data().ok()?;
        let cp_hi_inclusive = watermark.high_watermark().checkpoint();
        (checkpoint_viewed_at <= cp_hi_inclusive).then(|| Self {
            checkpoint_viewed_at: Some(checkpoint_viewed_at),
            active_transaction_digest: self.active_transaction_digest,
            root_bound: self.root_bound,
            data_source: self.data_source.clone(),
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        })
    }

    /// Create a nested scope with a root version bound.
    pub(crate) fn with_root_version(&self, root_version: u64) -> Self {
        Self {
            checkpoint_viewed_at: self.checkpoint_viewed_at,
            active_transaction_digest: self.active_transaction_digest,
            root_bound: Some(RootBound::Version(root_version)),
            data_source: self.data_source.clone(),
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        }
    }

    /// Create a nested scope with a root checkpoint bound.
    pub(crate) fn with_root_checkpoint(&self, root_checkpoint: u64) -> Self {
        Self {
            checkpoint_viewed_at: self.checkpoint_viewed_at,
            active_transaction_digest: self.active_transaction_digest,
            root_bound: Some(RootBound::Checkpoint(root_checkpoint)),
            data_source: self.data_source.clone(),
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        }
    }

    /// Reset the root bound.
    pub(crate) fn without_root_bound(&self) -> Self {
        Self {
            checkpoint_viewed_at: self.checkpoint_viewed_at,
            active_transaction_digest: self.active_transaction_digest,
            root_bound: None,
            data_source: self.data_source.clone(),
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
    /// Returns `Some(v)` only if the root bound is version-based.
    pub(crate) fn root_version(&self) -> Option<u64> {
        if let Some(RootBound::Version(v)) = self.root_bound {
            Some(v)
        } else {
            None
        }
    }

    /// Root checkpoint bound for dynamic fields.
    /// Returns the checkpoint bound if set, otherwise falls back to `checkpoint_viewed_at`.
    pub(crate) fn root_checkpoint(&self) -> Option<u64> {
        if let Some(RootBound::Checkpoint(cp)) = self.root_bound {
            Some(cp)
        } else {
            self.checkpoint_viewed_at
        }
    }

    /// Get the exclusive checkpoint bound, if any.
    ///
    /// Returns `None` in execution context (freshly executed transaction).
    pub(crate) fn checkpoint_viewed_at_exclusive_bound(&self) -> Option<u64> {
        self.checkpoint_viewed_at.map(|cp| cp + 1)
    }

    /// Lookup a transaction by digest within the streamed checkpoint backing this scope.
    /// Returns `None` if the scope is not in [`DataSource::Streamed`] mode, or if no
    /// transaction with that digest exists in the checkpoint.
    pub(crate) fn streamed_transaction_by_digest(
        &self,
        digest: TransactionDigest,
    ) -> Option<&ProcessedTransaction> {
        match &self.data_source {
            DataSource::Streamed { checkpoint } => checkpoint.transaction_by_digest(digest),
            DataSource::Indexed | DataSource::Executed { .. } => None,
        }
    }

    /// The execution objects map active for object lookups in this scope. Resolves through the
    /// scope's [`DataSource`]: in `Streamed` mode, returns the checkpoint-wide map (object
    /// visibility is end-of-checkpoint, matching the indexed Query path); in `Executed` mode,
    /// returns the freshly extracted map; in `Indexed` mode, returns `None` so callers fall
    /// through to the DB.
    fn execution_objects_in_view(&self) -> Option<&ExecutionObjectMap> {
        match &self.data_source {
            DataSource::Indexed => None,
            DataSource::Executed { execution_objects } => Some(execution_objects),
            DataSource::Streamed { checkpoint } => Some(&checkpoint.execution_objects),
        }
    }

    /// Get an object from the execution context cache, if available.
    ///
    /// Returns None if the object doesn't exist in the cache or if it was deleted/wrapped.
    pub(crate) fn execution_output_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<&NativeObject> {
        self.execution_objects_in_view()?
            .get(&(object_id, version))
            .and_then(|opt| opt.as_ref())
    }

    /// Get the latest version of an object from the execution context cache, if available.
    /// Returns None if the object doesn't exist in the cache or if it was deleted/wrapped.
    pub(crate) fn execution_output_object_latest(
        &self,
        object_id: ObjectID,
    ) -> Option<&NativeObject> {
        self.execution_objects_in_view()?
            .range(..=(object_id, SequenceNumber::MAX))
            .last()
            .and_then(|(_, opt)| opt.as_ref())
    }

    /// Create a nested scope with execution objects extracted from an ExecutedTransaction.
    pub(crate) fn with_executed_transaction(
        &self,
        executed_transaction: &grpc::ExecutedTransaction,
    ) -> Result<Self, RpcError> {
        let execution_objects = extract_objects_from_executed_transaction(executed_transaction)?;

        Ok(Self {
            checkpoint_viewed_at: None,
            active_transaction_digest: None,
            root_bound: self.root_bound,
            data_source: DataSource::Executed { execution_objects },
            package_store: self.package_store.clone(),
            resolver_limits: self.resolver_limits.clone(),
        })
    }

    /// A package resolver with access to the packages known at this scope.
    pub(crate) fn package_resolver(&self) -> Resolver<Self> {
        Resolver::new_with_limits(self.clone(), self.resolver_limits.clone())
    }
}

/// Extract object contents from an ExecutedTransaction, including tombstones for deleted/wrapped objects.
///
/// Returns a BTreeMap mapping (ObjectID, SequenceNumber) to Option<NativeObject>,
/// where None indicates the object was deleted or wrapped at that version.
fn extract_objects_from_executed_transaction(
    executed_transaction: &grpc::ExecutedTransaction,
) -> Result<ExecutionObjectMap, RpcError> {
    use anyhow::Context;

    let mut map = BTreeMap::new();

    // Extract all objects from the ObjectSet.
    if let Some(object_set) = &executed_transaction.objects {
        for obj in &object_set.objects {
            if let Some(bcs) = &obj.bcs {
                let native_obj: NativeObject = bcs
                    .deserialize()
                    .context("Failed to deserialize object BCS")?;
                map.insert((native_obj.id(), native_obj.version()), Some(native_obj));
            }
        }
    }

    // Add tombstones for objects that no longer exist from effects
    if let Some(effects) = &executed_transaction.effects {
        // Get lamport version directly from gRPC effects
        let lamport_version = SequenceNumber::from_u64(
            effects
                .lamport_version
                .context("Effects should have lamport_version")?,
        );

        for changed_obj in &effects.changed_objects {
            if changed_obj.output_state() == OutputObjectState::DoesNotExist {
                let object_id = changed_obj
                    .object_id
                    .as_ref()
                    .and_then(|id| id.parse().ok())
                    .context("ChangedObject should have valid object_id")?;

                // Deleted/wrapped objects don't have an output_version, so we use the transaction's
                // lamport version as the tombstone version. This ensures execution_output_object_latest
                // returns None for these objects.
                map.insert((object_id, lamport_version), None);
            }
        }
    }

    Ok(Arc::new(map))
}

impl Debug for Scope {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scope")
            .field("checkpoint_viewed_at", &self.checkpoint_viewed_at)
            .field("root_bound", &self.root_bound)
            .field("active_transaction_digest", &self.active_transaction_digest)
            .field("resolver_limits", &self.resolver_limits)
            .finish()
    }
}

#[async_trait]
impl PackageStore for Scope {
    /// Fetches a package, first checking execution context objects if available,
    /// then falling back to the underlying package store.
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>, PackageResolverError> {
        let object_id = ObjectID::from(id);

        // First check execution context objects if we have any
        if let Some(execution_objects) = self.execution_objects_in_view() {
            let latest_package = execution_objects
                .range((object_id, SequenceNumber::MIN)..=(object_id, SequenceNumber::MAX))
                .last()
                .and_then(|(_, opt_object)| opt_object.as_ref())
                .and_then(|object| {
                    // Check if this object is actually a package
                    object.data.try_as_package()
                });

            if let Some(package) = latest_package {
                return Package::read_from_package(package).map(Arc::new);
            }
        }

        // Package not found in execution context, fall back to the underlying store
        self.package_store.fetch(id).await
    }
}
