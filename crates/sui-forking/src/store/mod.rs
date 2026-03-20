// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use tracing::{error, warn};

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};

use simulacrum::SimulatorStore;
use sui_data_store::stores::{
    CompositeStore, DataStore, FileSystemStore, InMemoryStore, ReadThroughStore, WriteThroughStore,
};
use sui_data_store::{
    CheckpointStore as _, CheckpointStoreWriter as _, FullCheckpointData, ObjectKey,
    ObjectStore as _, ObjectStoreWriter as _, TransactionInfo, TransactionStore,
    TransactionStoreWriter, VersionQuery,
};
use sui_types::SUI_CLOCK_OBJECT_ID;
use sui_types::clock::Clock;
use sui_types::error::SuiErrorKind;
use sui_types::message_envelope::Envelope;
use sui_types::storage::ReadStore;
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorSetV1;
use sui_types::sui_system_state::{SuiSystemState, get_sui_system_state};
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

pub(crate) type HotMemFs = WriteThroughStore<Arc<InMemoryStore>, Arc<FileSystemStore>>;
pub(crate) type DiskThenGraphqlObjects = ReadThroughStore<Arc<FileSystemStore>, Arc<DataStore>>;
pub(crate) type HotObjects = WriteThroughStore<Arc<InMemoryStore>, Arc<DiskThenGraphqlObjects>>;
pub(crate) type ForkDataStore =
    CompositeStore<Arc<HotMemFs>, Arc<HotMemFs>, Arc<HotObjects>, Arc<HotMemFs>>;

pub struct ForkingStore {
    // Transaction events not available through fs transaction file blobs.
    // TODO: add events to FS transaction blobs and remove this in-memory cache.
    events: HashMap<TransactionDigest, TransactionEvents>,

    // Committee data
    epoch_to_committee: BTreeMap<EpochId, Committee>,

    /// Bare file system handle used for filesystem-specific helpers such as latest-object lookups
    /// and owner scans that are not part of the generic `sui-data-store` traits.
    filesystem: Arc<FileSystemStore>,

    /// Capability-routed composite store:
    /// - transactions/epochs/checkpoints: memory -> filesystem
    /// - objects: memory -> filesystem -> GraphQL
    store: ForkDataStore,

    // The checkpoint at which this forked network was forked
    forked_at_checkpoint: u64,

    /// Optional validator-set override used when building epoch state for checkpoint production.
    /// This keeps the committee aligned with locally available validator keys in forking mode.
    validator_set_override: Option<ValidatorSetV1>,

    // Simulacrum inserts checkpoint summary and contents in two separate calls.
    // Keep the summary only until contents arrives so we can persist one full checkpoint payload.
    pending_checkpoint: Option<VerifiedCheckpoint>,
}

impl ForkingStore {
    /// Creates a forking store with local cache/store chains already composed.
    pub fn new(
        forked_at_checkpoint: u64,
        filesystem: Arc<FileSystemStore>,
        store: ForkDataStore,
    ) -> Self {
        Self {
            events: HashMap::new(),
            epoch_to_committee: BTreeMap::new(),
            filesystem,
            store,
            forked_at_checkpoint,
            validator_set_override: None,
            pending_checkpoint: None,
        }
    }

    /// Converts full checkpoint payload data into a verified checkpoint summary envelope.
    fn verified_checkpoint_from_full_checkpoint_data(
        checkpoint: &FullCheckpointData,
    ) -> VerifiedCheckpoint {
        let certified_summary = checkpoint.summary.clone();
        let envelope = Envelope::new_from_data_and_sig(
            certified_summary.data().clone(),
            certified_summary.auth_sig().clone(),
        );
        VerifiedCheckpoint::new_unchecked(envelope)
    }

    /// Loads full checkpoint payload by sequence from the local memory/filesystem path.
    pub fn get_full_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<FullCheckpointData> {
        self.store
            .get_checkpoint_by_sequence_number(sequence_number)
            .ok()
            .flatten()
    }

    /// Returns checkpoint summary by sequence from the local memory/filesystem path.
    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.get_full_checkpoint_by_sequence_number(sequence_number)
            .map(|checkpoint| Self::verified_checkpoint_from_full_checkpoint_data(&checkpoint))
    }

    /// Returns checkpoint summary by digest via local digest index and sequence read.
    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Option<VerifiedCheckpoint> {
        let sequence_number = self
            .store
            .get_sequence_by_checkpoint_digest(digest)
            .ok()
            .flatten()?;
        self.get_checkpoint_by_sequence_number(sequence_number)
    }

    /// Returns checkpoint contents by sequence from the local memory/filesystem path.
    pub fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        self.get_full_checkpoint_by_sequence_number(sequence_number)
            .map(|checkpoint| checkpoint.contents)
    }

    /// Returns checkpoint contents by digest via local digest index and sequence read.
    pub fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        let sequence_number = self
            .store
            .get_sequence_by_contents_digest(digest)
            .ok()
            .flatten()?;
        self.get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    /// Returns the latest locally available checkpoint summary from the local checkpoint path.
    pub fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        let full_checkpoint = self.store.get_latest_checkpoint().ok().flatten()?;
        Some(Self::verified_checkpoint_from_full_checkpoint_data(
            &full_checkpoint,
        ))
    }

    /// Returns committee metadata for an epoch, if known in-memory.
    pub fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<&Committee> {
        self.epoch_to_committee.get(&epoch)
    }

    /// Returns the transaction by digest from the local transaction path.
    pub fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        let tx = match self.store.transaction_data_and_effects(&digest.to_string()) {
            Ok(tx) => tx,
            Err(err) => {
                error!(
                    transaction_digest = %digest,
                    "failed to read transaction data/effects from local store: {err}"
                );
                return None;
            }
        };

        let tx = match tx {
            None => return None,
            Some(tx_info) => {
                let sender_signed_data = SenderSignedData::new(tx_info.data, vec![]).clone();
                let tx = Transaction::new(sender_signed_data);
                VerifiedTransaction::new_unchecked(tx)
            }
        };

        Some(tx)
    }

    /// Returns transaction effects by digest from the local transaction path.
    pub fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<TransactionEffects> {
        let tx = match self.store.transaction_data_and_effects(&digest.to_string()) {
            Ok(tx) => tx,
            Err(err) => {
                error!(
                    transaction_digest = %digest,
                    "failed to read transaction effects from local store: {err}"
                );
                return None;
            }
        };

        tx.map(|tx_info| tx_info.effects)
    }

    /// Returns in-memory transaction events by transaction digest.
    pub fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<&TransactionEvents> {
        self.events.get(digest)
    }

    /// Tries to fetch the object at the latest version, and if not found, it will fetch it from
    /// RPC at the forked checkpoint.
    pub fn get_object(&self, id: &ObjectID) -> Option<Object> {
        // fetch object at latest version from the primary cache (FileSystem).
        let object = match self.filesystem.get_object_latest(id) {
            Ok(object) => object,
            Err(err) => {
                error!(
                    object_id = %id,
                    "failed to read latest object from local filesystem store: {err}"
                );
                None
            }
        };

        if let Some((obj, _)) = object {
            return Some(obj);
        }

        // if object does not exist, then fetch it at forked checkpoint. The object store will
        // first try in the memory/filesystem cache chain and then fallback to GraphQL if not
        // found, persisting the result into the local stores for future reads.
        let objects = match self.store.get_objects(&[ObjectKey {
            object_id: *id,
            version_query: sui_data_store::VersionQuery::AtCheckpoint(self.forked_at_checkpoint),
        }]) {
            Ok(objects) => objects,
            Err(err) => {
                error!(
                    object_id = %id,
                    checkpoint = self.forked_at_checkpoint,
                    "failed to fetch object at fork checkpoint via read-through store: {err}"
                );
                return None;
            }
        };

        let first = objects.first().and_then(|opt| opt.as_ref());
        first.map(|(obj, _)| obj.clone())
    }

    /// Returns an object at an exact version using read-through object fetch.
    pub fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        let objects = match self.store.get_objects(&[ObjectKey {
            object_id: *id,
            version_query: sui_data_store::VersionQuery::Version(version.into()),
        }]) {
            Ok(objects) => objects,
            Err(err) => {
                error!(
                    object_id = %id,
                    object_version = version.value(),
                    "failed to fetch object version via read-through store: {err}"
                );
                return None;
            }
        };
        let first = objects.first().and_then(|opt| opt.as_ref());

        first.map(|(obj, _)| obj.clone())
    }

    /// Gets the latest version of each object for the given keys, returning None for any missing
    /// objects.
    ///
    /// This uses the memory/filesystem/object-fallback path.
    pub fn get_objects(
        &self,
        keys: &[ObjectKey],
    ) -> Result<Vec<Option<(Object, u64)>>, anyhow::Error> {
        self.store.get_objects(keys)
    }

    /// Returns the current system state view derived from this store.
    /// Importantly, if `validator_set_override` is set, it will be used in place of the on-chain
    /// validator set for epoch state construction. This allows the forking store to keep the
    /// system state up-to-date with the locally available validator set.
    pub fn get_system_state(&self) -> SuiSystemState {
        let system_state = get_sui_system_state(self).expect("system state should exist");
        let Some(validators) = &self.validator_set_override else {
            return system_state;
        };

        match system_state {
            SuiSystemState::V1(mut inner) => {
                inner.validators = validators.clone();
                SuiSystemState::V1(inner)
            }
            SuiSystemState::V2(mut inner) => {
                inner.validators = validators.clone();
                SuiSystemState::V2(inner)
            }
            #[cfg(msim)]
            state @ (SuiSystemState::SimTestV1(_)
            | SuiSystemState::SimTestShallowV2(_)
            | SuiSystemState::SimTestDeepV2(_)) => state,
        }
    }

    /// Gets the clock object, which should always be present in the store since it's a system
    /// object. Panics if not found or fails to deserialize.
    pub fn get_clock(&self) -> Clock {
        self.get_object(&SUI_CLOCK_OBJECT_ID)
            .expect("clock should exist")
            .to_rust()
            .expect("clock object should deserialize")
    }

    /// Returns all locally cached objects currently owned by an address.
    pub fn owned_objects(&self, owner: SuiAddress) -> Vec<Object> {
        self.filesystem
            .get_objects_by_owner(owner)
            .unwrap_or_default()
    }

    /// Installs a validator-set override used by `get_system_state` for epoch-state derivation.
    pub fn set_system_state_validator_set_override(&mut self, validators: ValidatorSetV1) {
        self.validator_set_override = Some(validators);
    }
}

impl ForkingStore {
    /// Records checkpoint summary state and updates committee map on epoch transitions.
    /// The matching contents are expected in a later `insert_checkpoint_contents` call.
    pub fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        if let Some(end_of_epoch_data) = &checkpoint.data().end_of_epoch_data {
            let next_committee = end_of_epoch_data
                .next_epoch_committee
                .iter()
                .cloned()
                .collect();
            if let Some(next_epoch) = checkpoint.epoch().checked_add(1) {
                let committee = Committee::new(next_epoch, next_committee);
                self.insert_committee(committee);
            } else {
                warn!(
                    sequence_number = *checkpoint.sequence_number(),
                    current_epoch = checkpoint.epoch(),
                    "skipping committee insertion due to epoch overflow"
                );
            }
        }

        if let Some(previous_pending) = &self.pending_checkpoint {
            warn!(
                previous_sequence = *previous_pending.sequence_number(),
                next_sequence = *checkpoint.sequence_number(),
                "overwriting pending checkpoint before matching contents were inserted"
            );
        }
        self.pending_checkpoint = Some(checkpoint);
    }

    /// Completes a pending checkpoint and persists full checkpoint payload for post-fork sequences.
    pub fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        let Some(checkpoint) = self.pending_checkpoint.take() else {
            warn!("checkpoint contents inserted without a pending checkpoint summary");
            return;
        };

        if checkpoint.content_digest != *contents.digest() {
            warn!(
                sequence_number = *checkpoint.sequence_number(),
                "checkpoint content digest mismatch between summary and inserted contents"
            );
            return;
        }

        let sequence_number = *checkpoint.sequence_number();

        // The startup checkpoint is fetched directly through the checkpoint store read path.
        // Only persist checkpoints produced after the fork point.
        if sequence_number <= self.forked_at_checkpoint {
            return;
        }

        // Startup resume can begin from an already persisted post-fork checkpoint.
        // Avoid rewriting existing entries and duplicating digest index rows.
        match self
            .store
            .get_checkpoint_by_sequence_number(sequence_number)
        {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(err) => {
                error!(
                    sequence_number,
                    "failed to check for existing checkpoint before persistence: {err}"
                );
            }
        }

        match self.get_checkpoint_data(checkpoint, contents) {
            Ok(full_checkpoint) => {
                if let Err(err) = self.store.write_checkpoint(&full_checkpoint) {
                    error!(
                        sequence_number,
                        "failed to persist checkpoint to checkpoint store: {err}"
                    );
                }
            }
            Err(err) => {
                error!(
                    sequence_number,
                    "failed to build full checkpoint data for persistence: {err}"
                );
            }
        }
    }

    /// Inserts committee info for an epoch if not already present.
    pub fn insert_committee(&mut self, committee: Committee) {
        self.epoch_to_committee
            .entry(committee.epoch)
            .or_insert(committee);
    }

    /// Inserts the transaction, its effects, events, and the written objects into the store. The
    /// transaction and its effects are stored in the fs transaction file blobs, while the events
    /// and written objects are stored in memory in the ForkingStore since they are not available
    /// through the fs transaction file blobs and are needed for the execution of subsequent
    /// transactions in the forked network.
    pub fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        let transaction_digest = *effects.transaction_digest();
        let tx_digest = transaction_digest.to_string();
        let checkpoint_sequence = match self.get_latest_checkpoint_sequence_number() {
            Ok(sequence) => sequence,
            Err(err) => {
                error!(
                    transaction_digest = %transaction_digest,
                    "skipping transaction persistence because latest checkpoint is unavailable: {err}"
                );
                return;
            }
        };
        let tx_info = TransactionInfo {
            data: transaction.data().inner().intent_message().value.clone(),
            effects,
            checkpoint: checkpoint_sequence,
        };
        if let Err(err) = self.store.write_transaction(&tx_digest, tx_info) {
            error!(
                transaction_digest = %transaction_digest,
                "failed to persist transaction data/effects to local store: {err}"
            );
        }
        self.events.insert(transaction_digest, events);

        let objects = written_objects.into_iter().map(|(id, object)| {
            let version = object.version().into();
            let key = ObjectKey {
                object_id: id,
                version_query: sui_data_store::VersionQuery::Version(version),
            };
            (key, object, version)
        });

        if let Err(err) = self.persist_objects(objects) {
            error!(
                transaction_digest = %transaction_digest,
                "failed to persist written objects for executed transaction: {err}"
            );
        }
    }

    /// Placeholder for direct transaction insertion; currently unused in forking mode.
    pub fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        warn!(
            transaction_digest = %transaction.digest(),
            "insert_transaction is not implemented for ForkingStore; use insert_executed_transaction"
        );
    }

    /// Placeholder for direct effects insertion; currently unused in forking mode.
    pub fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        warn!(
            transaction_digest = %effects.transaction_digest(),
            "insert_transaction_effects is not implemented for ForkingStore; use insert_executed_transaction"
        );
    }

    /// Stores transaction events in-memory.
    pub fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        self.events.insert(*tx_digest, events);
    }

    /// Placeholder for object update path; currently unused.
    pub fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        _deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        let objects = written_objects.into_iter().map(|(id, object)| {
            let version = object.version().into();
            let key = ObjectKey {
                object_id: id,
                version_query: sui_data_store::VersionQuery::Version(version),
            };
            (key, object, version)
        });

        if let Err(err) = self.persist_objects(objects) {
            error!("failed to persist updated objects to local store: {err}");
        }
    }

    fn persist_objects<I>(&self, objects: I) -> Result<(), anyhow::Error>
    where
        I: IntoIterator<Item = (ObjectKey, Object, u64)>,
    {
        for (key, object, version) in objects {
            self.store
                .write_object(&key, object, version)
                .with_context(|| {
                    format!(
                        "failed to persist object {} at version {}",
                        key.object_id, version
                    )
                })?;
        }
        Ok(())
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
        let validate = |child_object: Object| -> sui_types::error::SuiResult<Option<Object>> {
            let parent = *parent;
            if child_object.owner != Owner::ObjectOwner(parent.into()) {
                return Err(SuiErrorKind::InvalidChildObjectAccess {
                    object: *child,
                    given_parent: parent,
                    actual_owner: child_object.owner.clone(),
                }
                .into());
            }

            if child_object.version() > child_version_upper_bound {
                return Err(SuiErrorKind::UnsupportedFeatureError {
                    error:
                        "TODO ForkingStore::read_child_object does not yet support bounded reads"
                            .to_owned(),
                }
                .into());
            }

            Ok(Some(child_object))
        };

        let local_latest = self
            .filesystem
            .get_object_latest(child)
            .map_err(|e| SuiErrorKind::Storage(e.to_string()))?;
        if let Some((child_object, _)) = &local_latest
            && child_object.version() <= child_version_upper_bound
        {
            return validate(child_object.clone());
        }

        let object_key = ObjectKey {
            object_id: *child,
            version_query: VersionQuery::RootVersion(child_version_upper_bound.value()),
        };
        let mut object = self
            .get_objects(&[object_key])
            .map_err(|e| SuiErrorKind::Storage(e.to_string()))?;
        debug_assert!(object.len() == 1, "Expected one object for {}", child);
        let object = object.pop().unwrap().map(|(obj, _version)| obj);

        match object {
            Some(obj) => validate(obj),
            None => {
                if local_latest.is_some() {
                    Err(SuiErrorKind::UnsupportedFeatureError {
                        error:
                            "TODO ForkingStore::read_child_object does not yet support bounded reads"
                                .to_owned(),
                    }
                    .into())
                } else {
                    Ok(None)
                }
            }
        }
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
            .map(|bytes| {
                CompiledModule::deserialize_with_defaults(&bytes)
                    .map_err(|err| anyhow!("failed to deserialize compiled module {id:?}: {err}"))
            })
            .transpose()?;

        Ok(module.map(Arc::new))
    }
}

impl ModuleResolver for ForkingStore {
    type Error = anyhow::Error;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        get_module(self, module_id).map_err(|e| anyhow!(e.to_string()))
    }

    fn get_packages_static<const N: usize>(
        &self,
        _ids: [move_core_types::account_address::AccountAddress; N],
    ) -> Result<[Option<move_core_types::resolver::SerializedPackage>; N], Self::Error> {
        todo!()
    }

    fn get_packages<'a>(
        &self,
        _ids: impl ExactSizeIterator<Item = &'a move_core_types::account_address::AccountAddress>,
    ) -> Result<Vec<Option<move_core_types::resolver::SerializedPackage>>, Self::Error> {
        todo!()
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
        ForkingStore::get_checkpoint_by_sequence_number(self, sequence_number)
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_checkpoint_by_digest(self, digest)
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_highest_checkpint(self)
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        ForkingStore::get_checkpoint_contents_by_digest(self, digest)
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

    fn get_system_state(&self) -> SuiSystemState {
        self.get_system_state()
    }

    fn get_clock(&self) -> Clock {
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

impl ReadStore for ForkingStore {
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.get_committee_by_epoch(epoch).cloned().map(Arc::new)
    }

    fn get_latest_checkpoint(&self) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.get_highest_checkpint()
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
        ForkingStore::get_checkpoint_by_digest(self, digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_checkpoint_by_sequence_number(self, sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        ForkingStore::get_checkpoint_contents_by_digest(self, digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        ForkingStore::get_checkpoint_contents_by_sequence_number(self, sequence_number)
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

#[cfg(test)]
mod tests {
    use sui_data_store::{
        CheckpointStoreWriter as _, Node, ObjectKey, ObjectStoreWriter as _, SetupStore as _,
        TransactionInfo, TransactionStoreWriter as _, VersionQuery,
    };
    use sui_types::{
        base_types::{ObjectID, SequenceNumber, SuiAddress},
        object::{Object, Owner},
        test_checkpoint_data_builder::TestCheckpointBuilder,
    };
    use tempfile::TempDir;

    use super::*;

    const CHAIN_ID: &str = "test_chain";

    fn sample_checkpoint(sequence: u64) -> FullCheckpointData {
        TestCheckpointBuilder::new(sequence)
            .start_transaction(1)
            .create_owned_object(42)
            .finish_transaction()
            .build_checkpoint()
    }

    fn transaction_info(checkpoint: &FullCheckpointData) -> TransactionInfo {
        let executed = &checkpoint.transactions[0];
        TransactionInfo {
            data: executed.transaction.clone(),
            effects: executed.effects.clone(),
            checkpoint: checkpoint.summary.sequence_number,
        }
    }

    fn sample_object(object_id: ObjectID, owner: SuiAddress, version: u64) -> Object {
        Object::with_id_owner_version_for_testing(
            object_id,
            SequenceNumber::from_u64(version),
            Owner::AddressOwner(owner),
        )
    }

    fn make_forking_store(
        forked_at_checkpoint: u64,
    ) -> Result<(TempDir, ForkingStore), anyhow::Error> {
        let tempdir = tempfile::tempdir()?;
        let filesystem = Arc::new(FileSystemStore::new_with_path(
            Node::Testnet,
            tempdir.path().to_path_buf(),
        )?);
        filesystem.setup(Some(CHAIN_ID.to_string()))?;

        let memory = Arc::new(InMemoryStore::new(Node::Testnet));
        let graphql = Arc::new(DataStore::new(Node::Testnet, "test-version")?);
        let hot_mem_fs: Arc<HotMemFs> =
            Arc::new(WriteThroughStore::new(memory.clone(), filesystem.clone()));
        let disk_then_graphql_objects: Arc<DiskThenGraphqlObjects> =
            Arc::new(ReadThroughStore::new(filesystem.clone(), graphql));
        let hot_objects: Arc<HotObjects> =
            Arc::new(WriteThroughStore::new(memory, disk_then_graphql_objects));
        let store = ForkDataStore::new(
            hot_mem_fs.clone(),
            hot_mem_fs.clone(),
            hot_objects,
            hot_mem_fs,
        );

        Ok((
            tempdir,
            ForkingStore::new(forked_at_checkpoint, filesystem, store),
        ))
    }

    #[test]
    fn transaction_reads_use_local_composite_store() -> Result<(), anyhow::Error> {
        let (_tempdir, store) = make_forking_store(11)?;
        let checkpoint = sample_checkpoint(11);
        let tx_info = transaction_info(&checkpoint);
        let tx_digest = checkpoint.transactions[0]
            .effects
            .transaction_digest()
            .to_string();

        store
            .store
            .write_transaction(&tx_digest, tx_info.clone())
            .expect("write transaction");

        let tx = store.get_transaction(checkpoint.transactions[0].effects.transaction_digest());
        assert!(
            tx.is_some(),
            "transaction should be readable from forking store"
        );

        let effects = store
            .get_transaction_effects(checkpoint.transactions[0].effects.transaction_digest())
            .expect("transaction effects");
        assert_eq!(effects, tx_info.effects);

        Ok(())
    }

    #[test]
    fn checkpoint_reads_use_local_composite_store() -> Result<(), anyhow::Error> {
        let (_tempdir, store) = make_forking_store(11)?;
        let checkpoint = sample_checkpoint(13);

        store
            .store
            .write_checkpoint(&checkpoint)
            .expect("write checkpoint");

        let summary = store
            .get_checkpoint_by_sequence_number(13)
            .expect("checkpoint summary");
        assert_eq!(summary.sequence_number, 13);
        assert_eq!(
            store
                .get_checkpoint_by_digest(checkpoint.summary.digest())
                .expect("checkpoint by digest")
                .sequence_number,
            13
        );
        assert_eq!(
            store
                .get_checkpoint_contents_by_digest(checkpoint.contents.digest())
                .expect("checkpoint contents")
                .digest(),
            checkpoint.contents.digest()
        );
        assert!(store.get_full_checkpoint_by_sequence_number(13).is_some());

        Ok(())
    }

    #[test]
    fn get_object_prefers_local_latest_and_owned_objects_stay_filesystem_backed()
    -> Result<(), anyhow::Error> {
        let (_tempdir, store) = make_forking_store(50)?;
        let owner = SuiAddress::random_for_testing_only();
        let object_id = ObjectID::random();
        let checkpoint_object = sample_object(object_id, owner, 1);
        let latest_object = sample_object(object_id, owner, 3);

        store
            .store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::AtCheckpoint(50),
                },
                checkpoint_object,
                1,
            )
            .expect("write checkpoint object");
        store
            .store
            .write_object(
                &ObjectKey {
                    object_id,
                    version_query: VersionQuery::Version(3),
                },
                latest_object.clone(),
                3,
            )
            .expect("write latest object");

        let returned = store.get_object(&object_id).expect("latest object");
        assert_eq!(returned.version().value(), 3);

        let owned = store.owned_objects(owner);
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].id(), object_id);

        Ok(())
    }

    #[test]
    fn update_objects_persists_to_local_filesystem_path() -> Result<(), anyhow::Error> {
        let (_tempdir, mut store) = make_forking_store(9)?;
        let owner = SuiAddress::random_for_testing_only();
        let object_id = ObjectID::random();
        let object = sample_object(object_id, owner, 7);

        store.update_objects(BTreeMap::from([(object_id, object.clone())]), Vec::new());

        let local_latest = store
            .filesystem
            .get_object_latest(&object_id)?
            .expect("object should be in filesystem");
        assert_eq!(local_latest.0.version().value(), 7);
        assert_eq!(
            store.get_object(&object_id).expect("object lookup").id(),
            object_id
        );

        Ok(())
    }
}
