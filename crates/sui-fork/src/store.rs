// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockWriteGuard;

use anyhow::Context as _;
use anyhow::anyhow;
use anyhow::bail;
use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::language_storage::StructTag;
use simulacrum::store::SimulatorStore;
use sui_protocol_config::Chain;
use sui_rpc_store::RpcStoreReader;
use sui_rpc_store::schema::objects::Status;
use sui_rpc_store::schema::objects::TombstoneKind;
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
use sui_types::storage::CoinInfo;
use sui_types::storage::DynamicFieldIteratorItem;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::EpochInfo;
use sui_types::storage::LedgerBitmapBucketIterator;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::LedgerTxSeqDigestIterator;
use sui_types::storage::ObjectKey;
use sui_types::storage::ObjectStore;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::PackageObject;
use sui_types::storage::ParentSync;
use sui_types::storage::ReadStore;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;
use sui_types::storage::RuntimeObjectResolver;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Kind as StorageErrorKind;
use sui_types::storage::error::Result as StorageResult;
use sui_types::storage::load_package_object_from_object_store;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::VerifiedTransaction;
use typed_store_error::TypedStoreError;

use crate::GraphQLClient;
use crate::TransactionInfo;
use crate::inventory::InventoryInitializer;
use crate::local_store::LocalStore;
use crate::local_store::ObjectRemoval;
use crate::metadata::MetadataStore;
use crate::pending::PendingCheckpointBuffer;
use crate::remote::RemoteSource;

/// The fork's state store: reads check `LocalStore` first and fall back to
/// `RemoteSource` (pinned at the fork checkpoint), persisting fetched
/// pre-fork data back into the local store. The metadata sidecar keeps fork
/// metadata and completion markers for remote inventory scans.
///
/// Cloned stores share the same inner state and local snapshot guard, so RPC readers and the local
/// executor coordinate index initialization.
///
/// Implements [`SimulatorStore`] so it can be passed directly into
/// [`simulacrum::Simulacrum::new_from_custom_state`], and the upstream RPC
/// storage traits (`ReadStore`, `RpcStateReader`, `RpcIndexes`) so the same
/// store backs the `sui-rpc-api` `RpcService`.
#[derive(Clone)]
pub struct ForkStore {
    inner: Arc<ForkStoreInner>,
}

struct ForkStoreInner {
    forked_at_checkpoint: CheckpointSequenceNumber,
    /// Checkpoint-pinned GraphQL access to the forked-from chain; owns all
    /// pre/post-fork remote-read policy.
    remote: RemoteSource,
    metadata: MetadataStore,
    local_store: LocalStore,
    /// Lazy full-enumeration initializer for the owner/type indexes.
    inventory: InventoryInitializer,
    /// Staging for the in-flight checkpoint; see [`PendingCheckpointBuffer`].
    pending: PendingCheckpointBuffer,
    /// Coordinates index initialization and local object writes across cloned
    /// stores; the same lock is shared with `inventory`.
    local_snapshot_lock: Arc<RwLock<()>>,
}

impl ForkStore {
    pub(crate) fn from_parts(
        forked_at_checkpoint: CheckpointSequenceNumber,
        gql: GraphQLClient,
        metadata: MetadataStore,
        local_store: LocalStore,
    ) -> Self {
        let remote = RemoteSource::new(gql, forked_at_checkpoint);
        let local_snapshot_lock = Arc::new(RwLock::new(()));
        let inventory = InventoryInitializer::new(
            remote.clone(),
            metadata.clone(),
            local_store.clone(),
            local_snapshot_lock.clone(),
        );
        Self {
            inner: Arc::new(ForkStoreInner {
                forked_at_checkpoint,
                remote,
                metadata,
                local_store,
                inventory,
                pending: PendingCheckpointBuffer::new(),
                local_snapshot_lock,
            }),
        }
    }

    pub fn forked_at_checkpoint(&self) -> CheckpointSequenceNumber {
        self.inner.forked_at_checkpoint
    }

    /// Return the chain (mainnet/testnet/devnet/unknown) this store is connected to.
    pub fn chain(&self) -> Chain {
        self.inner.remote.gql().chain()
    }

    fn write_local_snapshot(&self) -> anyhow::Result<RwLockWriteGuard<'_, ()>> {
        self.inner
            .local_snapshot_lock
            .write()
            .map_err(|_| anyhow!("local snapshot lock poisoned"))
    }

    pub(crate) fn gql(&self) -> &GraphQLClient {
        self.inner.remote.gql()
    }

    pub(crate) fn metadata(&self) -> &MetadataStore {
        &self.inner.metadata
    }

    pub(crate) fn local_store(&self) -> &LocalStore {
        &self.inner.local_store
    }

    /// Get a checkpoint summary by sequence number. The RPC store is the
    /// durable history store; pre-fork misses are fetched from GraphQL and
    /// persisted there.
    pub(crate) fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        let local_store = self.local_store();
        let reader = local_store.reader();
        if let Some(checkpoint) = ReadStore::get_checkpoint_by_sequence_number(reader, sequence) {
            return Ok(Some(checkpoint));
        }
        Ok(self
            .fetch_and_save_checkpoint(sequence)?
            .map(|(checkpoint, _)| checkpoint))
    }

    /// Get checkpoint contents by sequence number, with the same rpc-store
    /// remote-fallback policy as [`Self::get_checkpoint_by_sequence_number`].
    pub(crate) fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        let local_store = self.local_store();
        let reader = local_store.reader();
        if let Some(contents) =
            ReadStore::get_checkpoint_contents_by_sequence_number(reader, sequence)
        {
            return Ok(Some(contents));
        }
        Ok(self
            .fetch_and_save_checkpoint(sequence)?
            .map(|(_, contents)| contents))
    }

    /// Look up a checkpoint summary by its digest. RPC-store only: the
    /// GraphQL checkpoint query is keyed by sequence number, so there is no
    /// remote fallback for digest lookups.
    pub(crate) fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        let local_store = self.local_store();
        Ok(ReadStore::get_checkpoint_by_digest(
            local_store.reader(),
            digest,
        ))
    }

    /// Look up checkpoint contents by their digest from the RPC store.
    pub(crate) fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        self.local_store().get_checkpoint_contents_by_digest(digest)
    }

    /// Return the highest checkpoint summary persisted in the RPC store. This never
    /// consults the remote endpoint — the local executor is the source of
    /// truth for "latest" in a forked network.
    ///
    /// Only a `Missing` read maps to `Ok(None)`; other storage errors
    /// propagate so a store failure cannot masquerade as "no checkpoint yet".
    pub(crate) fn get_highest_verified_checkpoint(
        &self,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        let reader = self.local_store().reader();
        match ReadStore::get_highest_verified_checkpoint(reader) {
            Ok(checkpoint) => Ok(Some(checkpoint)),
            Err(err) if err.kind() == StorageErrorKind::Missing => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Get the highest checkpoint sequence number available in the RPC store.
    pub(crate) fn get_highest_checkpoint(&self) -> anyhow::Result<CheckpointSequenceNumber> {
        self.local_store()
            .highest_checkpoint_sequence()?
            .ok_or_else(|| anyhow!("no checkpoint persisted yet"))
    }

    /// Query the remote GraphQL endpoint to determine the lowest checkpoint for
    /// which both checkpoint and transaction data are available.
    pub(crate) fn get_lowest_available_checkpoint(
        &self,
    ) -> anyhow::Result<CheckpointSequenceNumber> {
        self.inner.remote.lowest_available_checkpoint()
    }

    /// Query the remote GraphQL endpoint to determine the lowest checkpoint for
    /// which object data is available.
    pub(crate) fn get_lowest_available_checkpoint_objects(
        &self,
    ) -> anyhow::Result<CheckpointSequenceNumber> {
        self.inner.remote.lowest_available_checkpoint_objects()
    }

    fn fetch_and_save_checkpoint(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<(VerifiedCheckpoint, CheckpointContents)>> {
        let Some((checkpoint, contents)) = self.inner.remote.checkpoint(sequence)? else {
            return Ok(None);
        };
        self.save_checkpoint(&checkpoint, &contents)?;
        Ok(Some((checkpoint, contents)))
    }

    pub(crate) fn save_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: &CheckpointContents,
    ) -> anyhow::Result<()> {
        self.local_store().save_checkpoint(checkpoint, contents)
    }

    /// Get the latest known object. If not found locally, fetch the object at the forked checkpoint
    /// from remote GraphQL and persist it for future use.
    pub(crate) fn get_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        self.get_latest_object(object_id)
    }

    /// Get the object at the specified version. It first tries local saved state and falls
    /// back to a checkpoint-scoped remote query. Successfully fetched objects are persisted
    /// locally before being returned.
    pub(crate) fn get_object_at_version(
        &self,
        object_id: &ObjectID,
        version: u64,
    ) -> anyhow::Result<Option<Object>> {
        let local_store = self.local_store();
        let sequence = SequenceNumber::from_u64(version);
        match local_store.get_object_at_version(*object_id, sequence)? {
            Some(Status::Live(object)) => return Ok(Some(object)),
            Some(Status::Tombstone(_)) => return Ok(None),
            None => {}
        }

        let object = self.inner.remote.object_at_version(object_id, version)?;
        if let Some(ref object) = object {
            local_store.save_object_version_only(object)?;
        }

        Ok(object)
    }

    /// Get the latest object version at or below the given root version.
    ///
    /// TODO(fork): this trusts the highest *local* row at or below the bound,
    /// which is only the true highest-≤-bound if the local history is complete
    /// below the bound. A sparse cache polluted by an exact-historical-version
    /// read (e.g. an RPC client fetching an old dynamic-field version) can hold
    /// a lower live row than the chain's true highest-≤-bound; that stale row
    /// then wins here without the remote ever being consulted. Reachable from
    /// `read_child_object` on both the RPC and the executor paths, so it can
    /// skew execution, not just reads. Candidate fix (undecided): short-circuit
    /// only on live-state authority (`Live(v)` with `v <= bound`) or on an
    /// authoritative tombstone; otherwise query the remote `RootVersion(bound)`
    /// and take the max-version of the remote result and the local candidate.
    /// Optionally let executor-driven bounds (always the parent's current root
    /// version, which Lamport-dominates the child) set the live pointer on
    /// fetch so each child pays the remote round-trip once. Tracked in
    /// design/storage.md § "Known gaps".
    fn get_object_lt_or_eq_version(
        &self,
        object_id: &ObjectID,
        version_bound: SequenceNumber,
    ) -> anyhow::Result<Option<Object>> {
        let local_store = self.local_store();
        match local_store.get_object_at_or_before(*object_id, version_bound)? {
            Some((_, Status::Live(object))) => return Ok(Some(object)),
            Some((_, Status::Tombstone(_))) => return Ok(None),
            None => {}
        }

        let object = self
            .inner
            .remote
            .object_at_or_before(object_id, version_bound.value())?;
        if let Some(ref object) = object {
            local_store.save_object_version_only(object)?;
        }

        Ok(object)
    }

    /// Local-first lookup for the latest known version of an object. Falls back to a remote
    /// `AtCheckpoint(forked_at_checkpoint)` query and persists the result in the RPC store.
    fn get_latest_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        let local_store = self.local_store();
        match local_store.get_latest_object_status(*object_id)? {
            Some((_, Status::Live(object))) => return Ok(Some(object)),
            Some((_, Status::Tombstone(_))) => return Ok(None),
            None => {}
        }

        let object = self.inner.remote.latest_object(object_id)?;
        if let Some(ref object) = object {
            local_store.save_live_object_if_current(object)?;
        }

        Ok(object)
    }

    pub(crate) fn read_child_object_fallible(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let Some(child_object) = self
            .get_object_lt_or_eq_version(child, child_version_upper_bound)
            .map_err(|err| format!("failed to read child object {child}: {err:#}"))?
        else {
            return Ok(None);
        };
        check_child_object_owner(parent, child, child_object).map(Some)
    }

    pub(crate) fn get_object_received_at_version_fallible(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let Some(recv_object) = self.get_object(receiving_object_id).map_err(|err| {
            format!("failed to read received object {receiving_object_id}: {err:#}")
        })?
        else {
            return Ok(None);
        };
        Ok(check_received_object(
            owner,
            receive_object_at_version,
            recv_object,
        ))
    }

    /// Get a signed transaction by digest from the RPC store. Pre-fork misses
    /// are fetched from GraphQL and persisted there.
    pub(crate) fn get_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<Option<VerifiedTransaction>> {
        let reader = self.local_store().reader();
        if let Some(transaction) = ReadStore::get_transaction(reader, digest) {
            return Ok(Some((*transaction).clone()));
        }
        Ok(self
            .fetch_and_save_transaction(digest)?
            .map(|info| info.transaction))
    }

    /// Get the checkpoint that finalized a transaction from RPC-store metadata.
    pub(crate) fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<Option<CheckpointSequenceNumber>> {
        let reader = self.local_store().reader();
        if let Some(sequence) = ReadStore::get_transaction_checkpoint(reader, digest) {
            return Ok(Some(sequence));
        }
        Ok(self
            .fetch_and_save_transaction(digest)?
            .map(|info| info.checkpoint))
    }

    /// Get transaction effects by digest, with the same RPC-store remote-fallback
    /// policy as [`Self::get_transaction`].
    pub(crate) fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<Option<TransactionEffects>> {
        let reader = self.local_store().reader();
        if let Some(effects) = ReadStore::get_transaction_effects(reader, digest) {
            return Ok(Some(effects));
        }
        Ok(self
            .fetch_and_save_transaction(digest)?
            .map(|info| info.effects))
    }

    /// Fetch a transaction and its effects from the remote GraphQL endpoint and persist them
    /// into the RPC store. Shared by [`Self::get_transaction`] and
    /// [`Self::get_transaction_effects`] so a single remote round-trip is used.
    ///
    /// The post-fork drop guard lives in [`RemoteSource::transaction`].
    fn fetch_and_save_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> anyhow::Result<Option<TransactionInfo>> {
        let Some(info) = self.inner.remote.transaction(digest)? else {
            return Ok(None);
        };

        let local_store = self.local_store();
        let checkpoint = self
            .get_checkpoint_by_sequence_number(info.checkpoint)?
            .ok_or_else(|| anyhow!("checkpoint {} not found on remote", info.checkpoint))?;
        let contents = self
            .get_checkpoint_contents_by_sequence_number(info.checkpoint)?
            .ok_or_else(|| {
                anyhow!(
                    "checkpoint {} contents not found on remote",
                    info.checkpoint
                )
            })?;
        local_store.save_checkpoint(&checkpoint, &contents)?;

        let events = if info.effects.events_digest().is_some() {
            self.inner.remote.transaction_events(digest)?
        } else {
            TransactionEvents::default()
        };
        local_store.save_transaction(
            &checkpoint,
            &contents,
            &info.transaction,
            &info.effects,
            &events,
        )?;

        Ok(Some(info))
    }

    /// Persist local object writes and current-state removals, then update the address-owned
    /// index from the same diff.
    fn apply_object_updates(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        removed_objects: Vec<ObjectRemoval>,
    ) -> anyhow::Result<()> {
        let _local_snapshot_guard = self
            .write_local_snapshot()
            .context("failed to lock local snapshot for object update")?;

        self.local_store()
            .apply_local_object_diff(&written_objects, &removed_objects)
    }

    /// Construct a `ForkStore` for tests, backed by an explicit local root and a fake (unused)
    /// GraphQL endpoint. The remote client is constructed but never called because tests should
    /// pre-populate the attached RPC store with the data they need.
    #[cfg(test)]
    pub(crate) fn new_for_testing(root: std::path::PathBuf, local_store: LocalStore) -> Self {
        let gql = GraphQLClient::new(
            crate::Node::Custom("http://localhost:1".to_string()),
            "test",
        )
        .expect("graphql store with localhost url should construct");
        let metadata = MetadataStore::new_with_root(root);
        Self::from_parts(0, gql, metadata, local_store)
    }

    /// Test-only constructor that lets callers point the GraphQL client at an arbitrary URL
    /// (e.g., a wiremock `MockServer`) and pin `forked_at_checkpoint` explicitly.
    #[cfg(test)]
    pub(crate) fn new_for_testing_with_remote(
        root: std::path::PathBuf,
        gql_url: String,
        forked_at_checkpoint: CheckpointSequenceNumber,
        local_store: LocalStore,
    ) -> Self {
        let gql = GraphQLClient::new(crate::Node::Custom(gql_url), "test")
            .expect("graphql store with custom url should construct");
        let metadata = MetadataStore::new_with_root(root);
        Self::from_parts(forked_at_checkpoint, gql, metadata, local_store)
    }

    /// Read the seed/local address-owner index from the RPC store.
    pub(crate) fn get_owned_object_infos(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Vec<OwnedObjectInfo>> {
        let local_store = self.local_store();
        let iter =
            RpcIndexes::owned_objects_iter(local_store.reader(), owner, object_type, cursor)?;
        iter.collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError::custom(e.to_string()))
    }

    pub(crate) fn save_address_owned_seed_objects(
        &self,
        object_refs: &[ObjectRef],
    ) -> anyhow::Result<()> {
        let local_store = self.local_store();
        let mut missing = Vec::new();

        for object_ref in object_refs {
            match local_store.get_object_at_version(object_ref.0, object_ref.1)? {
                Some(Status::Live(object)) => {
                    if object.compute_object_reference() != *object_ref {
                        bail!(
                            "seed object {} metadata does not match persisted object at version {}",
                            object_ref.0,
                            object_ref.1.value(),
                        );
                    }
                    local_store.save_address_owned_seed_object(&object)?;
                }
                Some(Status::Tombstone(_)) => bail!(
                    "seed object {} version {} is stored as removed",
                    object_ref.0,
                    object_ref.1.value(),
                ),
                None => missing.push(*object_ref),
            }
        }

        let objects = self
            .inner
            .remote
            .objects_at_fork(&missing, "seed objects")?;
        for object in objects {
            local_store.save_address_owned_seed_object(&object)?;
        }

        Ok(())
    }

    /// Seal the staged checkpoint matching `contents` into the rpc-store:
    /// summary and contents first, then every staged transaction it references,
    /// and finally drop the staged entries. Idempotent when the contents are
    /// already persisted.
    fn save_pending_checkpoint_contents(
        &self,
        contents: &CheckpointContents,
    ) -> anyhow::Result<()> {
        let local_store = self.local_store();
        if local_store
            .get_checkpoint_contents_by_digest(contents.digest())?
            .is_some()
        {
            return Ok(());
        }

        let checkpoint = self.inner.pending.checkpoint_for_contents(contents)?;
        local_store.save_checkpoint(&checkpoint, contents)?;

        let staged = self
            .inner
            .pending
            .staged_transactions_for(&checkpoint, contents)?;
        for transaction in &staged {
            local_store.save_transaction(
                &checkpoint,
                contents,
                &transaction.transaction,
                &transaction.effects,
                &transaction.events,
            )?;
        }

        self.inner.pending.clear_sealed(
            &checkpoint,
            staged.iter().map(|transaction| transaction.digest),
        )
    }
}

/// RPC-side helpers; the RPC storage trait impls below are the only callers.
impl ForkStore {
    /// The stock reader over the fork's local `sui-rpc-store`, for reads the
    /// fork has no policy for.
    fn stock_reader(&self) -> &RpcStoreReader {
        self.local_store().reader()
    }

    /// Chain identifier for the forked network: known networks use their
    /// fixed identifiers; custom networks derive one from the fork checkpoint
    /// digest.
    fn chain_identifier(&self) -> StorageResult<ChainIdentifier> {
        let id = match self.chain() {
            Chain::Mainnet => get_mainnet_chain_identifier(),
            Chain::Testnet => get_testnet_chain_identifier(),
            Chain::Unknown => {
                let checkpoint =
                    ForkStore::get_checkpoint_by_sequence_number(self, self.forked_at_checkpoint())
                        .map_err(to_storage_error)?
                        .ok_or_else(|| {
                            StorageError::missing(
                                "forked checkpoint missing -- cannot derive chain identifier",
                            )
                        })?;
                ChainIdentifier::from(*checkpoint.digest())
            }
        };
        Ok(id)
    }
}

// ============================================================================
// SimulatorStore super-traits
// ============================================================================

/// Object reads delegate to the inherent `ForkStore::get_object` / `get_object_at_version`,
/// which provide local-first lookups with remote fallback. The trait signature cannot
/// propagate errors, so failures are logged before being surfaced as `None`.
impl ObjectStore for ForkStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.get_object(object_id).unwrap_or_else(|err| {
            tracing::warn!(%object_id, "latest-object read failed: {err:#}");
            None
        })
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.get_object_at_version(object_id, version.value())
            .unwrap_or_else(|err| {
                tracing::warn!(%object_id, ?version, "versioned object read failed: {err:#}");
                None
            })
    }
}

/// Package reads go through the standard `load_package_object_from_object_store` helper, which
/// validates that the resolved object is actually a Move package.
impl BackingPackageStore for ForkStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

/// `ParentSync` is only required by older protocol versions and is never called by the executor
/// for the protocol versions we target. Calling it indicates a misconfiguration.
impl ParentSync for ForkStore {
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        panic!("Never called in newer protocol versions")
    }
}

/// Both methods go through the fallible helpers: a store or remote failure
/// must surface as an error rather than read as "object not found", which
/// execution would otherwise durably commit as a wrong result.
impl RuntimeObjectResolver for ForkStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        self.read_child_object_fallible(parent, child, child_version_upper_bound)
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        self.get_object_received_at_version_fallible(
            owner,
            receiving_object_id,
            receive_object_at_version,
        )
    }
}

// ============================================================================
// SimulatorStore
// ============================================================================

/// Write methods are fail-stop: the trait cannot surface errors, and executing
/// past a failed persist would silently diverge the executor's in-memory view
/// from durable fork state. Crashing is strictly safer than continuing on top
/// of unpersisted state.
impl SimulatorStore for ForkStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        ForkStore::get_checkpoint_by_sequence_number(self, sequence_number)
            .ok()
            .flatten()
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        ForkStore::get_checkpoint_by_digest(self, digest)
            .ok()
            .flatten()
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        ForkStore::get_highest_verified_checkpoint(self)
            .ok()
            .flatten()
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        ForkStore::get_checkpoint_contents_by_digest(self, digest)
            .ok()
            .flatten()
    }

    fn get_committee_by_epoch(&self, _epoch: EpochId) -> Option<Committee> {
        todo!("SimulatorStore::get_committee_by_epoch")
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        ForkStore::get_transaction(self, digest).ok().flatten()
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        ForkStore::get_transaction_effects(self, digest)
            .ok()
            .flatten()
    }

    fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        let local_store = self.local_store();
        ReadStore::get_events(local_store.reader(), digest)
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
        let objects = match self.get_owned_object_infos(owner, None, None).map(|infos| {
            infos
                .into_iter()
                .filter_map(|info| {
                    self.get_object(&info.object_id)
                        .ok()
                        .flatten()
                        .filter(|object| object.version() == info.version)
                })
                .collect()
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
        let local_store = self.local_store();
        if ReadStore::get_checkpoint_by_sequence_number(local_store.reader(), sequence).is_some() {
            return;
        }
        if let Err(err) = self.inner.pending.record_checkpoint(checkpoint) {
            panic!("failed to record pending checkpoint {sequence}: {err:?}");
        }
    }

    fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        let digest = *contents.digest();
        if let Err(err) = self.save_pending_checkpoint_contents(&contents) {
            panic!("failed to persist checkpoint contents {digest}: {err:?}");
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
        if let Err(err) = self.apply_object_updates(written_objects, removed_objects) {
            panic!("failed to persist transaction object updates for {tx_digest}: {err:?}");
        }
    }

    fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        let digest = *transaction.digest();
        if let Err(err) = self.inner.pending.record_transaction(transaction) {
            panic!("failed to record pending transaction {digest}: {err:?}");
        }
    }

    fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        let digest = *effects.transaction_digest();
        if let Err(err) = self.inner.pending.record_effects(effects) {
            panic!("failed to record pending transaction effects for {digest}: {err:?}");
        }
    }

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        if let Err(err) = self.inner.pending.record_events(*tx_digest, events) {
            panic!("failed to record pending transaction events for {tx_digest}: {err:?}");
        }
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        let removed_objects = deleted_objects
            .into_iter()
            .map(|(object_id, version, _digest)| ObjectRemoval {
                object_id,
                version,
                kind: TombstoneKind::Deleted,
            })
            .collect();
        if let Err(err) = self.apply_object_updates(written_objects, removed_objects) {
            panic!("failed to persist object updates: {err:?}");
        }
    }

    fn backing_store(&self) -> &dyn BackingStore {
        self
    }
}

// ============================================================================
// RPC storage traits
// ============================================================================
//
// The fork serves RPC directly through `sui-rpc-api` (it does not use
// `sui-rpc-node`): [`crate::startup`] hands the store to `RpcService` as its
// `RpcStateReader`. These impls are the routing table between fork policy and
// the stock rpc-store reader: every read the fork has policy for resolves
// through the local-first, remote-fallback helpers above, while surfaces the
// fork keeps no policy for -- events, full checkpoint contents, committees,
// epoch info, struct layouts, and the ledger/bitmap indexes, all written by
// the embedded indexer -- are served straight from the stock reader.
//
// The one read the stock reader could answer but must not is `get_object`
// without a version: a stock reverse scan over the fork's sparse `objects` CF
// would serve cached history as current, so latest-semantics reads go through
// the store's `LiveState`-backed path (see the `ObjectStore` impl above).

impl ReadStore for ForkStore {
    /// Reads committee information from committed `sui-rpc-store` data.
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.stock_reader().get_committee(epoch)
    }

    /// Reads the latest checkpoint from the local rpc-store. This never
    /// consults the remote endpoint -- the local executor is the source of
    /// truth for "latest" in a forked network.
    fn get_latest_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        ForkStore::get_highest_verified_checkpoint(self)
            .map_err(to_storage_error)?
            .ok_or_else(|| StorageError::missing("no checkpoint persisted yet"))
    }

    /// Reads the highest verified checkpoint: the fork's local tip.
    fn get_highest_verified_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        self.get_latest_checkpoint()
    }

    /// Reads the highest synced checkpoint: also the fork's local tip.
    fn get_highest_synced_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        ForkStore::get_highest_verified_checkpoint(self)
            .map_err(to_storage_error)?
            .ok_or_else(|| {
                StorageError::missing(
                    "no checkpoint persisted yet -- cannot determine highest synced checkpoint",
                )
            })
    }

    /// Returns the remote chain's lowest available checkpoint.
    ///
    /// This value is not derived from the fork's local store.
    fn get_lowest_available_checkpoint(&self) -> StorageResult<CheckpointSequenceNumber> {
        ForkStore::get_lowest_available_checkpoint(self).map_err(to_storage_error)
    }

    /// Reads a checkpoint summary by checkpoint digest; rpc-store only (the
    /// GraphQL checkpoint query is keyed by sequence number).
    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        optional_store_read(
            "checkpoint digest lookup",
            ForkStore::get_checkpoint_by_digest(self, digest),
        )
    }

    /// Reads a checkpoint summary by sequence number, persisting pre-fork
    /// rows fetched from the remote.
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        optional_store_read(
            "checkpoint sequence lookup",
            ForkStore::get_checkpoint_by_sequence_number(self, sequence_number),
        )
    }

    /// Reads checkpoint contents by content digest.
    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        optional_store_read(
            "checkpoint contents digest lookup",
            ForkStore::get_checkpoint_contents_by_digest(self, digest),
        )
    }

    /// Reads checkpoint contents by sequence number, with the same remote
    /// fallback as the summary read.
    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        optional_store_read(
            "checkpoint contents sequence lookup",
            ForkStore::get_checkpoint_contents_by_sequence_number(self, sequence_number),
        )
    }

    /// Reads a transaction by digest, persisting pre-fork rows fetched from
    /// the remote.
    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        optional_store_read(
            "transaction lookup",
            ForkStore::get_transaction(self, tx_digest),
        )
        .map(Arc::new)
    }

    /// Reads transaction effects by digest, with the same remote fallback as
    /// the transaction read.
    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        optional_store_read(
            "transaction effects lookup",
            ForkStore::get_transaction_effects(self, tx_digest),
        )
    }

    /// Reads transaction events from the rpc-store only. Events are saved
    /// with their transaction rows and the fork keeps no separate copy, so a
    /// fork-side fallback would just repeat the same lookup.
    fn get_events(&self, event_digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.stock_reader().get_events(event_digest)
    }

    /// Reads unchanged runtime-loaded objects from committed `sui-rpc-store`
    /// data only; the fork does not synthesize this execution cache.
    fn get_unchanged_loaded_runtime_objects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        self.stock_reader()
            .get_unchanged_loaded_runtime_objects(digest)
    }

    /// Reads the checkpoint sequence that finalized a transaction.
    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        optional_store_read(
            "transaction checkpoint lookup",
            ForkStore::get_transaction_checkpoint(self, digest),
        )
    }

    /// Reads full checkpoint contents from committed `sui-rpc-store` data
    /// only; the fork exposes checkpoint summaries and contents, not full
    /// checkpoint payloads.
    fn get_full_checkpoint_contents(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
        digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        self.stock_reader()
            .get_full_checkpoint_contents(sequence_number, digest)
    }
}

impl RpcStateReader for ForkStore {
    /// Returns the lowest checkpoint with object data on the remote chain;
    /// availability metadata, not fork-local state.
    fn get_lowest_available_checkpoint_objects(&self) -> StorageResult<CheckpointSequenceNumber> {
        ForkStore::get_lowest_available_checkpoint_objects(self).map_err(to_storage_error)
    }

    /// Genuinely hybrid: the stock read serves the framework `chain_ids`
    /// table seeded at open, while the fallback derives the identifier from
    /// the fork checkpoint for custom networks.
    fn get_chain_identifier(&self) -> StorageResult<ChainIdentifier> {
        fallback_on_missing(self.stock_reader().get_chain_identifier(), || {
            self.chain_identifier()
        })
    }

    /// Exposes the store as the RPC index provider.
    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        Some(self)
    }

    /// Reads a struct layout from `sui-rpc-store`.
    fn get_struct_layout_with_overlay(
        &self,
        struct_tag: &StructTag,
        overlay: &ObjectSet,
    ) -> StorageResult<Option<MoveTypeLayout>> {
        self.stock_reader()
            .get_struct_layout_with_overlay(struct_tag, overlay)
    }
}

impl RpcIndexes for ForkStore {
    /// Reads epoch index metadata from `sui-rpc-store`.
    fn get_epoch_info(&self, epoch: EpochId) -> StorageResult<Option<EpochInfo>> {
        RpcIndexes::get_epoch_info(self.stock_reader(), epoch)
    }

    /// Initialize and iterate address-owned objects from the RPC-store owner
    /// index. The remote scan is checkpoint-bounded and recorded in the
    /// metadata store so repeated owner queries read the local index.
    fn owned_objects_iter(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Box<dyn Iterator<Item = Result<OwnedObjectInfo, TypedStoreError>> + '_>>
    {
        self.inner
            .inventory
            .ensure_address_owner(owner)
            .map_err(to_storage_error)?;
        RpcIndexes::owned_objects_iter(self.stock_reader(), owner, object_type, cursor)
    }

    /// Initialize and iterate the object-owned children of `parent`, with the
    /// same checkpoint-bounded remote scan as the owner index.
    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<DynamicFieldKey>,
    ) -> StorageResult<Box<dyn Iterator<Item = DynamicFieldIteratorItem> + '_>> {
        self.inner
            .inventory
            .ensure_object_owner(parent)
            .map_err(to_storage_error)?;
        RpcIndexes::dynamic_field_iter(self.stock_reader(), parent, cursor)
    }

    /// Initialize the type indexes needed to assemble RPC coin metadata.
    fn get_coin_info(&self, coin_type: &StructTag) -> StorageResult<Option<CoinInfo>> {
        self.inner
            .inventory
            .ensure_coin_info(coin_type)
            .map_err(to_storage_error)?;
        RpcIndexes::get_coin_info(self.stock_reader(), coin_type)
    }

    /// Initialize address inventory and read an address balance from the
    /// RPC-store balance index.
    fn get_balance(
        &self,
        owner: &SuiAddress,
        coin_type: &StructTag,
    ) -> StorageResult<Option<BalanceInfo>> {
        self.inner
            .inventory
            .ensure_address_owner(*owner)
            .map_err(to_storage_error)?;
        RpcIndexes::get_balance(self.stock_reader(), owner, coin_type)
    }

    /// Initialize address inventory and iterate address balances from the
    /// RPC-store balance index.
    fn balance_iter(
        &self,
        owner: &SuiAddress,
        cursor: Option<(SuiAddress, StructTag)>,
    ) -> StorageResult<BalanceIterator<'_>> {
        self.inner
            .inventory
            .ensure_address_owner(*owner)
            .map_err(to_storage_error)?;
        RpcIndexes::balance_iter(self.stock_reader(), owner, cursor)
    }

    /// Iterates package versions from committed `sui-rpc-store` indexes.
    fn package_versions_iter(
        &self,
        original_id: ObjectID,
        cursor: Option<u64>,
    ) -> StorageResult<Box<dyn Iterator<Item = Result<(u64, ObjectID), TypedStoreError>> + '_>>
    {
        RpcIndexes::package_versions_iter(self.stock_reader(), original_id, cursor)
    }

    /// Genuinely hybrid: the stock read serves the indexer watermark, while
    /// the fallback reports the highest persisted checkpoint before the
    /// indexer has written its first watermark.
    fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> StorageResult<Option<CheckpointSequenceNumber>> {
        match RpcIndexes::get_highest_indexed_checkpoint_seq_number(self.stock_reader())? {
            Some(sequence) => Ok(Some(sequence)),
            None => Ok(self.get_highest_checkpoint().ok()),
        }
    }

    /// Reads the transaction sequence-to-digest index from `sui-rpc-store`.
    fn ledger_tx_seq_digest(&self, tx_seq: u64) -> StorageResult<Option<LedgerTxSeqDigest>> {
        RpcIndexes::ledger_tx_seq_digest(self.stock_reader(), tx_seq)
    }

    /// Iterates transaction sequence-to-digest rows from `sui-rpc-store`.
    fn ledger_tx_seq_digest_iter(
        &self,
        start: u64,
        end_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerTxSeqDigestIterator<'_>> {
        RpcIndexes::ledger_tx_seq_digest_iter(self.stock_reader(), start, end_exclusive, descending)
    }

    /// Iterates transaction bitmap buckets from `sui-rpc-store`.
    fn transaction_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerBitmapBucketIterator<'_>> {
        RpcIndexes::transaction_bitmap_bucket_iter(
            self.stock_reader(),
            dimension_key,
            start_bucket,
            end_bucket_exclusive,
            descending,
        )
    }

    /// Iterates event bitmap buckets from `sui-rpc-store`.
    fn event_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerBitmapBucketIterator<'_>> {
        RpcIndexes::event_bitmap_bucket_iter(
            self.stock_reader(),
            dimension_key,
            start_bucket,
            end_bucket_exclusive,
            descending,
        )
    }
}

fn to_storage_error(err: anyhow::Error) -> StorageError {
    StorageError::custom(err.to_string())
}

/// Validate that a child object loaded for `parent` is actually owned by it.
///
/// Shared by the fallible child read (RPC path) and the `RuntimeObjectResolver`
/// impl (execution path), which differ only in how lookup errors surface.
fn check_child_object_owner(
    parent: &ObjectID,
    child: &ObjectID,
    child_object: Object,
) -> SuiResult<Object> {
    if child_object.owner != sui_types::object::Owner::ObjectOwner((*parent).into()) {
        return Err(sui_types::error::SuiErrorKind::InvalidChildObjectAccess {
            object: *child,
            given_parent: *parent,
            actual_owner: child_object.owner.clone(),
        }
        .into());
    }
    Ok(child_object)
}

/// Received-object checks: owner and exact version; mismatches surface as `None`.
fn check_received_object(
    owner: &ObjectID,
    receive_object_at_version: SequenceNumber,
    object: Object,
) -> Option<Object> {
    if object.owner != sui_types::object::Owner::AddressOwner((*owner).into()) {
        return None;
    }
    if object.version() != receive_object_at_version {
        return None;
    }
    Some(object)
}

/// Converts effect removals into object tombstones for the RPC store.
fn removed_objects_from_effects(effects: &TransactionEffects) -> Vec<ObjectRemoval> {
    effects
        .deleted()
        .into_iter()
        .map(|object_ref| ObjectRemoval {
            object_id: object_ref.0,
            version: object_ref.1,
            kind: TombstoneKind::Deleted,
        })
        .chain(
            effects
                .unwrapped_then_deleted()
                .into_iter()
                .map(|object_ref| ObjectRemoval {
                    object_id: object_ref.0,
                    version: object_ref.1,
                    kind: TombstoneKind::Deleted,
                }),
        )
        .chain(
            effects
                .wrapped()
                .into_iter()
                .map(|object_ref| ObjectRemoval {
                    object_id: object_ref.0,
                    version: object_ref.1,
                    kind: TombstoneKind::Wrapped,
                }),
        )
        .collect()
}

/// Runs the fork-policy fallback only when the stock rpc-store read reports
/// missing data.
fn fallback_on_missing<T>(
    result: StorageResult<T>,
    fallback: impl FnOnce() -> StorageResult<T>,
) -> StorageResult<T> {
    match result {
        Ok(value) => Ok(value),
        Err(err) if err.kind() == StorageErrorKind::Missing => fallback(),
        Err(err) => Err(err),
    }
}

/// Converts a fallible fork-policy read into an optional trait response.
///
/// The storage traits using this helper return `Option`, so store errors are
/// logged and treated as absent data.
fn optional_store_read<T>(context: &'static str, result: anyhow::Result<Option<T>>) -> Option<T> {
    match result {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(context, error = ?err, "fork-state read failed");
            None
        }
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

#[cfg(test)]
#[path = "tests/store_rpc_traits.rs"]
mod rpc_traits_tests;
