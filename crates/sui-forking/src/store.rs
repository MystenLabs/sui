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

    /// Fetch a verified checkpoint from the remote GraphQL endpoint. When `checkpoint` is `None`,
    /// the store's `forked_at_checkpoint` is used as the default.
    pub(crate) async fn get_verified_checkpoint_from_rpc(
        &self,
        checkpoint: Option<CheckpointSequenceNumber>,
    ) -> anyhow::Result<Option<VerifiedCheckpoint>> {
        let checkpoint = checkpoint.unwrap_or(self.forked_at_checkpoint);
        let verified_checkpoint = self.gql.get_verified_checkpoint(Some(checkpoint))?;

        Ok(verified_checkpoint)
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
        let child_object = match <Self as ObjectStore>::get_object(self, child) {
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
        let recv_object = match <Self as ObjectStore>::get_object(self, receiving_object_id) {
            None => return Ok(None),
            Some(obj) => obj,
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
        _sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        todo!("SimulatorStore::get_checkpoint_by_sequence_number")
    }

    fn get_checkpoint_by_digest(&self, _digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        todo!("SimulatorStore::get_checkpoint_by_digest")
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        todo!()
    }

    fn get_checkpoint_contents(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        todo!("SimulatorStore::get_checkpoint_contents")
    }

    fn get_committee_by_epoch(&self, _epoch: EpochId) -> Option<Committee> {
        todo!("SimulatorStore::get_committee_by_epoch")
    }

    fn get_transaction(&self, _digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        todo!("SimulatorStore::get_transaction")
    }

    fn get_transaction_effects(&self, _digest: &TransactionDigest) -> Option<TransactionEffects> {
        todo!("SimulatorStore::get_transaction_effects")
    }

    fn get_transaction_events(&self, _digest: &TransactionDigest) -> Option<TransactionEvents> {
        todo!("SimulatorStore::get_transaction_events")
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
        <Self as ObjectStore>::get_object(self, &sui_types::SUI_CLOCK_OBJECT_ID)
            .expect("clock should exist")
            .to_rust()
            .expect("clock object should deserialize")
    }

    fn owned_objects(&self, _owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        todo!("SimulatorStore::owned_objects")
    }

    fn insert_checkpoint(&mut self, _checkpoint: VerifiedCheckpoint) {
        todo!("SimulatorStore::insert_checkpoint")
    }

    fn insert_checkpoint_contents(&mut self, _contents: CheckpointContents) {
        todo!("SimulatorStore::insert_checkpoint_contents")
    }

    fn insert_committee(&mut self, _committee: Committee) {
        todo!("SimulatorStore::insert_committee")
    }

    fn insert_executed_transaction(
        &mut self,
        _transaction: VerifiedTransaction,
        _effects: TransactionEffects,
        _events: TransactionEvents,
        _written_objects: BTreeMap<ObjectID, Object>,
    ) {
        todo!("SimulatorStore::insert_executed_transaction")
    }

    fn insert_transaction(&mut self, _transaction: VerifiedTransaction) {
        todo!("SimulatorStore::insert_transaction")
    }

    fn insert_transaction_effects(&mut self, _effects: TransactionEffects) {
        todo!("SimulatorStore::insert_transaction_effects")
    }

    fn insert_events(&mut self, _tx_digest: &TransactionDigest, _events: TransactionEvents) {
        todo!("SimulatorStore::insert_events")
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
