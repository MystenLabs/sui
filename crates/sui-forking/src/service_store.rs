// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use forking_data_store::{
    CheckpointStore, CheckpointStoreWriter, LatestObjectStore, ObjectKey,
    ObjectStore as ForkingObjectStore, VersionQuery,
};
use simulacrum::SimulatorStore;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    clock::Clock,
    committee::{Committee, EpochId},
    digests::{CheckpointContentsDigest, CheckpointDigest, ObjectDigest, TransactionDigest},
    effects::{TransactionEffects, TransactionEvents},
    error::{SuiErrorKind, SuiResult},
    full_checkpoint_content::{Checkpoint as CheckpointData, ObjectSet},
    messages_checkpoint::{CheckpointContents, CheckpointSequenceNumber, VerifiedCheckpoint},
    object::{Object, Owner},
    storage::{
        BackingPackageStore, ChildObjectResolver, PackageObject, ParentSync,
        load_package_object_from_object_store,
    },
    sui_system_state::SuiSystemState,
    transaction::VerifiedTransaction,
};

/// Persistent checkpoint/object adapter backing a forked network.
///
/// `ServiceStore` keeps two composed `ForkingStore` values:
/// - `historical_store` serves checkpoint reads up to and including the fork point
/// - `local_store` serves post-fork checkpoint/object reads and all local writes
pub struct ServiceStore<H, L> {
    forked_at_checkpoint: u64,
    historical_store: H,
    local_store: L,
    pending_checkpoints: HashMap<CheckpointContentsDigest, VerifiedCheckpoint>,
}

impl<H, L> ServiceStore<H, L>
where
    H: CheckpointStore,
    L: CheckpointStore + CheckpointStoreWriter,
{
    /// Create a service store over historical and local `ForkingStore` values.
    pub fn new(forked_at_checkpoint: u64, historical_store: H, local_store: L) -> Self {
        Self {
            forked_at_checkpoint,
            historical_store,
            local_store,
            pending_checkpoints: HashMap::new(),
        }
    }

    fn checkpoint_store_for_sequence(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> &dyn CheckpointStore {
        if sequence_number <= self.forked_at_checkpoint {
            &self.historical_store
        } else {
            &self.local_store
        }
    }

    /// Return the checkpoint summary for a sequence, routing around the fork boundary.
    pub fn checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.checkpoint_store_for_sequence(sequence_number)
            .get_checkpoint_by_sequence_number(sequence_number)
            .ok()
            .flatten()
            .map(|checkpoint| VerifiedCheckpoint::new_unchecked(checkpoint.summary))
    }

    /// Return the checkpoint summary for a digest, preferring the local store first.
    ///
    /// The local-first lookup makes post-fork checkpoints immediately visible even though
    /// historical checkpoints may also be cached on disk.
    pub fn checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.local_store
            .get_sequence_by_checkpoint_digest(digest)
            .ok()
            .flatten()
            .and_then(|sequence_number| self.checkpoint_by_sequence_number(sequence_number))
            .or_else(|| {
                self.historical_store
                    .get_sequence_by_checkpoint_digest(digest)
                    .ok()
                    .flatten()
                    .and_then(|sequence_number| self.checkpoint_by_sequence_number(sequence_number))
            })
    }

    /// Return checkpoint contents for a sequence, routing around the fork boundary.
    pub fn checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        self.checkpoint_store_for_sequence(sequence_number)
            .get_checkpoint_by_sequence_number(sequence_number)
            .ok()
            .flatten()
            .map(|checkpoint| checkpoint.contents)
    }

    /// Return checkpoint contents for a digest, preferring the local store first.
    pub fn checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.local_store
            .get_sequence_by_contents_digest(digest)
            .ok()
            .flatten()
            .and_then(|sequence_number| {
                self.checkpoint_contents_by_sequence_number(sequence_number)
            })
            .or_else(|| {
                self.historical_store
                    .get_sequence_by_contents_digest(digest)
                    .ok()
                    .flatten()
                    .and_then(|sequence_number| {
                        self.checkpoint_contents_by_sequence_number(sequence_number)
                    })
            })
    }

    /// Return the highest available checkpoint, preferring locally written checkpoints first.
    pub fn highest_checkpoint(&self) -> Option<VerifiedCheckpoint> {
        self.local_store
            .get_latest_checkpoint()
            .ok()
            .flatten()
            .map(|checkpoint| VerifiedCheckpoint::new_unchecked(checkpoint.summary))
            .or_else(|| {
                self.historical_store
                    .get_latest_checkpoint()
                    .ok()
                    .flatten()
                    .map(|checkpoint| VerifiedCheckpoint::new_unchecked(checkpoint.summary))
            })
    }

    /// Stage a checkpoint summary until its contents arrive.
    pub fn insert_checkpoint_summary(&mut self, checkpoint: VerifiedCheckpoint) {
        self.pending_checkpoints
            .insert(checkpoint.data().content_digest, checkpoint);
    }

    /// Persist a complete post-fork checkpoint once both summary and contents are available.
    pub fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        let digest = *contents.digest();
        if let Some(summary) = self.pending_checkpoints.remove(&digest) {
            let checkpoint = CheckpointData {
                summary: summary.into_inner(),
                contents,
                transactions: Vec::new(),
                object_set: ObjectSet::default(),
            };
            self.local_store
                .write_checkpoint(&checkpoint)
                .expect("local checkpoint writes should succeed");
        }
    }
}

impl<H, L> ServiceStore<H, L>
where
    H: CheckpointStore + ForkingObjectStore,
    L: CheckpointStore + CheckpointStoreWriter + ForkingObjectStore + LatestObjectStore,
{
    fn object_lookup(
        store: &dyn ForkingObjectStore,
        object_id: &ObjectID,
        version_query: VersionQuery,
    ) -> Option<(Object, u64)> {
        store
            .get_objects(&[ObjectKey {
                object_id: *object_id,
                version_query,
            }])
            .ok()?
            .into_iter()
            .next()
            .flatten()
    }

    fn local_object_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Option<Object> {
        Self::object_lookup(
            &self.local_store,
            object_id,
            VersionQuery::Version(version.value()),
        )
        .map(|(object, _)| object)
    }

    fn historical_object_at_fork(&self, object_id: &ObjectID) -> Option<(Object, u64)> {
        Self::object_lookup(
            &self.historical_store,
            object_id,
            VersionQuery::AtCheckpoint(self.forked_at_checkpoint),
        )
    }

    fn historical_exact_object_if_visible_at_fork(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Option<Object> {
        let (_, fork_version) = self.historical_object_at_fork(object_id)?;
        if version.value() > fork_version {
            return None;
        }

        Self::object_lookup(
            &self.historical_store,
            object_id,
            VersionQuery::Version(version.value()),
        )
        .map(|(object, _)| object)
    }

    fn latest_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.local_store
            .latest_object(object_id)
            .ok()
            .flatten()
            .map(|(object, _)| object)
            .or_else(|| {
                self.historical_object_at_fork(object_id)
                    .map(|(object, _)| object)
            })
    }
}

#[cold]
fn checkpoint_only_store(method: &str) -> ! {
    todo!("{method} is not yet implemented for ServiceStore")
}

impl<H, L> BackingPackageStore for ServiceStore<H, L>
where
    H: CheckpointStore + ForkingObjectStore,
    L: CheckpointStore + CheckpointStoreWriter + ForkingObjectStore + LatestObjectStore,
{
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

impl<H, L> ChildObjectResolver for ServiceStore<H, L>
where
    H: CheckpointStore + ForkingObjectStore,
    L: CheckpointStore + CheckpointStoreWriter + ForkingObjectStore + LatestObjectStore,
{
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let validate = |child_object: Object| -> SuiResult<Option<Object>> {
            let parent = *parent;
            if child_object.owner != Owner::ObjectOwner(parent.into()) {
                return Err(SuiErrorKind::InvalidChildObjectAccess {
                    object: *child,
                    given_parent: parent,
                    actual_owner: child_object.owner.clone(),
                }
                .into());
            }

            // Post-fork local objects are currently stored without a version
            // upper-bound index, so we cannot efficiently resolve the correct
            // version when the child exceeds the bound. This is deferred to PR5
            // which introduces transaction execution and object-write tracking.
            if child_object.version() > child_version_upper_bound {
                return Err(SuiErrorKind::UnsupportedFeatureError {
                    error:
                        "ServiceStore::read_child_object does not yet support bounded local post-fork reads"
                            .to_owned(),
                }
                .into());
            }

            Ok(Some(child_object))
        };

        let local_object = Self::object_lookup(
            &self.local_store,
            child,
            VersionQuery::RootVersion(child_version_upper_bound.value()),
        )
        .map(|(object, _)| object);
        if let Some(child_object) = local_object {
            return validate(child_object);
        }

        let historical_object = Self::object_lookup(
            &self.historical_store,
            child,
            VersionQuery::RootVersion(child_version_upper_bound.value()),
        )
        .map(|(object, _)| object);

        match historical_object {
            Some(object) => validate(object),
            None => Ok(None),
        }
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        let Some(received_object) = sui_types::storage::ObjectStore::get_object_by_key(
            self,
            receiving_object_id,
            receive_object_at_version,
        ) else {
            return Ok(None);
        };
        if received_object.owner != Owner::AddressOwner((*owner).into()) {
            return Ok(None);
        }

        Ok(Some(received_object))
    }
}

impl<H, L> sui_types::storage::ObjectStore for ServiceStore<H, L>
where
    H: CheckpointStore + ForkingObjectStore,
    L: CheckpointStore + CheckpointStoreWriter + ForkingObjectStore + LatestObjectStore,
{
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.latest_object(object_id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.local_object_version(object_id, version)
            .or_else(|| self.historical_exact_object_if_visible_at_fork(object_id, version))
    }
}

impl<H, L> ParentSync for ServiceStore<H, L>
where
    H: CheckpointStore,
    L: CheckpointStore + CheckpointStoreWriter,
{
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        checkpoint_only_store("get_latest_parent_entry_ref_deprecated")
    }
}

impl<H, L> SimulatorStore for ServiceStore<H, L>
where
    H: CheckpointStore + ForkingObjectStore,
    L: CheckpointStore + CheckpointStoreWriter + ForkingObjectStore + LatestObjectStore,
{
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.checkpoint_by_digest(digest)
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        self.highest_checkpoint()
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.checkpoint_contents_by_digest(digest)
    }

    fn get_committee_by_epoch(&self, _epoch: EpochId) -> Option<Committee> {
        checkpoint_only_store("get_committee_by_epoch")
    }

    fn get_transaction(&self, _digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        checkpoint_only_store("get_transaction")
    }

    fn get_transaction_effects(&self, _digest: &TransactionDigest) -> Option<TransactionEffects> {
        checkpoint_only_store("get_transaction_effects")
    }

    fn get_transaction_events(&self, _digest: &TransactionDigest) -> Option<TransactionEvents> {
        checkpoint_only_store("get_transaction_events")
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        sui_types::storage::ObjectStore::get_object(self, id)
    }

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        sui_types::storage::ObjectStore::get_object_by_key(self, id, version)
    }

    fn get_system_state(&self) -> SuiSystemState {
        checkpoint_only_store("get_system_state")
    }

    fn get_clock(&self) -> Clock {
        checkpoint_only_store("get_clock")
    }

    fn owned_objects(&self, _owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        checkpoint_only_store("owned_objects")
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        self.insert_checkpoint_summary(checkpoint);
    }

    fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        ServiceStore::insert_checkpoint_contents(self, contents);
    }

    fn insert_committee(&mut self, _committee: Committee) {
        checkpoint_only_store("insert_committee")
    }

    fn insert_executed_transaction(
        &mut self,
        _transaction: VerifiedTransaction,
        _effects: TransactionEffects,
        _events: TransactionEvents,
        _written_objects: BTreeMap<ObjectID, Object>,
    ) {
        checkpoint_only_store("insert_executed_transaction")
    }

    fn insert_transaction(&mut self, _transaction: VerifiedTransaction) {
        checkpoint_only_store("insert_transaction")
    }

    fn insert_transaction_effects(&mut self, _effects: TransactionEffects) {
        checkpoint_only_store("insert_transaction_effects")
    }

    fn insert_events(&mut self, _tx_digest: &TransactionDigest, _events: TransactionEvents) {
        checkpoint_only_store("insert_events")
    }

    fn update_objects(
        &mut self,
        _written_objects: BTreeMap<ObjectID, Object>,
        _deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        checkpoint_only_store("update_objects")
    }

    fn backing_store(&self) -> &dyn sui_types::storage::BackingStore {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use forking_data_store::{ObjectStoreWriter, VersionQuery};
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use sui_types::{
        base_types::SuiAddress, digests::get_mainnet_chain_identifier,
        message_envelope::Message as _, object::Owner, storage::ObjectStore,
    };
    use tempfile::tempdir;

    use crate::test_utils::{filesystem_store, forking_store, test_object};

    fn test_checkpoint(sequence: u64, epoch: u64) -> CheckpointData {
        TestCheckpointBuilder::new(sequence)
            .with_epoch(epoch)
            .build_checkpoint()
    }

    #[test]
    fn checkpoint_routing_uses_historical_before_fork_and_local_after_fork() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();

        let historical_fs = filesystem_store(
            historical_dir.path(),
            &chain_id,
        );
        let local_fs = filesystem_store(
            local_dir.path(),
            &chain_id,
        );
        let historical_checkpoint = test_checkpoint(7, 2);
        let local_checkpoint = test_checkpoint(11, 2);
        let historical_digest = historical_checkpoint.summary.data().digest();
        let local_digest = local_checkpoint.summary.data().digest();

        historical_fs
            .write_checkpoint(&historical_checkpoint)
            .unwrap();
        local_fs.write_checkpoint(&local_checkpoint).unwrap();

        let historical_store = forking_store(historical_dir.path(), &chain_id);
        let local_store = forking_store(local_dir.path(), &chain_id);
        let store = ServiceStore::new(10, historical_store, local_store);

        assert_eq!(
            store
                .checkpoint_by_sequence_number(7)
                .unwrap()
                .sequence_number(),
            &7
        );
        assert_eq!(
            store
                .checkpoint_by_sequence_number(11)
                .unwrap()
                .sequence_number(),
            &11
        );
        assert_eq!(
            store
                .checkpoint_by_digest(&historical_digest)
                .unwrap()
                .sequence_number(),
            &7
        );
        assert_eq!(
            store
                .checkpoint_by_digest(&local_digest)
                .unwrap()
                .sequence_number(),
            &11
        );
        assert_eq!(store.highest_checkpoint().unwrap().sequence_number(), &11);
    }

    #[test]
    fn checkpoint_inserts_persist_only_to_the_local_store() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();
        let historical_store = forking_store(historical_dir.path(), &chain_id);
        let local_store = forking_store(local_dir.path(), &chain_id);
        let mut store = ServiceStore::new(10, historical_store, local_store);
        let checkpoint = test_checkpoint(12, 3);
        let verified_checkpoint = VerifiedCheckpoint::new_unchecked(checkpoint.summary.clone());
        let contents = checkpoint.contents.clone();

        store.insert_checkpoint_summary(verified_checkpoint);
        store.insert_checkpoint_contents(contents);

        let historical_fs = filesystem_store(
            historical_dir.path(),
            &chain_id,
        );
        let local_fs = filesystem_store(
            local_dir.path(),
            &chain_id,
        );

        assert!(
            historical_fs
                .get_checkpoint_by_sequence_number(12)
                .unwrap()
                .is_none()
        );
        assert!(
            local_fs
                .get_checkpoint_by_sequence_number(12)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn get_object_prefers_local_latest_object_over_historical_fork_snapshot() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();
        let object_id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();

        let historical_store = forking_store(historical_dir.path(), &chain_id);
        historical_store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::AtCheckpoint(10),
                },
                test_object(object_id, owner, 1),
                1,
            )
            .unwrap();

        let local_store = forking_store(local_dir.path(), &chain_id);
        local_store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::Version(3),
                },
                test_object(object_id, owner, 3),
                3,
            )
            .unwrap();

        let store = ServiceStore::new(10, historical_store, local_store);
        assert_eq!(
            ObjectStore::get_object(&store, &object_id)
                .unwrap()
                .version()
                .value(),
            3
        );
    }

    #[test]
    fn get_object_by_key_fetches_historical_version_only_when_visible_at_fork() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();
        let object_id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();

        let historical_store = forking_store(historical_dir.path(), &chain_id);
        historical_store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::Version(10),
                },
                test_object(object_id, owner, 10),
                10,
            )
            .unwrap();
        historical_store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::AtCheckpoint(100),
                },
                test_object(object_id, owner, 15),
                15,
            )
            .unwrap();

        let store = ServiceStore::new(
            100,
            historical_store,
            forking_store(local_dir.path(), &chain_id),
        );
        assert_eq!(
            store
                .get_object_by_key(&object_id, SequenceNumber::from_u64(10))
                .unwrap()
                .version()
                .value(),
            10
        );
    }

    #[test]
    fn get_object_by_key_rejects_versions_created_after_the_fork_checkpoint() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();
        let object_id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();

        let historical_store = forking_store(historical_dir.path(), &chain_id);
        historical_store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::Version(25),
                },
                test_object(object_id, owner, 25),
                25,
            )
            .unwrap();
        historical_store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::AtCheckpoint(100),
                },
                test_object(object_id, owner, 15),
                15,
            )
            .unwrap();

        let store = ServiceStore::new(
            100,
            historical_store,
            forking_store(local_dir.path(), &chain_id),
        );
        assert!(
            store
                .get_object_by_key(&object_id, SequenceNumber::from_u64(25))
                .is_none()
        );
    }

    #[test]
    fn get_object_by_key_returns_local_post_fork_versions() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();
        let object_id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();

        let local_store = forking_store(local_dir.path(), &chain_id);
        local_store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::Version(22),
                },
                test_object(object_id, owner, 22),
                22,
            )
            .unwrap();

        let store = ServiceStore::new(
            100,
            forking_store(historical_dir.path(), &chain_id),
            local_store,
        );
        assert_eq!(
            store
                .get_object_by_key(&object_id, SequenceNumber::from_u64(22))
                .unwrap()
                .version()
                .value(),
            22
        );
    }

    #[test]
    fn get_object_received_at_version_uses_exact_version_instead_of_latest_object() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();
        let receiving_object_id = ObjectID::random();
        let owner_id = ObjectID::random();

        let historical_store = forking_store(historical_dir.path(), &chain_id);
        historical_store
            .write_object(
                &ObjectKey {
                    object_id: receiving_object_id,
                    version_query: VersionQuery::Version(10),
                },
                Object::with_id_owner_version_for_testing(
                    receiving_object_id,
                    SequenceNumber::from_u64(10),
                    Owner::AddressOwner(owner_id.into()),
                ),
                10,
            )
            .unwrap();
        historical_store
            .write_object(
                &ObjectKey {
                    object_id: receiving_object_id,
                    version_query: VersionQuery::AtCheckpoint(100),
                },
                Object::with_id_owner_version_for_testing(
                    receiving_object_id,
                    SequenceNumber::from_u64(15),
                    Owner::AddressOwner(owner_id.into()),
                ),
                15,
            )
            .unwrap();

        let local_store = forking_store(local_dir.path(), &chain_id);
        local_store
            .write_object(
                &ObjectKey {
                    object_id: receiving_object_id,
                    version_query: VersionQuery::Version(20),
                },
                Object::with_id_owner_version_for_testing(
                    receiving_object_id,
                    SequenceNumber::from_u64(20),
                    Owner::AddressOwner(owner_id.into()),
                ),
                20,
            )
            .unwrap();

        let store = ServiceStore::new(100, historical_store, local_store);
        assert_eq!(
            store
                .get_object_received_at_version(
                    &owner_id,
                    &receiving_object_id,
                    SequenceNumber::from_u64(10),
                    0,
                )
                .unwrap()
                .unwrap()
                .version()
                .value(),
            10
        );
    }

    #[test]
    fn read_child_object_uses_root_version_history_and_validates_parent() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();
        let parent_id = ObjectID::random();
        let child_id = ObjectID::random();
        let child = Object::with_id_owner_version_for_testing(
            child_id,
            SequenceNumber::from_u64(5),
            Owner::ObjectOwner(parent_id.into()),
        );

        let historical_store = forking_store(historical_dir.path(), &chain_id);
        historical_store
            .write_object(
                &ObjectKey {
                    object_id: child_id,
                    version_query: VersionQuery::RootVersion(5),
                },
                child,
                5,
            )
            .unwrap();

        let store = ServiceStore::new(
            100,
            historical_store,
            forking_store(local_dir.path(), &chain_id),
        );
        assert_eq!(
            store
                .read_child_object(&parent_id, &child_id, SequenceNumber::from_u64(5))
                .unwrap()
                .unwrap()
                .version()
                .value(),
            5
        );
    }
}
