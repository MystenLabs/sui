// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::anyhow;

use simulacrum::store::SimulatorStore;
use sui_protocol_config::Chain;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::clock::Clock;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::ObjectDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::error::SuiResult;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::BackingStore;
use sui_types::storage::ChildObjectResolver;
use sui_types::storage::ObjectStore;
use sui_types::storage::PackageObject;
use sui_types::storage::ParentSync;
use sui_types::storage::load_package_object_from_object_store;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::VerifiedTransaction;

use crate::CheckpointRead;
use crate::GraphQLClient;
use crate::Node;
use crate::ObjectKey;
use crate::ObjectRead;
use crate::VersionQuery;
use crate::filesystem::FilesystemStore;

/// A data store for Sui data, combining a local filesystem cache with a remote GraphQL endpoint
/// for historical reads. Pre-fork data is fetched on demand and cached locally; post-fork data
/// (written by the executor) lives on disk only.
///
/// Implements [`SimulatorStore`] so it can be passed directly into
/// [`simulacrum::Simulacrum::new_from_custom_state`].
pub(crate) struct DataStore {
    forked_at_checkpoint: CheckpointSequenceNumber,
    gql: GraphQLClient,
    local: FilesystemStore,
}

impl DataStore {
    /// Create a new `DataStore` for the given network, anchored at `forked_at_checkpoint`.
    ///
    /// The local filesystem cache is rooted under a per-network, per-checkpoint directory
    /// (see [`FilesystemStore`]). The GraphQL client is constructed eagerly but no remote
    /// requests are made until reads happen.
    pub(crate) async fn new(
        node: Node,
        forked_at_checkpoint: CheckpointSequenceNumber,
        version: &str,
    ) -> Result<Self, anyhow::Error> {
        let gql = GraphQLClient::new(node.clone(), version)?;
        let local = FilesystemStore::new(&node, forked_at_checkpoint)?;

        Ok(Self {
            forked_at_checkpoint,
            gql,
            local,
        })
    }

    fn forked_at_checkpoint(&self) -> CheckpointSequenceNumber {
        self.forked_at_checkpoint
    }

    /// Return the chain (mainnet/testnet/devnet/unknown) this store is connected to.
    pub fn get_chain_identifier(&self) -> Chain {
        self.gql.chain()
    }

    /// Get a checkpoint summary by sequence number. Prefers the local cache;
    /// on miss, any pre-fork checkpoint (`sequence <= forked_at_checkpoint`)
    /// is fetched from the remote GraphQL endpoint and written back to disk
    /// so subsequent reads hit the cache. Post-fork checkpoints are never
    /// fetched remotely — they only exist if the local executor produced
    /// them, so a miss returns `None`.
    pub(crate) fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        if let Some(checkpoint) = self.local.get_checkpoint_by_sequence_number(sequence)? {
            return Ok(Some(checkpoint));
        }
        if sequence > self.forked_at_checkpoint {
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
            .local
            .get_checkpoint_contents_by_sequence_number(sequence)?
        {
            return Ok(Some(contents));
        }
        if sequence > self.forked_at_checkpoint {
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
        self.local.get_checkpoint_by_digest(digest)
    }

    /// Look up checkpoint contents by their digest. Local only: contents are
    /// content-addressed on disk, but the remote GraphQL schema does not
    /// expose a contents-by-digest query, so there is no fallback path.
    pub(crate) fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        self.local.get_checkpoint_contents_by_digest(digest)
    }

    /// Return the highest checkpoint summary cached locally. This never
    /// consults the remote endpoint — the local executor is the source of
    /// truth for "latest" in a forked network.
    pub(crate) fn get_highest_verified_checkpoint(
        &self,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        self.local.get_highest_verified_checkpoint()
    }

    /// Eagerly populate the cache with the startup (forked-at) checkpoint so
    /// any bootstrap failure surfaces now instead of on first access.
    pub(crate) fn download_and_persist_startup_checkpoint(&self) -> anyhow::Result<()> {
        self.get_checkpoint_by_sequence_number(self.forked_at_checkpoint)?
            .ok_or_else(|| {
                anyhow!(
                    "checkpoint {} not found on remote",
                    self.forked_at_checkpoint
                )
            })?;
        Ok(())
    }

    /// Fetch a checkpoint pair from the remote GraphQL endpoint and persist
    /// both halves to disk. Shared by the sequence-keyed cache-aware getters.
    fn fetch_and_cache_checkpoint(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> anyhow::Result<Option<(VerifiedCheckpoint, CheckpointContents)>> {
        let Some((checkpoint, contents)) = self.gql.get_verified_checkpoint(Some(sequence))? else {
            return Ok(None);
        };
        // Write contents first: they're content-addressed (idempotent), so
        // if the summary write fails afterward the contents are harmless
        // orphans and the next request retries cleanly. The reverse order
        // would leave a summary on disk pointing to missing contents.
        self.local.write_checkpoint_contents(&contents)?;
        self.local.write_checkpoint_summary(&checkpoint)?;
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
        if let Some(object) = self.local.get_object_at_version(object_id, version)? {
            return Ok(Some(object));
        }

        let object =
            self.get_object_from_remote(object_id, Some(version), self.forked_at_checkpoint())?;

        if let Some(ref object) = object {
            self.local.write_object(object)?;
        }

        Ok(object)
    }

    /// Local-first lookup for the latest known version of an object. Falls back to a remote
    /// `AtCheckpoint(forked_at_checkpoint)` query and caches the result on disk.
    fn get_latest_object(&self, object_id: &ObjectID) -> anyhow::Result<Option<Object>> {
        if let Some(object) = self.local.get_latest_object(object_id)? {
            return Ok(Some(object));
        }

        // if not found, load from remote rpc at forked checkpoint and save it to disk for future
        // use
        let object = self.get_object_from_remote(object_id, None, self.forked_at_checkpoint())?;

        if let Some(ref object) = object {
            self.local.write_object(object)?;
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

        let objects = self.gql.get_objects(&[ObjectKey {
            object_id: *object_id,
            version_query,
        }])?;

        Ok(objects
            .into_iter()
            .next()
            .flatten()
            .map(|(object, _)| object))
    }

    /// Get the highest checkpoint sequence number available on disk.
    pub(crate) fn get_highest_checkpoint(&self) -> anyhow::Result<CheckpointSequenceNumber> {
        self.local.get_highest_checkpoint_sequence_number()
    }

    /// Construct a `DataStore` for tests, backed by an explicit local root and a fake (unused)
    /// GraphQL endpoint. The remote client is constructed but never called because tests should
    /// pre-populate the local cache with the data they need.
    #[cfg(test)]
    pub(crate) fn new_for_testing(root: std::path::PathBuf) -> Self {
        let gql = GraphQLClient::new(Node::Custom("http://localhost:1".to_string()), "test")
            .expect("graphql store with localhost url should construct");
        let local = FilesystemStore::new_with_root(root);
        Self {
            forked_at_checkpoint: 0,
            gql,
            local,
        }
    }
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
        self.local.get_transaction(digest).ok().flatten()
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.local.get_transaction_effects(digest).ok().flatten()
    }

    fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.local.get_transaction_events(digest).ok().flatten()
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

    fn owned_objects(&self, _owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        todo!("SimulatorStore::owned_objects")
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        let sequence = checkpoint.data().sequence_number;
        // Pre-fork summary was persisted at seed time; skip rewrites.
        if self
            .local
            .get_checkpoint_by_sequence_number(sequence)
            .ok()
            .flatten()
            .is_some()
        {
            return;
        }
        if let Err(err) = self.local.write_checkpoint_summary(&checkpoint) {
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
            .local
            .get_checkpoint_contents_by_digest(&digest)
            .ok()
            .flatten()
            .is_some()
        {
            return;
        }
        if let Err(err) = self.local.write_checkpoint_contents(&contents) {
            tracing::error!(
                contents_digest = %digest,
                "failed to persist checkpoint contents: {err:?}",
            );
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
        let deleted_objects = effects.deleted();
        let tx_digest = *effects.transaction_digest();
        self.insert_transaction(transaction);
        self.insert_transaction_effects(effects);
        self.insert_events(&tx_digest, events);
        self.update_objects(written_objects, deleted_objects);
    }

    fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        let digest = *transaction.digest();
        self.local
            .write_transaction(&digest, &transaction)
            .expect("failed to persist transaction to disk");
    }

    fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        let digest = *effects.transaction_digest();
        self.local
            .write_transaction_effects(&digest, &effects)
            .expect("failed to persist transaction effects to disk");
    }

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        self.local
            .write_transaction_events(tx_digest, &events)
            .expect("failed to persist transaction events to disk");
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        _deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        for object in written_objects.values() {
            self.local
                .write_object(object)
                .expect("failed to write object to disk");
        }
    }

    fn backing_store(&self) -> &dyn BackingStore {
        self
    }
}

#[cfg(test)]
#[path = "tests/store_checkpoint_persistence.rs"]
mod checkpoint_persistence_tests;

#[cfg(test)]
#[path = "tests/store_execution.rs"]
mod execution_tests;
