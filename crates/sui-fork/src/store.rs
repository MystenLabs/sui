// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

use anyhow::anyhow;
use tracing::info;

use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::language_storage::StructTag;
use simulacrum::store::SimulatorStore;
use sui_protocol_config::Chain;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::clock::Clock;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::ObjectDigest;
use sui_types::digests::TransactionDigest;
use sui_types::digests::get_mainnet_chain_identifier;
use sui_types::digests::get_testnet_chain_identifier;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::error::SuiResult;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VersionedFullCheckpointContents;
use sui_types::object::Object;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::BackingStore;
use sui_types::storage::BalanceInfo;
use sui_types::storage::BalanceIterator;
use sui_types::storage::ChildObjectResolver;
use sui_types::storage::CoinInfo;
use sui_types::storage::DynamicFieldIteratorItem;
use sui_types::storage::EpochInfo;
use sui_types::storage::ObjectStore;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::PackageObject;
use sui_types::storage::ParentSync;
use sui_types::storage::ReadStore;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result as StorageResult;
use sui_types::storage::load_package_object_from_object_store;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::VerifiedTransaction;
use typed_store_error::TypedStoreError;

use crate::CheckpointRead;
use crate::GraphQLClient;
use crate::Node;
use crate::ObjectKey;
use crate::ObjectRead;
use crate::TransactionInfo;
use crate::TransactionRead;
use crate::VersionQuery;
use crate::filesystem::FilesystemStore;
use crate::filesystem::OwnedObjectEntry;
use crate::filesystem::RemovedObjectKind;

/// A data store for Sui data, combining a shared local filesystem cache with a remote GraphQL
/// endpoint for historical reads. Pre-fork data is fetched on demand and cached locally; post-fork
/// data (written by the executor) lives on disk only.
///
/// Cloned stores share the same inner state and local snapshot guard, so RPC readers and the local
/// executor coordinate multi-file filesystem snapshots.
///
/// Implements [`SimulatorStore`] so it can be passed directly into
/// [`simulacrum::Simulacrum::new_from_custom_state`].
#[derive(Clone)]
pub struct DataStore {
    inner: Arc<DataStoreInner>,
}

struct DataStoreInner {
    forked_at_checkpoint: CheckpointSequenceNumber,
    gql: GraphQLClient,
    local: FilesystemStore,
    /// Protects multi-file filesystem snapshots between executor writes and cloned RPC readers.
    local_snapshot_lock: RwLock<()>,
}

/// Object reference paired with the current-state removal kind that produced it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RemovedObject {
    object_ref: ObjectRef,
    kind: RemovedObjectKind,
}

impl DataStore {
    /// Create a new `DataStore` for the given network, anchored at `forked_at_checkpoint`.
    ///
    /// The local filesystem cache root is selected by `FilesystemStore`. The GraphQL client is
    /// constructed eagerly but no remote requests are made until reads happen.
    pub async fn new(
        node: Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
        version: &str,
        data_dir: Option<std::path::PathBuf>,
    ) -> Result<Self, anyhow::Error> {
        let gql = GraphQLClient::new(node.clone(), version)?;
        let local = FilesystemStore::new(&node, forked_at_checkpoint, data_dir)?;

        Ok(Self::from_parts(forked_at_checkpoint, gql, local))
    }

    fn from_parts(
        forked_at_checkpoint: CheckpointSequenceNumber,
        gql: GraphQLClient,
        local: FilesystemStore,
    ) -> Self {
        Self {
            inner: Arc::new(DataStoreInner {
                forked_at_checkpoint,
                gql,
                local,
                local_snapshot_lock: RwLock::new(()),
            }),
        }
    }

    pub fn forked_at_checkpoint(&self) -> CheckpointSequenceNumber {
        self.inner.forked_at_checkpoint
    }

    /// Return the chain (mainnet/testnet/devnet/unknown) this store is connected to.
    pub fn chain(&self) -> Chain {
        self.inner.gql.chain()
    }

    fn read_local_snapshot(&self) -> StorageResult<RwLockReadGuard<'_, ()>> {
        self.inner
            .local_snapshot_lock
            .read()
            .map_err(|_| StorageError::custom("local snapshot lock poisoned"))
    }

    fn write_local_snapshot(&self) -> anyhow::Result<RwLockWriteGuard<'_, ()>> {
        self.inner
            .local_snapshot_lock
            .write()
            .map_err(|_| anyhow!("local snapshot lock poisoned"))
    }

    pub(crate) fn gql(&self) -> &GraphQLClient {
        &self.inner.gql
    }

    pub(crate) fn local(&self) -> &FilesystemStore {
        &self.inner.local
    }

    /// Get a checkpoint summary by sequence number. Tries the local filesystem first. If it's a
    /// miss, then fetches it from remote if it's at or before the fork checkpoint and caches it
    /// locally for next time.
    pub(crate) fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        if let Some(checkpoint) = self
            .inner
            .local
            .get_checkpoint_by_sequence_number(sequence)?
        {
            info!("Found checkpoint {sequence} in local filesystem");
            return Ok(Some(checkpoint));
        }
        if sequence > self.inner.forked_at_checkpoint {
            info!(
                "Checkpoint requested for sequence {sequence} > forked_at_checkpoint {}, returning None",
                self.inner.forked_at_checkpoint
            );
            return Ok(None);
        }
        Ok(self
            .fetch_and_cache_checkpoint(sequence)?
            .map(|(checkpoint, _)| checkpoint))
    }

    /// Get checkpoint contents by sequence number, with the same local-first
    /// remote-fallback policy as [`Self::get_checkpoint_by_sequence_number`].
    pub(crate) fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        if let Some(contents) = self
            .inner
            .local
            .get_checkpoint_contents_by_sequence_number(sequence)?
        {
            return Ok(Some(contents));
        }
        if sequence > self.inner.forked_at_checkpoint {
            return Ok(None);
        }
        Ok(self
            .fetch_and_cache_checkpoint(sequence)?
            .map(|(_, contents)| contents))
    }

    /// Look up a checkpoint summary by its digest. Local only: the GraphQL
    /// checkpoint query is keyed by sequence number, so there is no remote
    /// fallback for digest lookups.
    pub(crate) fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        self.inner.local.get_checkpoint_by_digest(digest)
    }

    /// Look up checkpoint contents by their digest. Local only: contents are
    /// content-addressed on disk, but the remote GraphQL schema does not
    /// expose a contents-by-digest query, so there is no fallback path.
    pub(crate) fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        self.inner.local.get_checkpoint_contents_by_digest(digest)
    }

    /// Return the highest checkpoint summary cached locally. This never
    /// consults the remote endpoint — the local executor is the source of
    /// truth for "latest" in a forked network.
    pub(crate) fn get_highest_verified_checkpoint(
        &self,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        self.inner.local.get_highest_verified_checkpoint()
    }

    /// Eagerly populate the cache with the startup (forked-at) checkpoint so
    /// any bootstrap failure surfaces now instead of on first access.
    pub(crate) fn download_and_persist_startup_checkpoint(&self) -> anyhow::Result<()> {
        self.get_checkpoint_by_sequence_number(self.inner.forked_at_checkpoint)?
            .ok_or_else(|| {
                anyhow!(
                    "checkpoint {} not found on remote",
                    self.inner.forked_at_checkpoint
                )
            })?;
        Ok(())
    }

    /// Get the highest checkpoint sequence number available on disk.
    pub(crate) fn get_highest_checkpoint(&self) -> anyhow::Result<CheckpointSequenceNumber> {
        self.inner.local.get_highest_checkpoint_sequence_number()
    }

    /// Query the remote GraphQL endpoint to determine the lowest checkpoint for
    /// which both checkpoint and transaction data are available.
    pub(crate) fn get_lowest_available_checkpoint(
        &self,
    ) -> anyhow::Result<CheckpointSequenceNumber> {
        self.inner.gql.get_lowest_available_checkpoint()
    }

    /// Query the remote GraphQL endpoint to determine the lowest checkpoint for
    /// which object data is available.
    pub(crate) fn get_lowest_available_checkpoint_objects(
        &self,
    ) -> anyhow::Result<CheckpointSequenceNumber> {
        self.inner.gql.get_lowest_available_checkpoint_objects()
    }

    /// Fetch checkpoint summary and contents from the remote GraphQL endpoint and persist them to
    /// disk. Shared by the sequence-keyed cache-aware getters.
    fn fetch_and_cache_checkpoint(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<(VerifiedCheckpoint, CheckpointContents)>> {
        let Some((checkpoint, contents)) = self.inner.gql.get_checkpoint(Some(sequence))? else {
            return Ok(None);
        };
        // Write contents first: they're content-addressed (idempotent), so
        // if the summary write fails afterward the contents are harmless
        // orphans and the next request retries cleanly. The reverse order
        // would leave a summary on disk pointing to missing contents.
        self.inner.local.write_checkpoint_contents(&contents)?;
        self.inner.local.write_checkpoint_summary(&checkpoint)?;
        Ok(Some((checkpoint, contents)))
    }

    /// Get the object at the latest version available on disk. If not found, it will fetch the
    /// object at the forked checkpoint from remote rpc and save it to disk for future use. Returns
    /// `None` in the latter case.
    pub(crate) fn get_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        self.get_latest_object(object_id)
    }

    /// Get the object at the specified version. It will first try to load from disk, and if not
    /// found, it will fetch from remote rpc by making a query to fetch this version at the forked
    /// checkpoint. If none is found, it will return None. If the object is successfully fetched
    /// from remote rpc, it will be saved to disk for future use before returning the object.
    pub(crate) fn get_object_at_version(
        &self,
        object_id: &ObjectID,
        version: u64,
    ) -> anyhow::Result<Option<Object>> {
        if let Some(object) = self.inner.local.get_object_at_version(object_id, version)? {
            return Ok(Some(object));
        }

        let object =
            self.get_object_from_remote(object_id, Some(version), self.forked_at_checkpoint())?;

        if let Some(ref object) = object {
            let _local_snapshot_guard = self.write_local_snapshot()?;
            self.inner.local.write_object(object)?;
        }

        Ok(object)
    }

    /// Local-first lookup for the latest known version of an object. Falls back to a remote
    /// `AtCheckpoint(forked_at_checkpoint)` query and caches the result on disk.
    fn get_latest_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        if self.inner.local.is_object_currently_removed(object_id)? {
            return Ok(None);
        }

        if let Some(object) = self.inner.local.get_latest_object(object_id)? {
            return Ok(Some(object));
        }

        // if not found, load from remote rpc at forked checkpoint and save it to disk for future
        // use
        let object = self.get_object_from_remote(object_id, None, self.forked_at_checkpoint())?;

        if let Some(ref object) = object {
            let _local_snapshot_guard = self.write_local_snapshot()?;
            self.inner.local.write_object(object)?;
        }

        Ok(object)
    }

    /// Get the object at the specified checkpoint from remote rpc. If version is `None`, latest
    /// version at that checkpoint will be returned. Otherwise, the object at the specified version
    /// will be returned if it existed at that checkpoint.
    fn get_object_from_remote(
        &self,
        object_id: &ObjectID,
        version: Option<u64>,
        checkpoint: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<Object>> {
        let version_query = if let Some(version) = version {
            VersionQuery::VersionAtCheckpoint {
                version,
                checkpoint,
            }
        } else {
            VersionQuery::AtCheckpoint(checkpoint)
        };

        let objects = self.inner.gql.get_objects(&[ObjectKey {
            object_id: *object_id,
            version_query,
        }])?;

        Ok(objects
            .into_iter()
            .next()
            .flatten()
            .map(|(object, _)| object))
    }

    /// Get a signed transaction by digest. First tries the local filesystem, and on miss it falls
    /// back to the remote GraphQL endpoint. If the transaction is found remotely and is at or
    /// before the fork checkpoint, then it is saved to disk before being returned. Transactions
    /// produced by the local executor are always on disk.
    ///
    /// Note that currently historical reads do not include events, whereas local execution does
    /// write events to disk.
    pub(crate) fn get_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<Option<VerifiedTransaction>> {
        if let Some(transaction) = self.inner.local.get_transaction(digest)? {
            return Ok(Some(transaction));
        }
        Ok(self
            .fetch_and_cache_transaction(digest)?
            .map(|info| info.transaction))
    }

    /// Get the checkpoint that finalized a transaction. Local-only: the checkpoint
    /// file is written alongside the transaction by both the remote-fetch path
    /// and the post-fork executor path.
    pub(crate) fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<Option<CheckpointSequenceNumber>> {
        if let Some(seq) = self.inner.local.get_transaction_checkpoint(digest)? {
            return Ok(Some(seq));
        }
        // If the checkpoint file is missing but the transaction itself hasn't
        // been fetched yet, try fetching it — that will also write the
        // checkpoint file as a side-effect.
        Ok(self
            .fetch_and_cache_transaction(digest)?
            .map(|info| info.checkpoint))
    }

    /// Get transaction effects by digest, with the same local-first remote-fallback
    /// policy as [`Self::get_transaction`].
    pub(crate) fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<Option<TransactionEffects>> {
        if let Some(effects) = self.inner.local.get_transaction_effects(digest)? {
            return Ok(Some(effects));
        }
        Ok(self
            .fetch_and_cache_transaction(digest)?
            .map(|info| info.effects))
    }

    /// Fetch a transaction and its effects from the remote GraphQL endpoint and persist both halves
    /// to disk. Shared by [`Self::get_transaction`] and [`Self::get_transaction_effects`] so a
    /// single remote round-trip is used.
    ///
    /// Pre-fork guard: transaction digests aren't ordered, so we can't reject post-fork requests
    /// up front the way [`Self::get_checkpoint_by_sequence_number`] does. Instead we check
    /// `info.checkpoint` on the remote response and drop anything executed strictly after
    /// `forked_at_checkpoint` so our fork doesn't silently absorb upstream activity that
    /// happened after the fork point.
    fn fetch_and_cache_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<Option<TransactionInfo>> {
        let Some(info) = self
            .inner
            .gql
            .transaction_data_and_effects(&digest.base58_encode())?
        else {
            return Ok(None);
        };
        if info.checkpoint > self.inner.forked_at_checkpoint {
            return Ok(None);
        }
        self.inner
            .local
            .write_transaction(digest, &info.transaction)?;
        self.inner
            .local
            .write_transaction_effects(digest, &info.effects)?;
        self.inner
            .local
            .write_transaction_checkpoint(digest, info.checkpoint)?;

        // Fetch and persist events separately — they require paginated queries.
        // Best-effort: if the events fetch fails we still want the transaction
        // and effects cached, so log the error and fall back to empty events.
        let events = match self
            .inner
            .gql
            .get_transaction_events(&digest.base58_encode())
        {
            Ok(Some(events)) => events,
            Ok(None) => TransactionEvents::default(),
            Err(err) => {
                tracing::warn!(
                    %digest,
                    "failed to fetch transaction events, storing empty: {err:#}",
                );
                TransactionEvents::default()
            }
        };
        self.inner.local.write_transaction_events(digest, &events)?;

        Ok(Some(info))
    }

    /// Look up the checkpoint sequence number that references the given contents
    /// digest by scanning the highest persisted checkpoint. Called from
    /// `insert_checkpoint_contents` to build the tx→checkpoint reverse mapping.
    fn checkpoint_sequence_for_contents(
        &self,
        contents_digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointSequenceNumber> {
        // The summary persisted by the immediately preceding `insert_checkpoint`
        // call is typically the highest checkpoint. Read it back and verify the
        // content_digest matches.
        let checkpoint = self.inner.local.get_highest_verified_checkpoint().ok()??;
        if checkpoint.data().content_digest == *contents_digest {
            return Some(checkpoint.data().sequence_number);
        }
        None
    }

    /// Persist local object writes and current-state tombstones, then update the address-owned
    /// index from the same diff.
    fn apply_object_updates(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        removed_objects: Vec<RemovedObject>,
    ) {
        let _local_snapshot_guard = self
            .write_local_snapshot()
            .expect("failed to lock local snapshot for object update");
        let removed_object_ids: Vec<_> = removed_objects
            .iter()
            .map(|removed| removed.object_ref.0)
            .collect();

        for removed in &removed_objects {
            match removed.kind {
                RemovedObjectKind::Deleted => self
                    .inner
                    .local
                    .mark_object_deleted(&removed.object_ref)
                    .expect("failed to mark object deleted on disk"),
                RemovedObjectKind::Wrapped => self
                    .inner
                    .local
                    .mark_object_wrapped(&removed.object_ref)
                    .expect("failed to mark object wrapped on disk"),
            }
        }

        for object in written_objects.values() {
            self.inner
                .local
                .write_object(object)
                .expect("failed to write object to disk");
            self.inner
                .local
                .clear_object_wrapped(&object.id())
                .expect("failed to clear object wrapped marker");
        }

        let indexable_written_objects: Vec<_> = written_objects
            .values()
            .filter(|object| {
                !self
                    .inner
                    .local
                    .is_object_deleted(&object.id())
                    .expect("failed to read object removal marker")
            })
            .collect();

        self.inner
            .local
            .apply_owned_object_index_updates(&removed_object_ids, indexable_written_objects)
            .expect("failed to update owned-object index");
    }

    /// Construct a `DataStore` for tests, backed by an explicit local root and a fake (unused)
    /// GraphQL endpoint. The remote client is constructed but never called because tests should
    /// pre-populate the local cache with the data they need.
    #[cfg(test)]
    pub(crate) fn new_for_testing(root: std::path::PathBuf) -> Self {
        let gql = GraphQLClient::new(Node::Custom("http://localhost:1".to_string()), "test")
            .expect("graphql store with localhost url should construct");
        let local = FilesystemStore::new_with_root(root);
        Self::from_parts(0, gql, local)
    }

    /// Test-only constructor that lets callers point the GraphQL client at an arbitrary URL
    /// (e.g., a wiremock `MockServer`) and pin `forked_at_checkpoint` explicitly.
    #[cfg(test)]
    pub(crate) fn new_for_testing_with_remote(
        root: std::path::PathBuf,
        gql_url: String,
        forked_at_checkpoint: CheckpointSequenceNumber,
    ) -> Self {
        let gql = GraphQLClient::new(Node::Custom(gql_url), "test")
            .expect("graphql store with custom url should construct");
        let local = FilesystemStore::new_with_root(root);
        Self::from_parts(forked_at_checkpoint, gql, local)
    }

    /// Get owned objects for an address, optionally filtered by object type and paginated with a
    /// cursor.
    fn get_owned_objects(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Vec<OwnedObjectInfo>> {
        let _local_snapshot_guard = self.read_local_snapshot()?;
        self.get_owned_objects_unlocked(owner, object_type, cursor)
    }

    /// Get owned objects while the caller holds a local snapshot guard.
    fn get_owned_objects_unlocked(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Vec<OwnedObjectInfo>> {
        let entries = self
            .inner
            .local
            .get_owned_object_entries()
            .map_err(|e| StorageError::custom(e.to_string()))?;
        let cursor_object_id = cursor.map(|cursor| cursor.object_id);

        Ok(entries
            .into_iter()
            .filter(|entry| entry.owner == owner)
            .filter(|entry| {
                object_type
                    .as_ref()
                    .is_none_or(|ty| struct_tag_filter_matches(ty, &entry.object_type))
            })
            // `RpcIndexes` cursors are lower bounds. The v2 RPC layer stores
            // the first not-yet-returned item in the page token and expects
            // the next iterator to include it.
            .filter(|entry| cursor_object_id.is_none_or(|id| entry.object_id >= id))
            .filter_map(|entry| self.valid_owned_object_info(entry))
            .collect())
    }

    /// Validate that the given `OwnedObjectEntry` corresponds to an actual owned object in the
    /// local store, and if so convert it to `OwnedObjectInfo`. This guards against stale index
    /// entries that point to objects that have been deleted or wrapped by later transactions.
    fn valid_owned_object_info(&self, entry: OwnedObjectEntry) -> Option<OwnedObjectInfo> {
        if let Some(object) = self.local().get_latest_object(&entry.object_id).ok()? {
            if object.version() != entry.version {
                return None;
            }
            if object.owner != sui_types::object::Owner::AddressOwner(entry.owner) {
                return None;
            }
            let object_type = object.struct_tag()?;
            if object_type != entry.object_type {
                return None;
            }
        }

        Some(OwnedObjectInfo {
            owner: entry.owner,
            object_type: entry.object_type,
            balance: entry.balance,
            object_id: entry.object_id,
            version: entry.version,
        })
    }
}

/// Check if the these two `StructTag`s match for the purposes of owned-object filtering. The filter
/// may have empty type parameters, in which case they are ignored and only the address, module, and
/// name are compared.
///
/// This allows a wildcard filter like `0x2::coin::Coin` to match all versions of the `Coin` struct,
/// regardless of the type parameter (e.g., `0x2::coin::Coin<0x1::sui::SUI>`).
fn struct_tag_filter_matches(filter: &StructTag, candidate: &StructTag) -> bool {
    filter.address == candidate.address
        && filter.module.as_ident_str() == candidate.module.as_ident_str()
        && filter.name.as_ident_str() == candidate.name.as_ident_str()
        && (filter.type_params.is_empty()
            || filter.type_params.as_slice() == candidate.type_params.as_slice())
}

/// Extract removal kinds before passing removals through `update_objects`, whose trait signature
/// does not distinguish deleted, wrapped, or unwrapped-then-deleted objects.
fn removed_objects_from_effects(effects: &TransactionEffects) -> Vec<RemovedObject> {
    effects
        .deleted()
        .into_iter()
        .chain(effects.unwrapped_then_deleted())
        .map(|object_ref| RemovedObject {
            object_ref,
            kind: RemovedObjectKind::Deleted,
        })
        .chain(
            effects
                .wrapped()
                .into_iter()
                .map(|object_ref| RemovedObject {
                    object_ref,
                    kind: RemovedObjectKind::Wrapped,
                }),
        )
        .collect()
}

// ============================================================================
// SimulatorStore super-traits
// ============================================================================

/// Object reads delegate to the inherent `DataStore::get_object` / `get_object_at_version`,
/// which provide local-first lookups with remote fallback. Errors are swallowed and surfaced
/// as `None` because the trait signature does not allow propagating them.
impl ObjectStore for DataStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.get_object(object_id).ok().flatten()
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.get_object_at_version(object_id, version.value())
            .ok()
            .flatten()
    }
}

/// Package reads go through the standard `load_package_object_from_object_store` helper, which
/// validates that the resolved object is actually a Move package.
impl BackingPackageStore for DataStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

/// `ParentSync` is only required by older protocol versions and is never called by the executor
/// for the protocol versions we target. Calling it indicates a misconfiguration.
impl ParentSync for DataStore {
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        panic!("Never called in newer protocol versions")
    }
}

impl ChildObjectResolver for DataStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let child_object = match self.get_object(child).ok().flatten() {
            None => return Ok(None),
            Some(obj) => obj,
        };

        if child_object.owner != sui_types::object::Owner::ObjectOwner((*parent).into()) {
            return Err(sui_types::error::SuiErrorKind::InvalidChildObjectAccess {
                object: *child,
                given_parent: *parent,
                actual_owner: child_object.owner.clone(),
            }
            .into());
        }

        if child_object.version() > child_version_upper_bound {
            return Err(sui_types::error::SuiErrorKind::UnsupportedFeatureError {
                error: "DataStore::read_child_object does not yet support bounded reads".to_owned(),
            }
            .into());
        }

        Ok(Some(child_object))
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        let Some(recv_object) = self.get_object(receiving_object_id).ok().flatten() else {
            return Ok(None);
        };
        if recv_object.owner != sui_types::object::Owner::AddressOwner((*owner).into()) {
            return Ok(None);
        }
        if recv_object.version() != receive_object_at_version {
            return Ok(None);
        }
        Ok(Some(recv_object))
    }
}

// ============================================================================
// SimulatorStore
// ============================================================================

impl SimulatorStore for DataStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        DataStore::get_checkpoint_by_sequence_number(self, sequence_number)
            .ok()
            .flatten()
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        DataStore::get_checkpoint_by_digest(self, digest)
            .ok()
            .flatten()
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        DataStore::get_highest_verified_checkpoint(self)
            .ok()
            .flatten()
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        DataStore::get_checkpoint_contents_by_digest(self, digest)
            .ok()
            .flatten()
    }

    fn get_committee_by_epoch(&self, _epoch: EpochId) -> Option<Committee> {
        todo!("SimulatorStore::get_committee_by_epoch")
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        DataStore::get_transaction(self, digest).ok().flatten()
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        DataStore::get_transaction_effects(self, digest)
            .ok()
            .flatten()
    }

    fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.inner
            .local
            .get_transaction_events(digest)
            .ok()
            .flatten()
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        self.get_object(id).ok().flatten()
    }

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.get_object_at_version(id, version.value())
            .ok()
            .flatten()
    }

    fn get_system_state(&self) -> SuiSystemState {
        sui_types::sui_system_state::get_sui_system_state(self).expect("system state must exist")
    }

    fn get_clock(&self) -> Clock {
        self.get_object(&sui_types::SUI_CLOCK_OBJECT_ID)
            .ok()
            .flatten()
            .expect("clock should exist")
            .to_rust()
            .expect("clock object should deserialize")
    }

    fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        let objects = match self
            .read_local_snapshot()
            .and_then(|_local_snapshot_guard| {
                self.get_owned_objects_unlocked(owner, None, None)
                    .map(|infos| {
                        infos
                            .into_iter()
                            .filter_map(|info| {
                                self.inner
                                    .local
                                    .get_latest_object(&info.object_id)
                                    .ok()
                                    .flatten()
                                    .filter(|object| object.version() == info.version)
                            })
                            .collect()
                    })
            }) {
            Ok(objects) => objects,
            Err(err) => {
                tracing::error!(%owner, "failed to read owned-object index: {err:?}");
                Vec::new()
            }
        };
        Box::new(objects.into_iter())
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        let sequence = checkpoint.data().sequence_number;
        // Pre-fork summary was persisted at seed time; skip rewrites.
        if self
            .inner
            .local
            .get_checkpoint_by_sequence_number(sequence)
            .ok()
            .flatten()
            .is_some()
        {
            return;
        }
        if let Err(err) = self.inner.local.write_checkpoint_summary(&checkpoint) {
            tracing::error!(
                sequence_number = sequence,
                "failed to persist checkpoint summary: {err:?}",
            );
        }
    }

    fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        // Contents are content-addressed, so writes are independent of the
        // summary that references them. Idempotent: re-writing the same
        // digest is a no-op.
        let digest = *contents.digest();
        if self
            .inner
            .local
            .get_checkpoint_contents_by_digest(&digest)
            .ok()
            .flatten()
            .is_some()
        {
            return;
        }
        if let Err(err) = self.inner.local.write_checkpoint_contents(&contents) {
            tracing::error!(
                contents_digest = %digest,
                "failed to persist checkpoint contents: {err:?}",
            );
        }

        // Build the tx_digest → checkpoint reverse mapping. The summary
        // (persisted by the preceding `insert_checkpoint` call) carries the
        // sequence number we need.
        if let Some(sequence) = self.checkpoint_sequence_for_contents(&digest) {
            for exec_digest in contents.iter() {
                if let Err(err) = self
                    .inner
                    .local
                    .write_transaction_checkpoint(&exec_digest.transaction, sequence)
                {
                    tracing::error!(
                        tx_digest = %exec_digest.transaction,
                        "failed to persist transaction checkpoint: {err:?}",
                    );
                }
            }
        }
    }

    fn insert_committee(&mut self, _committee: Committee) {
        todo!("SimulatorStore::insert_committee")
    }

    fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        let removed_objects = removed_objects_from_effects(&effects);
        let tx_digest = *effects.transaction_digest();
        self.insert_transaction(transaction);
        self.insert_transaction_effects(effects);
        self.insert_events(&tx_digest, events);
        self.apply_object_updates(written_objects, removed_objects);
    }

    fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        let digest = *transaction.digest();
        self.inner
            .local
            .write_transaction(&digest, &transaction)
            .expect("failed to persist transaction to disk");
    }

    fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        let digest = *effects.transaction_digest();
        self.inner
            .local
            .write_transaction_effects(&digest, &effects)
            .expect("failed to persist transaction effects to disk");
    }

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        self.inner
            .local
            .write_transaction_events(tx_digest, &events)
            .expect("failed to persist transaction events to disk");
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        let removed_objects = deleted_objects
            .into_iter()
            .map(|object_ref| RemovedObject {
                object_ref,
                kind: RemovedObjectKind::Deleted,
            })
            .collect();
        self.apply_object_updates(written_objects, removed_objects);
    }

    fn backing_store(&self) -> &dyn BackingStore {
        self
    }
}

// ============================================================================
// ReadStore / RpcStateReader
// ============================================================================

impl ReadStore for DataStore {
    fn get_committee(&self, _epoch: sui_types::committee::EpochId) -> Option<Arc<Committee>> {
        todo!("ReadStore::get_committee on forked DataStore")
    }

    fn get_latest_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        self.get_highest_verified_checkpoint()
            .map_err(|e| StorageError::custom(e.to_string()))?
            .ok_or_else(|| StorageError::missing("no checkpoint persisted yet"))
    }

    fn get_highest_verified_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        DataStore::get_highest_verified_checkpoint(self)
            .map_err(|e| StorageError::custom(e.to_string()))?
            .ok_or_else(|| StorageError::missing("no checkpoint persisted yet"))
    }

    fn get_highest_synced_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        // A fork has no concept of an "unsynced" checkpoint — anything we hold
        // locally was either pre-fetched at startup or produced by the local
        // executor, so highest-synced collapses to highest-verified.
        DataStore::get_highest_verified_checkpoint(self)
            .map_err(|e| StorageError::custom(e.to_string()))?
            .ok_or_else(|| {
                StorageError::missing(
                    "no checkpoint persisted yet — cannot determine highest synced checkpoint",
                )
            })
    }

    /// This will be called for most requests to correctly fetch the earliest checkpoint at which
    /// transactions and checkpoint data are available. The GraphQL endpoint is the source of truth
    /// for this.
    fn get_lowest_available_checkpoint(&self) -> StorageResult<CheckpointSequenceNumber> {
        DataStore::get_lowest_available_checkpoint(self)
            .map_err(|e| StorageError::custom(e.to_string()))
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        DataStore::get_checkpoint_by_digest(self, digest)
            .ok()
            .flatten()
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        info!("Requested checkpoint {} through gRPC", sequence_number);
        DataStore::get_checkpoint_by_sequence_number(self, sequence_number)
            .ok()
            .flatten()
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        DataStore::get_checkpoint_contents_by_digest(self, digest)
            .ok()
            .flatten()
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        DataStore::get_checkpoint_contents_by_sequence_number(self, sequence_number)
            .ok()
            .flatten()
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        SimulatorStore::get_transaction(self, tx_digest).map(Arc::new)
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        SimulatorStore::get_transaction_effects(self, tx_digest)
    }

    fn get_events(&self, tx_digest: &TransactionDigest) -> Option<TransactionEvents> {
        SimulatorStore::get_transaction_events(self, tx_digest)
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        _digest: &TransactionDigest,
    ) -> Option<Vec<sui_types::storage::ObjectKey>> {
        None
    }

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        DataStore::get_transaction_checkpoint(self, digest)
            .ok()
            .flatten()
    }

    fn get_full_checkpoint_contents(
        &self,
        _sequence_number: Option<CheckpointSequenceNumber>,
        _digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        todo!("ReadStore::get_full_checkpoint_contents on forked DataStore")
    }
}

impl RpcStateReader for DataStore {
    fn get_lowest_available_checkpoint_objects(&self) -> StorageResult<CheckpointSequenceNumber> {
        DataStore::get_lowest_available_checkpoint_objects(self)
            .map_err(|e| StorageError::custom(e.to_string()))
    }

    fn get_chain_identifier(&self) -> StorageResult<ChainIdentifier> {
        // Map concrete `Chain` enum onto the canonical chain identifier so
        // clients see this fork as the network it's based on. Devnet/custom
        // forks fall back to the forked checkpoint's digest because those
        // chains don't have a stable on-disk identifier.
        let id = match self.chain() {
            Chain::Mainnet => get_mainnet_chain_identifier(),
            Chain::Testnet => get_testnet_chain_identifier(),
            Chain::Unknown => {
                let checkpoint =
                    ReadStore::get_checkpoint_by_sequence_number(self, self.forked_at_checkpoint())
                        .ok_or_else(|| {
                            StorageError::missing(
                                "forked checkpoint missing — cannot derive chain identifier",
                            )
                        })?;
                ChainIdentifier::from(*checkpoint.digest())
            }
        };
        Ok(id)
    }

    fn indexes(&self) -> Option<&dyn sui_types::storage::RpcIndexes> {
        Some(self)
    }

    fn get_struct_layout_with_overlay(
        &self,
        _struct_tag: &StructTag,
        _overlay: &ObjectSet,
    ) -> StorageResult<Option<MoveTypeLayout>> {
        Ok(None)
    }
}

impl RpcIndexes for DataStore {
    fn get_epoch_info(&self, _epoch: EpochId) -> StorageResult<Option<EpochInfo>> {
        // TODO: For now, we don't really need it. To be added later
        StorageResult::Ok(None)
    }

    fn owned_objects_iter(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Box<dyn Iterator<Item = Result<OwnedObjectInfo, TypedStoreError>> + '_>>
    {
        let infos = self.get_owned_objects(owner, object_type, cursor)?;
        Ok(Box::new(
            infos
                .into_iter()
                .map(Ok::<OwnedObjectInfo, TypedStoreError>),
        ))
    }

    fn dynamic_field_iter(
        &self,
        _parent: ObjectID,
        _cursor: Option<ObjectID>,
    ) -> StorageResult<Box<dyn Iterator<Item = DynamicFieldIteratorItem> + '_>> {
        todo!("not supported yet")
    }

    fn get_coin_info(&self, _coin_type: &StructTag) -> StorageResult<Option<CoinInfo>> {
        todo!("not supported yet")
    }

    fn get_balance(
        &self,
        _owner: &SuiAddress,
        _coin_type: &StructTag,
    ) -> StorageResult<Option<BalanceInfo>> {
        todo!("not supported yet")
    }

    fn balance_iter(
        &self,
        _owner: &SuiAddress,
        _cursor: Option<(SuiAddress, StructTag)>,
    ) -> StorageResult<BalanceIterator<'_>> {
        todo!("not supported yet")
    }

    fn package_versions_iter(
        &self,
        _original_id: ObjectID,
        _cursor: Option<u64>,
    ) -> StorageResult<Box<dyn Iterator<Item = Result<(u64, ObjectID), TypedStoreError>> + '_>>
    {
        todo!("not supported yet")
    }

    fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> StorageResult<Option<CheckpointSequenceNumber>> {
        Ok(self.get_highest_checkpoint().ok())
    }

    fn authenticated_event_iter(
        &self,
        _stream_id: SuiAddress,
        _start_checkpoint: u64,
        _start_accumulator_version: Option<u64>,
        _start_transaction_idx: Option<u32>,
        _start_event_idx: Option<u32>,
        _end_checkpoint: u64,
        _limit: u32,
    ) -> StorageResult<
        Box<
            dyn Iterator<
                    Item = Result<(u64, u64, u32, u32, sui_types::event::Event), TypedStoreError>,
                > + '_,
        >,
    > {
        todo!("not supported yet")
    }
}

#[cfg(test)]
#[path = "tests/store_checkpoint_persistence.rs"]
mod checkpoint_persistence_tests;

#[cfg(test)]
#[path = "tests/store_execution.rs"]
mod execution_tests;

#[cfg(test)]
#[path = "tests/store_transaction_fallback.rs"]
mod transaction_fallback_tests;
