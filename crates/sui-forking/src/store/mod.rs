// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};

use simulacrum::SimulatorStore;
use sui_types::transaction::{SenderSignedData, Transaction};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    committee::{Committee, EpochId},
    digests::{ObjectDigest, TransactionDigest},
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
        VerifiedCheckpoint,
    },
    object::{Object, Owner},
    storage::{
        BackingPackageStore, ChildObjectResolver, ObjectStore, PackageObject, ParentSync,
        get_module, load_package_object_from_object_store,
    },
    transaction::VerifiedTransaction,
};

use sui_data_store::stores::{DataStore, FileSystemStore, ReadThroughStore};
use sui_data_store::{
    ObjectKey, ObjectStore as _, TransactionInfo, TransactionStore, TransactionStoreWriter,
    VersionQuery,
};
use sui_types::storage::ReadStore;

pub struct ForkingStore {
    // Checkpoint data
    checkpoints: BTreeMap<CheckpointSequenceNumber, VerifiedCheckpoint>,
    checkpoint_digest_to_sequence_number: HashMap<CheckpointDigest, CheckpointSequenceNumber>,
    checkpoint_contents: HashMap<CheckpointContentsDigest, CheckpointContents>,

    // // Transaction data
    // transactions: HashMap<TransactionDigest, VerifiedTransaction>,
    // effects: HashMap<TransactionDigest, TransactionEffects>,
    // events: HashMap<TransactionDigest, TransactionEvents>,

    // Committee data
    epoch_to_committee: Vec<Committee>,

    // // Object data
    // // for object versions and other data, we need the normal indexer grpc reader
    fs_store: FileSystemStore,
    object_store: ReadThroughStore<FileSystemStore, DataStore>,

    // // Fallback to RPC data store
    // rpc_data_store:
    //     Arc<ReadThroughStore<LruMemoryStore, ReadThroughStore<FileSystemStore, DataStore>>>,

    // The checkpoint at which this forked network was forked
    forked_at_checkpoint: u64,
}

impl ForkingStore {
    pub fn new(
        // genesis: &Genesis,
        forked_at_checkpoint: u64,
        fs_store: FileSystemStore,
        object_store: ReadThroughStore<FileSystemStore, DataStore>,
    ) -> Self {
        let store = Self::new_with_rpc_data_store_and_checkpoint(
            fs_store,
            object_store,
            forked_at_checkpoint,
        );

        // println!(
        //     "Genesis transaction digest: {:?}",
        //     genesis.transaction().digest()
        // );
        // store.init_with_genesis(genesis);
        store
    }

    pub(crate) fn object_store(&self) -> &ReadThroughStore<FileSystemStore, DataStore> {
        &self.object_store
    }

    fn new_with_rpc_data_store_and_checkpoint(
        fs_store: FileSystemStore,
        object_store: ReadThroughStore<FileSystemStore, DataStore>,
        forked_at_checkpoint: u64,
    ) -> Self {
        Self {
            checkpoints: BTreeMap::new(),
            checkpoint_digest_to_sequence_number: HashMap::new(),
            checkpoint_contents: HashMap::new(),
            epoch_to_committee: vec![],
            fs_store,
            object_store,
            forked_at_checkpoint,
        }
    }

    pub fn get_objects(&self) -> &HashMap<ObjectID, BTreeMap<SequenceNumber, Object>> {
        println!("TODO fetching all objects is currently not supported in ForkingStore");
        todo!()
    }
    pub fn get_objects_by_keys(&self, _object_keys: &[ObjectKey]) -> &HashMap<ObjectID, Object> {
        println!("TODO fetching objects by keys is currently not supported in ForkingStore");
        todo!()
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<&VerifiedCheckpoint> {
        self.checkpoints.get(&sequence_number)
    }

    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Option<&VerifiedCheckpoint> {
        self.checkpoint_digest_to_sequence_number
            .get(digest)
            .and_then(|sequence_number| self.get_checkpoint_by_sequence_number(*sequence_number))
    }

    pub fn get_highest_checkpint(&self) -> Option<&VerifiedCheckpoint> {
        self.checkpoints
            .last_key_value()
            .map(|(_, checkpoint)| checkpoint)
    }

    pub fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<&CheckpointContents> {
        self.checkpoint_contents.get(digest)
    }

    pub fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<&Committee> {
        self.epoch_to_committee.get(epoch as usize)
    }

    pub fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        let tx = self
            .fs_store
            .transaction_data_and_effects(&digest.to_string())
            .unwrap();

        let tx = match tx {
            None => return None,
            Some(tx_info) => {
                let sender_signed_data = SenderSignedData::new(tx_info.data, vec![]).clone();
                let tx = Transaction::new(sender_signed_data);
                let verified_tx = VerifiedTransaction::new_unchecked(tx);
                verified_tx
            }
        };

        Some(tx)
    }

    pub fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<TransactionEffects> {
        let tx = self
            .fs_store
            .transaction_data_and_effects(&digest.to_string())
            .unwrap();

        tx.map(|tx_info| tx_info.effects)
    }

    pub fn get_transaction_events(
        &self,
        _digest: &TransactionDigest,
    ) -> Option<&TransactionEvents> {
        println!(
            "TODO fetching transaction events is currently not supported in ForkingStore, and it \
            retursn None"
        );
        None
        // println!("Fetching events for transaction digest: {:?}", digest);
        // todo!()
    }

    /// Tries to fetch the object at the latest version, and if not found, it will fetch it from
    /// RPC at the forked checkpoint.
    pub fn get_object(&self, id: &ObjectID) -> Option<Object> {
        // fetch object at latest version from the primary cache (FileSystem).
        let object = self.fs_store.get_object_latest(id).unwrap();

        if let Some((obj, _)) = object {
            return Some(obj);
        }

        // if object does not exist, then fetch it at forked checkpoint. The object store will
        // first try in the primary cache (FileSystemStore) and then fallback to the RPC data store
        // (DataStore) if not found, and it will be written back to the primary cache for future
        // reads.
        let objects = self
            .object_store
            .get_objects(&[ObjectKey {
                object_id: *id,
                version_query: sui_data_store::VersionQuery::AtCheckpoint(
                    self.forked_at_checkpoint,
                ),
            }])
            .unwrap();

        let first = objects.first().and_then(|opt| opt.as_ref());
        first.map(|(obj, _)| obj.clone())
    }

    pub fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        let objects = self
            .object_store
            .get_objects(&[ObjectKey {
                object_id: *id,
                version_query: sui_data_store::VersionQuery::Version(version.into()),
            }])
            .unwrap();
        let first = objects.first().and_then(|opt| opt.as_ref());

        first.map(|(obj, _)| obj.clone())
    }

    pub fn get_system_state(&self) -> sui_types::sui_system_state::SuiSystemState {
        todo!()
    }

    pub fn get_clock(&self) -> sui_types::clock::Clock {
        self.get_object(&sui_types::SUI_CLOCK_OBJECT_ID)
            .expect("clock should exist")
            .to_rust()
            .expect("clock object should deserialize")
    }

    pub fn owned_objects(&self, owner: SuiAddress) -> Vec<Object> {
        println!("Fetching owned objects for address: {:?}", owner);
        let objects = self.fs_store.get_objects_by_owner(owner).unwrap();
        objects
    }
}

impl ForkingStore {
    pub fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        if let Some(end_of_epoch_data) = &checkpoint.data().end_of_epoch_data {
            let next_committee = end_of_epoch_data
                .next_epoch_committee
                .iter()
                .cloned()
                .collect();
            let committee =
                Committee::new(checkpoint.epoch().checked_add(1).unwrap(), next_committee);
            self.insert_committee(committee);
        }

        self.checkpoint_digest_to_sequence_number
            .insert(*checkpoint.digest(), *checkpoint.sequence_number());
        self.checkpoints
            .insert(*checkpoint.sequence_number(), checkpoint);
    }

    pub fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        self.checkpoint_contents
            .insert(*contents.digest(), contents);
    }

    pub fn insert_committee(&mut self, committee: Committee) {
        let epoch = committee.epoch as usize;

        if self.epoch_to_committee.get(epoch).is_some() {
            return;
        }

        if self.epoch_to_committee.len() == epoch {
            self.epoch_to_committee.push(committee);
        } else {
            panic!("committee was inserted into EpochCommitteeMap out of order");
        }
    }

    pub fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        _events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        let tx_digest = effects.transaction_digest().to_string();
        let tx_info = TransactionInfo {
            data: transaction.data().inner().intent_message().value.clone(),
            effects,
            checkpoint: self.get_latest_checkpoint_sequence_number().unwrap(),
        };
        self.fs_store
            .write_transaction(&tx_digest, tx_info)
            .unwrap();

        let objects = written_objects
            .into_iter()
            .map(|(id, object)| {
                let version = object.version().into();
                let key = ObjectKey {
                    object_id: id,
                    version_query: sui_data_store::VersionQuery::Version(version),
                };
                (key, object, version)
            })
            .collect();

        self.object_store.write_objects(objects).unwrap();
    }

    pub fn insert_transaction(&mut self, _transaction: VerifiedTransaction) {
        todo!()
    }

    pub fn insert_transaction_effects(&mut self, _effects: TransactionEffects) {
        todo!()
    }

    pub fn insert_events(&mut self, _tx_digest: &TransactionDigest, _events: TransactionEvents) {
        todo!()
    }

    pub fn update_objects(
        &mut self,
        _written_objects: BTreeMap<ObjectID, Object>,
        _deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        todo!()
    }
}

impl BackingPackageStore for ForkingStore {
    fn get_package_object(
        &self,
        package_id: &ObjectID,
    ) -> sui_types::error::SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

impl ChildObjectResolver for ForkingStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        // let child_object = match self.get_object(child) {
        //     None => return Ok(None),
        //     Some(obj) => obj,
        // };
        //
        // let parent = *parent;
        // if child_object.owner != Owner::ObjectOwner(parent.into()) {
        //     return Err(SuiErrorKind::InvalidChildObjectAccess {
        //         object: *child,
        //         given_parent: parent,
        //         actual_owner: child_object.owner.clone(),
        //     }
        //     .into());
        // }

        println!(
            "Reading child object: {:?} of parent: {:?} with version upper bound: {:?}",
            child, parent, child_version_upper_bound
        );

        let object_key = ObjectKey {
            object_id: *child,
            version_query: VersionQuery::RootVersion(child_version_upper_bound.value()),
        };
        let object = self.object_store.get_objects(&[object_key]).unwrap();
        debug_assert!(object.len() == 1, "Expected one object for {}", child,);
        let object = object
            .into_iter()
            .next()
            .unwrap()
            .map(|(obj, _version)| obj);

        println!(
            "Found object {:?} for child: {:?} of parent: {:?}",
            object, child, parent
        );

        Ok(object)

        // println!("Reading child object: {:?} of parent: {:?}", child, parent);
        // let child_object = match self.get_object(child) {
        //     None => return Ok(None),
        //     Some(obj) => obj,
        // };
        //
        // let parent = *parent;
        // if child_object.owner != Owner::ObjectOwner(parent.into()) {
        //     return Err(SuiErrorKind::InvalidChildObjectAccess {
        //         object: *child,
        //         given_parent: parent,
        //         actual_owner: child_object.owner.clone(),
        //     }
        //     .into());
        // }
        //
        // println!(
        //     "Child object version: {:?}, upper bound: {:?}",
        //     child_object.version(),
        //     child_version_upper_bound
        // );
        //
        // // TODO: NO IDEA IF THIS IS CORRECT!
        // if child_object.version() > child_version_upper_bound {
        //     let id = child_object.id();
        //     let child_object = self
        //         .object_store
        //         .get_objects(&[sui_data_store::ObjectKey {
        //             object_id: id,
        //             version_query: sui_data_store::VersionQuery::AtCheckpoint(
        //                 self.forked_at_checkpoint,
        //             ),
        //         }])
        //         .unwrap();
        //
        //     let first = child_object
        //         .first()
        //         .and_then(|opt| opt.as_ref())
        //         .map(|(obj, _)| obj.clone());
        //
        //     return Ok(first);
        //
        //     // return Err(SuiErrorKind::UnsupportedFeatureError {
        //     //     error: "TODO InMemoryStorage::read_child_object does not yet support bounded reads"
        //     //         .to_owned(),
        //     // }
        //     // .into());
        // }
        //
        // Ok(Some(child_object))
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        let recv_object = match self.get_object(receiving_object_id) {
            None => return Ok(None),
            Some(obj) => obj,
        };
        if recv_object.owner != Owner::AddressOwner((*owner).into()) {
            return Ok(None);
        }

        if recv_object.version() != receive_object_at_version {
            return Ok(None);
        }
        Ok(Some(recv_object))
    }
}

impl GetModule for ForkingStore {
    type Error = anyhow::Error;
    type Item = Arc<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        let module = self
            .get_module(id)?
            .map(|bytes| CompiledModule::deserialize_with_defaults(&bytes).unwrap());

        Ok(module.map(Arc::new))
    }
}

impl ModuleResolver for ForkingStore {
    type Error = anyhow::Error;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        get_module(self, module_id).map_err(|e| anyhow::anyhow!(e.to_string()))
    }
}

impl ObjectStore for ForkingStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<Object> {
        self.get_object_at_version(object_id, version)
    }
}

impl ParentSync for ForkingStore {
    fn get_latest_parent_entry_ref_deprecated(
        &self,
        _object_id: ObjectID,
    ) -> Option<sui_types::base_types::ObjectRef> {
        panic!("Never called in newer protocol versions")
    }
}

impl SimulatorStore for ForkingStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.get_checkpoint_by_sequence_number(sequence_number)
            .cloned()
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.get_checkpoint_by_digest(digest).cloned()
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        self.get_highest_checkpint().cloned()
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.get_checkpoint_contents(digest).cloned()
    }

    fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<Committee> {
        self.get_committee_by_epoch(epoch).cloned()
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        self.get_transaction(digest)
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.get_transaction_effects(digest)
    }

    fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.get_transaction_events(digest).cloned()
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        self.get_object(id)
    }

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.get_object_at_version(id, version)
    }

    fn get_system_state(&self) -> sui_types::sui_system_state::SuiSystemState {
        self.get_system_state()
    }

    fn get_clock(&self) -> sui_types::clock::Clock {
        self.get_clock()
    }

    fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        Box::new(self.owned_objects(owner).into_iter())
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        self.insert_checkpoint(checkpoint)
    }

    fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        self.insert_checkpoint_contents(contents)
    }

    fn insert_committee(&mut self, committee: Committee) {
        self.insert_committee(committee)
    }

    fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        self.insert_executed_transaction(transaction, effects, events, written_objects)
    }

    fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        self.insert_transaction(transaction)
    }

    fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        self.insert_transaction_effects(effects)
    }

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        self.insert_events(tx_digest, events)
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        self.update_objects(written_objects, deleted_objects)
    }

    fn backing_store(&self) -> &dyn sui_types::storage::BackingStore {
        self
    }
}

// #[async_trait]
// impl ObjectProvider for ForkingStore {
//     type Error = anyhow::Error;
//     async fn get_object(
//         &self,
//         id: &ObjectID,
//         version: &SequenceNumber,
//     ) -> Result<Object, Self::Error> {
//         match self.get_object_at_version(id, *version) {
//             Some(obj) => Ok(obj.clone()),
//             None => Err(anyhow::anyhow!(
//                 "Object {:?} at version {:?} not found",
//                 id,
//                 version
//             )),
//         }
//     }
//
//     async fn find_object_lt_or_eq_version(
//         &self,
//         id: &ObjectID,
//         version: &SequenceNumber,
//     ) -> Result<Option<Object>, Self::Error> {
//         match self.get_object(id) {
//             Some(obj) if obj.version() <= *version => Ok(Some(obj.clone())),
//             _ => Ok(None),
//         }
//     }
// }

impl ReadStore for ForkingStore {
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.get_committee_by_epoch(epoch).cloned().map(Arc::new)
    }

    fn get_latest_checkpoint(&self) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.get_highest_checkpint()
            .cloned()
            .ok_or_else(|| sui_types::storage::error::Error::custom("No checkpoint available"))
    }

    fn get_highest_verified_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.get_latest_checkpoint()
    }

    fn get_highest_synced_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.get_latest_checkpoint()
    }

    fn get_lowest_available_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<CheckpointSequenceNumber> {
        Ok(0)
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_checkpoint_by_digest(self, digest).cloned()
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_checkpoint_by_sequence_number(self, sequence_number).cloned()
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.get_checkpoint_contents(digest).cloned()
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        let checkpoint = self.get_checkpoint_by_sequence_number(sequence_number)?;
        self.get_checkpoint_contents(&checkpoint.content_digest)
            .cloned()
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.get_transaction(tx_digest).map(Arc::new)
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.get_transaction_effects(tx_digest)
    }

    fn get_events(&self, tx_digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.get_transaction_events(tx_digest).cloned()
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        _digest: &TransactionDigest,
    ) -> Option<Vec<sui_types::storage::ObjectKey>> {
        // Not tracked in forking store
        None
    }

    fn get_transaction_checkpoint(
        &self,
        _digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        // Transaction-to-checkpoint mapping not tracked in forking store
        None
    }

    fn get_full_checkpoint_contents(
        &self,
        _sequence_number: Option<CheckpointSequenceNumber>,
        _digest: &CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::VersionedFullCheckpointContents> {
        // Full checkpoint contents not tracked in forking store
        None
    }
}
