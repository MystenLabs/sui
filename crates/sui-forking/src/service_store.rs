// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use forking_data_store::{CheckpointStore, CheckpointStoreWriter};
use simulacrum::SimulatorStore;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    clock::Clock,
    committee::{Committee, EpochId},
    digests::{CheckpointContentsDigest, CheckpointDigest, ObjectDigest, TransactionDigest},
    effects::{TransactionEffects, TransactionEvents},
    error::SuiResult,
    full_checkpoint_content::{Checkpoint as CheckpointData, ObjectSet},
    messages_checkpoint::{CheckpointContents, CheckpointSequenceNumber, VerifiedCheckpoint},
    object::Object,
    storage::{BackingPackageStore, ChildObjectResolver, PackageObject, ParentSync},
    sui_system_state::SuiSystemState,
    transaction::VerifiedTransaction,
};

/// Persistent checkpoint adapter backing a forked network.
///
/// `ServiceStore` keeps two composed `ForkingStore` values:
/// - `historical_store` serves checkpoint reads up to and including the fork point
/// - `local_store` serves post-fork checkpoint reads and all checkpoint writes
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
    /// Create a checkpoint-only service store over historical and local `ForkingStore` values.
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

#[cold]
fn checkpoint_only_store(method: &str) -> ! {
    todo!("{method} is not implemented for the PR3 checkpoint-only ServiceStore")
}

impl<H, L> BackingPackageStore for ServiceStore<H, L>
where
    H: CheckpointStore,
    L: CheckpointStore + CheckpointStoreWriter,
{
    fn get_package_object(&self, _package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        checkpoint_only_store("get_package_object")
    }
}

impl<H, L> ChildObjectResolver for ServiceStore<H, L>
where
    H: CheckpointStore,
    L: CheckpointStore + CheckpointStoreWriter,
{
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        _child: &ObjectID,
        _child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        checkpoint_only_store("read_child_object")
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        checkpoint_only_store("get_object_received_at_version")
    }
}

impl<H, L> sui_types::storage::ObjectStore for ServiceStore<H, L>
where
    H: CheckpointStore,
    L: CheckpointStore + CheckpointStoreWriter,
{
    fn get_object(&self, _object_id: &ObjectID) -> Option<Object> {
        checkpoint_only_store("get_object")
    }

    fn get_object_by_key(&self, _object_id: &ObjectID, _version: SequenceNumber) -> Option<Object> {
        checkpoint_only_store("get_object_by_key")
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
    H: CheckpointStore,
    L: CheckpointStore + CheckpointStoreWriter,
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

    fn get_object(&self, _id: &ObjectID) -> Option<Object> {
        checkpoint_only_store("SimulatorStore::get_object")
    }

    fn get_object_at_version(&self, _id: &ObjectID, _version: SequenceNumber) -> Option<Object> {
        checkpoint_only_store("get_object_at_version")
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

    use forking_data_store::{
        SetupStore,
        stores::{FileSystemStore, ForkingStore},
    };
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use sui_types::{digests::get_mainnet_chain_identifier, message_envelope::Message as _};
    use tempfile::tempdir;

    fn test_checkpoint(sequence: u64, epoch: u64) -> CheckpointData {
        TestCheckpointBuilder::new(sequence)
            .with_epoch(epoch)
            .build_checkpoint()
    }

    fn checkpoint_store(
        node: forking_data_store::Node,
        root: &std::path::Path,
        chain_id: &str,
    ) -> FileSystemStore {
        let store = FileSystemStore::new_with_path(node, root.to_path_buf()).unwrap();
        store.setup(Some(chain_id.to_string())).unwrap();
        store
    }

    #[test]
    fn checkpoint_routing_uses_historical_before_fork_and_local_after_fork() {
        let historical_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();
        let chain_id = get_mainnet_chain_identifier().to_string();

        let historical_fs = checkpoint_store(
            forking_data_store::Node::Mainnet,
            historical_dir.path(),
            &chain_id,
        );
        let local_fs = checkpoint_store(
            forking_data_store::Node::Mainnet,
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

        let historical_store = ForkingStore::new(
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                historical_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                historical_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                historical_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                historical_dir.path(),
                &chain_id,
            ),
        );
        let local_store = ForkingStore::new(
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                local_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                local_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                local_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                local_dir.path(),
                &chain_id,
            ),
        );
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
        let historical_store = ForkingStore::new(
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                historical_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                historical_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                historical_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                historical_dir.path(),
                &chain_id,
            ),
        );
        let local_store = ForkingStore::new(
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                local_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                local_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                local_dir.path(),
                &chain_id,
            ),
            checkpoint_store(
                forking_data_store::Node::Mainnet,
                local_dir.path(),
                &chain_id,
            ),
        );
        let mut store = ServiceStore::new(10, historical_store, local_store);
        let checkpoint = test_checkpoint(12, 3);
        let verified_checkpoint = VerifiedCheckpoint::new_unchecked(checkpoint.summary.clone());
        let contents = checkpoint.contents.clone();

        store.insert_checkpoint_summary(verified_checkpoint);
        store.insert_checkpoint_contents(contents);

        let historical_fs = checkpoint_store(
            forking_data_store::Node::Mainnet,
            historical_dir.path(),
            &chain_id,
        );
        let local_fs = checkpoint_store(
            forking_data_store::Node::Mainnet,
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
}
