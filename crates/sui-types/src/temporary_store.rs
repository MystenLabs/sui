// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::committee::EpochId;
use crate::effects::{TransactionEffects, TransactionEvents};
use crate::execution_status::ExecutionStatus;
use crate::storage::{DeleteKindWithOldVersion, ObjectStore};
use crate::sui_system_state::{
    get_sui_system_state, get_sui_system_state_wrapper, AdvanceEpochParams, SuiSystemState,
};
use crate::type_resolver::LayoutResolver;
use crate::{
    base_types::{
        ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    },
    error::{ExecutionError, SuiError, SuiResult},
    event::Event,
    fp_bail,
    gas::{GasCharger, GasCostSummary},
    object::Owner,
    object::{Data, Object},
    storage::{
        BackingPackageStore, ChildObjectResolver, DeleteKind, ObjectChange, ParentSync, Storage,
        WriteKind,
    },
    transaction::InputObjects,
};
use crate::{is_system_package, SUI_SYSTEM_STATE_OBJECT_ID};
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;

pub type WrittenObjects = BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>;
pub type ObjectMap = BTreeMap<ObjectID, Object>;
pub type TxCoins = (ObjectMap, WrittenObjects);

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct InnerTemporaryStore {
    pub objects: ObjectMap,
    pub mutable_inputs: Vec<ObjectRef>,
    // All the written objects' sequence number should have been updated to the lamport version.
    pub written: WrittenObjects,
    // deleted or wrapped or unwrap-then-delete. The sequence number should have been updated to
    // the lamport version.
    pub deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    pub loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    pub events: TransactionEvents,
    pub max_binary_format_version: u32,
    pub no_extraneous_module_bytes: bool,
    pub runtime_packages_loaded_from_db: BTreeMap<ObjectID, Object>,
}

impl InnerTemporaryStore {
    /// Return the written object value with the given ID (if any)
    pub fn get_written_object(&self, id: &ObjectID) -> Option<&Object> {
        self.written.get(id).map(|o| &o.1)
    }

    /// Return the set of object ID's created during the current tx
    pub fn created(&self) -> Vec<ObjectID> {
        self.written
            .values()
            .filter_map(|(obj_ref, _, w)| {
                if *w == WriteKind::Create {
                    Some(obj_ref.0)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get the written objects owned by `address`
    pub fn get_written_objects_owned_by(&self, address: &SuiAddress) -> Vec<ObjectID> {
        self.written
            .values()
            .filter_map(|(_, o, _)| {
                if o.get_single_owner()
                    .map_or(false, |owner| &owner == address)
                {
                    Some(o.id())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_sui_system_state_object(&self) -> SuiResult<SuiSystemState> {
        get_sui_system_state(&self.written)
    }
}

pub struct TemporaryModuleResolver<'a, R> {
    temp_store: &'a InnerTemporaryStore,
    fallback: R,
}

impl<'a, R> TemporaryModuleResolver<'a, R> {
    pub fn new(temp_store: &'a InnerTemporaryStore, fallback: R) -> Self {
        Self {
            temp_store,
            fallback,
        }
    }
}

impl<R> GetModule for TemporaryModuleResolver<'_, R>
where
    R: GetModule<Item = Arc<CompiledModule>, Error = anyhow::Error>,
{
    type Error = anyhow::Error;
    type Item = Arc<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        let obj = self.temp_store.written.get(&ObjectID::from(*id.address()));
        if let Some((_, o, _)) = obj {
            if let Some(p) = o.data.try_as_package() {
                return Ok(Some(Arc::new(p.deserialize_module(
                    &id.name().into(),
                    self.temp_store.max_binary_format_version,
                    self.temp_store.no_extraneous_module_bytes,
                )?)));
            }
        }
        self.fallback.get_module_by_id(id)
    }
}

pub trait BackingStore:
    BackingPackageStore
    + ChildObjectResolver
    + GetModule<Error = SuiError, Item = CompiledModule>
    + ObjectStore
    + ParentSync
{
    fn as_object_store(&self) -> &dyn ObjectStore;
}

impl<T> BackingStore for T
where
    T: BackingPackageStore,
    T: ChildObjectResolver,
    T: GetModule<Error = SuiError, Item = CompiledModule>,
    T: ObjectStore,
    T: ParentSync,
{
    fn as_object_store(&self) -> &dyn ObjectStore {
        self
    }
}

pub struct TemporaryStore<'backing> {
    // The backing store for retrieving Move packages onchain.
    // When executing a Move call, the dependent packages are not going to be
    // in the input objects. They will be fetched from the backing store.
    // Also used for fetching the backing parent_sync to get the last known version for wrapped
    // objects
    store: Arc<dyn BackingStore + Send + Sync + 'backing>,
    tx_digest: TransactionDigest,
    input_objects: BTreeMap<ObjectID, Object>,
    /// The version to assign to all objects written by the transaction using this store.
    lamport_timestamp: SequenceNumber,
    mutable_input_refs: Vec<ObjectRef>, // Inputs that are mutable
    // When an object is being written, we need to ensure that a few invariants hold.
    // It's critical that we always call write_object to update `written`, instead of writing
    // into written directly.
    written: BTreeMap<ObjectID, (Object, WriteKind)>, // Objects written
    /// Objects actively deleted.
    deleted: BTreeMap<ObjectID, DeleteKindWithOldVersion>,
    /// Child objects loaded during dynamic field opers
    /// Currently onply populated for full nodes, not for validators
    loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    /// Ordered sequence of events emitted by execution
    events: Vec<Event>,
    protocol_config: ProtocolConfig,

    /// Every package that was loaded from DB store during execution.
    /// These packages were not previously loaded into the temporary store.
    runtime_packages_loaded_from_db: RwLock<BTreeMap<ObjectID, Object>>,
}

impl<'backing> TemporaryStore<'backing> {
    /// Creates a new store associated with an authority store, and populates it with
    /// initial objects.
    pub fn new(
        store: Arc<dyn BackingStore + Send + Sync + 'backing>,
        input_objects: InputObjects,
        tx_digest: TransactionDigest,
        protocol_config: &ProtocolConfig,
    ) -> Self {
        let mutable_inputs = input_objects.mutable_inputs();
        let lamport_timestamp = input_objects.lamport_timestamp();
        let objects = input_objects.into_object_map();
        Self {
            store,
            tx_digest,
            input_objects: objects,
            lamport_timestamp,
            mutable_input_refs: mutable_inputs,
            written: BTreeMap::new(),
            deleted: BTreeMap::new(),
            events: Vec::new(),
            protocol_config: protocol_config.clone(),
            loaded_child_objects: BTreeMap::new(),
            runtime_packages_loaded_from_db: RwLock::new(BTreeMap::new()),
        }
    }

    /// WARNING! Should only be used for dry run and dev inspect!
    /// In dry run and dev inspect, you might load a dynamic field that is actually too new for
    /// the transaction. Ideally, we would want to load the "correct" dynamic fields, but as that
    /// is not easily determined, we instead set the lamport version MAX, which is a valid lamport
    /// version for any object used in the transaction (preventing internal assertions or
    /// invariant violations from being triggered)
    pub fn new_for_mock_transaction(
        store: Arc<dyn BackingStore + Send + Sync + 'backing>,
        input_objects: InputObjects,
        tx_digest: TransactionDigest,
        protocol_config: &ProtocolConfig,
    ) -> Self {
        let mutable_inputs = input_objects.mutable_inputs();
        let lamport_timestamp = SequenceNumber::MAX;
        let objects = input_objects.into_object_map();
        Self {
            store,
            tx_digest,
            input_objects: objects,
            lamport_timestamp,
            mutable_input_refs: mutable_inputs,
            written: BTreeMap::new(),
            deleted: BTreeMap::new(),
            events: Vec::new(),
            protocol_config: protocol_config.clone(),
            loaded_child_objects: BTreeMap::new(),
            runtime_packages_loaded_from_db: RwLock::new(BTreeMap::new()),
        }
    }

    // Helpers to access private fields
    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.input_objects
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(self) -> InnerTemporaryStore {
        #[cfg(debug_assertions)]
        {
            self.check_invariants();
        }

        let mut written = BTreeMap::new();
        let mut deleted = BTreeMap::new();

        for (id, (mut obj, kind)) in self.written {
            // Update the version for the written object.
            match &mut obj.data {
                Data::Move(obj) => {
                    // Move objects all get the transaction's lamport timestamp
                    obj.increment_version_to(self.lamport_timestamp);
                }

                Data::Package(pkg) => {
                    // Modified packages get their version incremented (this is a special case that
                    // only applies to system packages).  All other packages can only be created,
                    // and they are left alone.
                    if kind == WriteKind::Mutate {
                        pkg.increment_version();
                    }
                }
            }

            // Record the version that the shared object was created at in its owner field.  Note,
            // this only works because shared objects must be created as shared (not created as
            // owned in one transaction and later converted to shared in another).
            if let Owner::Shared {
                initial_shared_version,
            } = &mut obj.owner
            {
                if kind == WriteKind::Create {
                    assert_eq!(
                        *initial_shared_version,
                        SequenceNumber::new(),
                        "Initial version should be blank before this point for {id:?}",
                    );
                    *initial_shared_version = self.lamport_timestamp;
                }
            }
            written.insert(id, (obj.compute_object_reference(), obj, kind));
        }

        for (id, kind) in self.deleted {
            // Check invariant that version must increase.
            if let Some(version) = kind.old_version() {
                debug_assert!(version < self.lamport_timestamp);
            }
            deleted.insert(id, (self.lamport_timestamp, kind.to_delete_kind()));
        }

        // Combine object events with move events.

        InnerTemporaryStore {
            objects: self.input_objects,
            mutable_inputs: self.mutable_input_refs,
            written,
            deleted,
            events: TransactionEvents { data: self.events },
            max_binary_format_version: self.protocol_config.move_binary_format_version(),
            loaded_child_objects: self.loaded_child_objects,
            no_extraneous_module_bytes: self.protocol_config.no_extraneous_module_bytes(),
            runtime_packages_loaded_from_db: self.runtime_packages_loaded_from_db.read().clone(),
        }
    }

    /// For every object from active_inputs (i.e. all mutable objects), if they are not
    /// mutated during the transaction execution, force mutating them by incrementing the
    /// sequence number. This is required to achieve safety.
    fn ensure_active_inputs_mutated(&mut self) {
        let mut to_be_updated = vec![];
        for (id, _seq, _) in &self.mutable_input_refs {
            if !self.written.contains_key(id) && !self.deleted.contains_key(id) {
                // We cannot update here but have to push to `to_be_updated` and update later
                // because the for loop is holding a reference to `self`, and calling
                // `self.write_object` requires a mutable reference to `self`.
                to_be_updated.push(self.input_objects[id].clone());
            }
        }
        for object in to_be_updated {
            // The object must be mutated as it was present in the input objects
            self.write_object(object, WriteKind::Mutate);
        }
    }

    pub fn to_effects(
        mut self,
        shared_object_refs: Vec<ObjectRef>,
        transaction_digest: &TransactionDigest,
        transaction_dependencies: Vec<TransactionDigest>,
        gas_cost_summary: GasCostSummary,
        status: ExecutionStatus,
        gas_charger: &mut GasCharger,
        epoch: EpochId,
    ) -> (InnerTemporaryStore, TransactionEffects) {
        let mut modified_at_versions = vec![];

        // Remember the versions objects were updated from in case of rollback.
        self.written.iter_mut().for_each(|(id, (obj, kind))| {
            if *kind == WriteKind::Mutate {
                modified_at_versions.push((*id, obj.version()))
            }
        });

        self.deleted.iter_mut().for_each(|(id, kind)| {
            if let Some(version) = kind.old_version() {
                modified_at_versions.push((*id, version));
            }
        });

        let protocol_version = self.protocol_config.version;
        let inner = self.into_inner();

        // In the case of special transactions that don't require a gas object,
        // we don't really care about the effects to gas, just use the input for it.
        // Gas coins are guaranteed to be at least size 1 and if more than 1
        // the first coin is where all the others are merged.
        let updated_gas_object_info = if let Some(coin_id) = gas_charger.gas_coin() {
            let (obj_ref, object, _kind) = &inner.written[&coin_id];
            (*obj_ref, object.owner)
        } else {
            (
                (ObjectID::ZERO, SequenceNumber::default(), ObjectDigest::MIN),
                Owner::AddressOwner(SuiAddress::default()),
            )
        };

        let mut mutated = vec![];
        let mut created = vec![];
        let mut unwrapped = vec![];
        for (object_ref, object, kind) in inner.written.values() {
            match kind {
                WriteKind::Mutate => mutated.push((*object_ref, object.owner)),
                WriteKind::Create => created.push((*object_ref, object.owner)),
                WriteKind::Unwrap => unwrapped.push((*object_ref, object.owner)),
            }
        }

        let mut deleted = vec![];
        let mut wrapped = vec![];
        let mut unwrapped_then_deleted = vec![];
        for (id, (version, kind)) in &inner.deleted {
            match kind {
                DeleteKind::Normal => {
                    deleted.push((*id, *version, ObjectDigest::OBJECT_DIGEST_DELETED))
                }
                DeleteKind::UnwrapThenDelete => unwrapped_then_deleted.push((
                    *id,
                    *version,
                    ObjectDigest::OBJECT_DIGEST_DELETED,
                )),
                DeleteKind::Wrap => {
                    wrapped.push((*id, *version, ObjectDigest::OBJECT_DIGEST_WRAPPED))
                }
            }
        }

        let effects = TransactionEffects::new_from_execution(
            protocol_version,
            status,
            epoch,
            gas_cost_summary,
            modified_at_versions,
            shared_object_refs,
            *transaction_digest,
            created,
            mutated,
            unwrapped,
            deleted,
            unwrapped_then_deleted,
            wrapped,
            updated_gas_object_info,
            if inner.events.data.is_empty() {
                None
            } else {
                Some(inner.events.digest())
            },
            transaction_dependencies,
        );
        (inner, effects)
    }

    /// An internal check of the invariants (will only fire in debug)
    #[cfg(debug_assertions)]
    fn check_invariants(&self) {
        // Check not both deleted and written
        debug_assert!(
            {
                let mut used = HashSet::new();
                self.written.iter().all(|(elt, _)| used.insert(elt));
                self.deleted.iter().all(move |elt| used.insert(elt.0))
            },
            "Object both written and deleted."
        );

        // Check all mutable inputs are either written or deleted
        debug_assert!(
            {
                let mut used = HashSet::new();
                self.written.iter().all(|(elt, _)| used.insert(elt));
                self.deleted.iter().all(|elt| used.insert(elt.0));

                self.mutable_input_refs
                    .iter()
                    .all(|elt| !used.insert(&elt.0))
            },
            "Mutable input neither written nor deleted."
        );

        debug_assert!(
            {
                self.written
                    .iter()
                    .all(|(_, (obj, _))| obj.previous_transaction == self.tx_digest)
            },
            "Object previous transaction not properly set",
        );

        if self.protocol_config.simplified_unwrap_then_delete() {
            debug_assert!(self.deleted.iter().all(|(_, kind)| {
                !matches!(
                    kind,
                    DeleteKindWithOldVersion::UnwrapThenDeleteDEPRECATED(_)
                )
            }));
        } else {
            debug_assert!(self
                .deleted
                .iter()
                .all(|(_, kind)| { !matches!(kind, DeleteKindWithOldVersion::UnwrapThenDelete) }));
        }
    }

    // Invariant: A key assumption of the write-delete logic
    // is that an entry is not both added and deleted by the
    // caller.

    pub fn write_object(&mut self, mut object: Object, kind: WriteKind) {
        // there should be no write after delete
        debug_assert!(self.deleted.get(&object.id()).is_none());
        // Check it is not read-only
        #[cfg(test)] // Movevm should ensure this
        if let Some(existing_object) = self.read_object(&object.id()) {
            if existing_object.is_immutable() {
                // This is an internal invariant violation. Move only allows us to
                // mutate objects if they are &mut so they cannot be read-only.
                panic!("Internal invariant violation: Mutating a read-only object.")
            }
        }

        // Created mutable objects' versions are set to the store's lamport timestamp when it is
        // committed to effects. Creating an object at a non-zero version risks violating the
        // lamport timestamp invariant (that a transaction's lamport timestamp is strictly greater
        // than all versions witnessed by the transaction).
        debug_assert!(
            kind != WriteKind::Create
                || object.is_immutable()
                || object.version() == SequenceNumber::MIN,
            "Created mutable objects should not have a version set",
        );

        // The adapter is not very disciplined at filling in the correct
        // previous transaction digest, so we ensure it is correct here.
        object.previous_transaction = self.tx_digest;
        self.written.insert(object.id(), (object, kind));
    }

    pub fn delete_object(&mut self, id: &ObjectID, kind: DeleteKindWithOldVersion) {
        // there should be no deletion after write
        debug_assert!(self.written.get(id).is_none());

        // TODO: promote this to an on-in-prod check that raises an invariant_violation
        // Check that we are not deleting an immutable object
        #[cfg(debug_assertions)]
        if let Some(object) = self.read_object(id) {
            if object.is_immutable() {
                // This is an internal invariant violation. Move only allows us to
                // mutate objects if they are &mut so they cannot be read-only.
                // In addition, gas objects should never be immutable, so gas smashing
                // should not allow us to delete immutable objects
                let digest = self.tx_digest;
                panic!("Internal invariant violation in tx {digest}: Deleting immutable object {id}, delete kind {kind:?}")
            }
        }

        // For object deletion, we will increment the version when converting the store to effects
        // so the object will eventually show up in the parent_sync table with a new version.
        self.deleted.insert(*id, kind);
    }

    pub fn drop_writes(&mut self) {
        self.written.clear();
        self.deleted.clear();
        self.events.clear();
    }

    pub fn log_event(&mut self, event: Event) {
        self.events.push(event)
    }

    pub fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        // there should be no read after delete
        debug_assert!(self.deleted.get(id).is_none());
        self.written
            .get(id)
            .map(|(obj, _kind)| obj)
            .or_else(|| self.input_objects.get(id))
    }

    pub fn apply_object_changes(&mut self, changes: BTreeMap<ObjectID, ObjectChange>) {
        for (id, change) in changes {
            match change {
                ObjectChange::Write(new_value, kind) => self.write_object(new_value, kind),
                ObjectChange::Delete(kind) => self.delete_object(&id, kind),
            }
        }
    }
    pub fn save_loaded_child_objects(
        &mut self,
        loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    ) {
        self.loaded_child_objects = loaded_child_objects;
    }

    pub fn estimate_effects_size_upperbound(&self) -> usize {
        // In the worst case, the number of deps is equal to the number of input objects
        TransactionEffects::estimate_effects_size_upperbound(
            self.written.len(),
            self.mutable_input_refs.len(),
            self.deleted.len(),
            self.input_objects.len(),
        )
    }

    pub fn written_objects_size(&self) -> usize {
        self.written
            .iter()
            .fold(0, |sum, obj| sum + obj.1 .0.object_size_for_gas_metering())
    }

    /// If there are unmetered storage rebate (due to system transaction), we put them into
    /// the storage rebate of 0x5 object.
    pub fn conserve_unmetered_storage_rebate(&mut self, unmetered_storage_rebate: u64) {
        if unmetered_storage_rebate == 0 {
            // If unmetered_storage_rebate is 0, we are most likely executing the genesis transaction.
            // And in that case we cannot mutate the 0x5 object because it's newly created.
            // And there is no storage rebate that needs distribution anyway.
            return;
        }
        tracing::debug!(
            "Amount of unmetered storage rebate from system tx: {:?}",
            unmetered_storage_rebate
        );
        let mut system_state_wrapper = self
            .read_object(&SUI_SYSTEM_STATE_OBJECT_ID)
            .expect("0x5 object must be muated in system tx with unmetered storage rebate")
            .clone();
        // In unmetered execution, storage_rebate field of mutated object must be 0.
        // If not, we would be dropping SUI on the floor by overriding it.
        assert_eq!(system_state_wrapper.storage_rebate, 0);
        system_state_wrapper.storage_rebate = unmetered_storage_rebate;
        self.write_object(system_state_wrapper, WriteKind::Mutate);
    }
}

impl<'backing> TemporaryStore<'backing> {
    /// returns lists of (objects whose owner we must authenticate, objects whose owner has already been authenticated)
    fn get_objects_to_authenticate(
        &self,
        sender: &SuiAddress,
        gas_charger: &mut GasCharger,
        is_epoch_change: bool,
    ) -> SuiResult<(Vec<ObjectID>, HashSet<ObjectID>)> {
        let gas_objs: HashSet<&ObjectID> = gas_charger.gas_coins().iter().map(|g| &g.0).collect();
        let mut objs_to_authenticate = Vec::new();
        let mut authenticated_objs = HashSet::new();
        for (id, obj) in &self.input_objects {
            if gas_objs.contains(id) {
                // gas could be owned by either the sender (common case) or sponsor (if this is a sponsored tx,
                // which we do not know inside this function).
                // either way, no object ownership chain should be rooted in a gas object
                // thus, consider object authenticated, but don't add it to authenticated_objs
                continue;
            }
            match &obj.owner {
                Owner::AddressOwner(a) => {
                    assert!(sender == a, "Input object not owned by sender");
                    authenticated_objs.insert(*id);
                }
                Owner::Shared { .. } => {
                    authenticated_objs.insert(*id);
                }
                Owner::Immutable => {
                    // object is authenticated, but it cannot own other objects,
                    // so we should not add it to `authenticated_objs`
                    // However, we would definitely want to add immutable objects
                    // to the set of autehnticated roots if we were doing runtime
                    // checks inside the VM instead of after-the-fact in the temporary
                    // store. Here, we choose not to add them because this will catch a
                    // bug where we mutate or delete an object that belongs to an immutable
                    // object (though it will show up somewhat opaquely as an authentication
                    // failure), whereas adding the immutable object to the roots will prevent
                    // us from catching this.
                }
                Owner::ObjectOwner(_parent) => {
                    unreachable!("Input objects must be address owned, shared, or immutable")
                }
            }
        }

        for (id, (_new_obj, kind)) in &self.written {
            if authenticated_objs.contains(id) || gas_objs.contains(id) {
                continue;
            }
            match kind {
                WriteKind::Mutate => {
                    // get owner at beginning of tx, since that's what we have to authenticate against
                    // _new_obj.owner is not relevant here
                    let old_obj = self.store.get_object(id)?.unwrap_or_else(|| {
                        panic!("Mutated object must exist in the store: ID = {:?}", id)
                    });
                    match &old_obj.owner {
                        Owner::ObjectOwner(_parent) => {
                            objs_to_authenticate.push(*id);
                        }
                        Owner::AddressOwner(_) | Owner::Shared { .. } => {
                            unreachable!("Should already be in authenticated_objs")
                        }
                        Owner::Immutable => {
                            assert!(is_epoch_change, "Immutable objects cannot be written, except for Sui Framework/Move stdlib upgrades at epoch change boundaries");
                            // Note: this assumes that the only immutable objects an epoch change tx can update are system packages,
                            // but in principle we could allow others.
                            assert!(
                                is_system_package(*id),
                                "Only system packages can be upgraded"
                            );
                        }
                    }
                }
                WriteKind::Create | WriteKind::Unwrap => {
                    // created and unwrapped objects were not inputs to the tx
                    // or dynamic fields accessed at runtime, no ownership checks needed
                }
            }
        }

        for (id, kind) in &self.deleted {
            if authenticated_objs.contains(id) || gas_objs.contains(id) {
                continue;
            }
            match kind {
                DeleteKindWithOldVersion::Normal(_) | DeleteKindWithOldVersion::Wrap(_) => {
                    // get owner at beginning of tx
                    let old_obj = self.store.get_object(id)?.unwrap();
                    match &old_obj.owner {
                        Owner::ObjectOwner(_) => {
                            objs_to_authenticate.push(*id);
                        }
                        Owner::AddressOwner(_) | Owner::Shared { .. } => {
                            unreachable!("Should already be in authenticated_objs")
                        }
                        Owner::Immutable => unreachable!("Immutable objects cannot be deleted"),
                    }
                }
                DeleteKindWithOldVersion::UnwrapThenDelete
                | DeleteKindWithOldVersion::UnwrapThenDeleteDEPRECATED(_) => {
                    // unwrapped-then-deleted object was not an input to the tx,
                    // no ownership checks needed
                }
            }
        }
        Ok((objs_to_authenticate, authenticated_objs))
    }

    // check that every object read is owned directly or indirectly by sender, sponsor, or a shared object input
    pub fn check_ownership_invariants(
        &self,
        sender: &SuiAddress,
        gas_charger: &mut GasCharger,
        is_epoch_change: bool,
    ) -> SuiResult<()> {
        let (mut objects_to_authenticate, mut authenticated_objects) =
            self.get_objects_to_authenticate(sender, gas_charger, is_epoch_change)?;

        // Map from an ObjectID to the ObjectID that covers it.
        let mut covered = BTreeMap::new();
        while let Some(to_authenticate) = objects_to_authenticate.pop() {
            let Some(old_obj) = self.store.get_object(&to_authenticate)? else {
                // lookup failure is expected when the parent is an "object-less" UID (e.g., the ID of a table or bag)
                // we cannot distinguish this case from an actual authentication failure, so continue
                continue;
            };
            let parent = match &old_obj.owner {
                Owner::ObjectOwner(parent) => ObjectID::from(*parent),
                owner => panic!(
                    "Unauthenticated root at {to_authenticate:?} with owner {owner:?}\n\
             Potentially covering objects in: {covered:#?}",
                ),
            };

            if authenticated_objects.contains(&parent) {
                authenticated_objects.insert(to_authenticate);
            } else if !covered.contains_key(&parent) {
                objects_to_authenticate.push(parent);
            }

            covered.insert(to_authenticate, parent);
        }
        Ok(())
    }
}

impl<'backing> TemporaryStore<'backing> {
    /// Return the storage rebate of object `id`
    fn get_input_storage_rebate(&self, id: &ObjectID, expected_version: SequenceNumber) -> u64 {
        if let Some(old_obj) = self.input_objects.get(id) {
            old_obj.storage_rebate
        } else {
            // else, this is a dynamic field, not an input object
            if let Ok(Some(old_obj)) = self.store.get_object_by_key(id, expected_version) {
                old_obj.storage_rebate
            } else {
                // not a lot we can do safely and under this condition everything is broken
                panic!("Looking up storage rebate of mutated object should not fail")
            }
        }
    }

    pub(crate) fn ensure_gas_and_input_mutated(&mut self, gas_charger: &mut GasCharger) {
        if let Some(gas_object_id) = gas_charger.gas_coin() {
            let gas_object = self
                .read_object(&gas_object_id)
                .expect("We constructed the object map so it should always have the gas object id")
                .clone();
            self.written
                .entry(gas_object_id)
                .or_insert_with(|| (gas_object, WriteKind::Mutate));
        }
        self.ensure_active_inputs_mutated();
    }

    /// Track storage gas for each mutable input object (including the gas coin)
    /// and each created object. Compute storage refunds for each deleted object.
    /// Will *not* charge anything, gas status keeps track of storage cost and rebate.
    /// All objects will be updated with their new (current) storage rebate/cost.
    /// `SuiGasStatus` `storage_rebate` and `storage_gas_units` track the transaction
    /// overall storage rebate and cost.
    pub(crate) fn collect_storage_and_rebate(&mut self, gas_charger: &mut GasCharger) {
        let mut objects_to_update = vec![];
        for (object_id, (object, write_kind)) in &mut self.written {
            // get the object storage_rebate in input for mutated objects
            let old_storage_rebate = match write_kind {
                WriteKind::Create | WriteKind::Unwrap => 0,
                WriteKind::Mutate => {
                    if let Some(old_obj) = self.input_objects.get(object_id) {
                        old_obj.storage_rebate
                    } else {
                        // else, this is a dynamic field, not an input object
                        let expected_version = object.version();
                        if let Ok(Some(old_obj)) =
                            self.store.get_object_by_key(object_id, expected_version)
                        {
                            old_obj.storage_rebate
                        } else {
                            // not a lot we can do safely and under this condition everything is broken
                            panic!("Looking up storage rebate of mutated object should not fail");
                        }
                    }
                }
            };
            // new object size
            let new_object_size = object.object_size_for_gas_metering();
            // track changes and compute the new object `storage_rebate`
            let new_storage_rebate =
                gas_charger.track_storage_mutation(new_object_size, old_storage_rebate);
            object.storage_rebate = new_storage_rebate;
            if !object.is_immutable() {
                objects_to_update.push((object.clone(), *write_kind));
            }
        }

        self.collect_rebate(gas_charger);

        // Write all objects at the end only if all previous gas charges succeeded.
        // This avoids polluting the temporary store state if this function failed.
        for (object, write_kind) in objects_to_update {
            self.write_object(object, write_kind);
        }
    }

    pub(crate) fn collect_rebate(&self, gas_charger: &mut GasCharger) {
        for (object_id, kind) in &self.deleted {
            match kind {
                DeleteKindWithOldVersion::Wrap(version)
                | DeleteKindWithOldVersion::Normal(version) => {
                    // get and track the deleted object `storage_rebate`
                    let storage_rebate = self.get_input_storage_rebate(object_id, *version);
                    gas_charger.track_storage_mutation(0, storage_rebate);
                }
                DeleteKindWithOldVersion::UnwrapThenDelete
                | DeleteKindWithOldVersion::UnwrapThenDeleteDEPRECATED(_) => {
                    // an unwrapped object does not have a storage rebate, we will charge for storage changes via its wrapper object
                }
            }
        }
    }
}
//==============================================================================
// Charge gas current - end
//==============================================================================

impl<'backing> TemporaryStore<'backing> {
    pub fn advance_epoch_safe_mode(
        &mut self,
        params: &AdvanceEpochParams,
        protocol_config: &ProtocolConfig,
    ) {
        let wrapper = get_sui_system_state_wrapper(self.store.as_object_store())
            .expect("System state wrapper object must exist");
        let new_object =
            wrapper.advance_epoch_safe_mode(params, self.store.as_object_store(), protocol_config);
        self.write_object(new_object, WriteKind::Mutate);
    }
}

impl<'backing> TemporaryStore<'backing> {
    fn get_input_sui(
        &self,
        id: &ObjectID,
        expected_version: SequenceNumber,
        layout_resolver: &mut impl LayoutResolver,
        do_expensive_checks: bool,
    ) -> Result<(u64, u64), ExecutionError> {
        if let Some(obj) = self.input_objects.get(id) {
            // the assumption here is that if it is in the input objects must be the right one
            if obj.version() != expected_version {
                invariant_violation!(
                    "Version mismatching when resolving input object to check conservation--\
                     expected {}, got {}",
                    expected_version,
                    obj.version(),
                );
            }
            let input_sui = if do_expensive_checks {
                obj.get_total_sui(layout_resolver).map_err(|e| {
                    make_invariant_violation!(
                        "Failed looking up input SUI in SUI conservation checking for input with \
                         type {:?}: {e:#?}",
                        obj.struct_tag(),
                    )
                })?
            } else {
                0
            };
            Ok((input_sui, obj.storage_rebate))
        } else {
            // not in input objects, must be a dynamic field
            let Ok(Some(obj))= self.store.get_object_by_key(id, expected_version) else {
                invariant_violation!(
                    "Failed looking up dynamic field {id} in SUI conservation checking"
                );
            };
            let input_sui = if do_expensive_checks {
                obj.get_total_sui(layout_resolver).map_err(|e| {
                    make_invariant_violation!(
                        "Failed looking up input SUI in SUI conservation checking for type \
                         {:?}: {e:#?}",
                        obj.struct_tag(),
                    )
                })?
            } else {
                0
            };
            Ok((input_sui, obj.storage_rebate))
        }
    }

    /// Check that this transaction neither creates nor destroys SUI. This should hold for all txes
    /// except the epoch change tx, which mints staking rewards equal to the gas fees burned in the
    /// previous epoch.  Specifically, this checks two key invariants about storage fees and storage
    /// rebate:
    ///
    /// 1. all SUI in storage rebate fields of input objects should flow either to the transaction
    ///    storage rebate, or the transaction non-refundable storage rebate
    /// 2. all SUI charged for storage should flow into the storage rebate field of some output
    ///    object
    ///
    /// If `do_expensive_checks` is true, this will also check a third invariant:
    ///
    /// 3. all SUI in input objects (including coins etc in the Move part of an object) should flow
    ///    either to an output object, or be burned as part of computation fees or non-refundable
    ///    storage rebate
    ///
    /// This function is intended to be called *after* we have charged for gas + applied the storage
    /// rebate to the gas object, but *before* we have updated object versions.  If
    /// `do_expensive_checks` is false, this function will only check conservation of object storage
    /// rea `epoch_fees` and `epoch_rebates` are only set for advance epoch transactions.  The
    /// advance epoch transaction would mint `epoch_fees` amount of SUI, and burn `epoch_rebates`
    /// amount of SUI. We need these information for conservation check.
    pub fn check_sui_conserved(
        &self,
        gas_summary: &GasCostSummary,
        advance_epoch_gas_summary: Option<(u64, u64)>,
        layout_resolver: &mut impl LayoutResolver,
        do_expensive_checks: bool,
    ) -> Result<(), ExecutionError> {
        // total amount of SUI in input objects, including both coins and storage rebates
        let mut total_input_sui = 0;
        // total amount of SUI in output objects, including both coins and storage rebates
        let mut total_output_sui = 0;
        // total amount of SUI in storage rebate of input objects
        let mut total_input_rebate = 0;
        // total amount of SUI in storage rebate of output objects
        let mut total_output_rebate = 0;
        for (id, (output_obj, kind)) in &self.written {
            match kind {
                WriteKind::Mutate => {
                    // note: output_obj.version has not yet been increased by the tx, so output_obj.version
                    // is the object version at tx input
                    let input_version = output_obj.version();
                    let (input_sui, input_storage_rebate) = self.get_input_sui(
                        id,
                        input_version,
                        layout_resolver,
                        do_expensive_checks,
                    )?;
                    total_input_sui += input_sui;
                    if do_expensive_checks {
                        total_output_sui +=
                            output_obj.get_total_sui(layout_resolver).map_err(|e| {
                                make_invariant_violation!(
                                    "Failed looking up output SUI in SUI conservation checking for \
                                     mutated type {:?}: {e:#?}",
                                    output_obj.struct_tag(),
                                )
                            })?;
                    }
                    total_input_rebate += input_storage_rebate;
                    total_output_rebate += output_obj.storage_rebate;
                }
                WriteKind::Create => {
                    // created objects did not exist at input, and thus contribute 0 to input SUI
                    if do_expensive_checks {
                        total_output_sui +=
                            output_obj.get_total_sui(layout_resolver).map_err(|e| {
                                make_invariant_violation!(
                                    "Failed looking up output SUI in SUI conservation checking for \
                                     created type {:?}: {e:#?}",
                                    output_obj.struct_tag()
                                )
                            })?;
                    }
                    total_output_rebate += output_obj.storage_rebate;
                }
                WriteKind::Unwrap => {
                    // an unwrapped object was either:
                    // 1. wrapped in an input object A,
                    // 2. wrapped in a dynamic field A, or itself a dynamic field
                    // in both cases, its contribution to input SUI will be captured by looking at A
                    if do_expensive_checks {
                        total_output_sui +=
                            output_obj.get_total_sui(layout_resolver).map_err(|e| {
                                make_invariant_violation!(
                                    "Failed looking up output SUI in SUI conservation checking for \
                                     unwrapped type {:?}: {e:#?}",
                                    output_obj.struct_tag(),
                                )
                            })?;
                    }
                    total_output_rebate += output_obj.storage_rebate;
                }
            }
        }
        for (id, kind) in &self.deleted {
            match kind {
                DeleteKindWithOldVersion::Normal(input_version) => {
                    let (input_sui, input_storage_rebate) = self.get_input_sui(
                        id,
                        *input_version,
                        layout_resolver,
                        do_expensive_checks,
                    )?;
                    total_input_sui += input_sui;
                    total_input_rebate += input_storage_rebate;
                }
                DeleteKindWithOldVersion::Wrap(input_version) => {
                    // wrapped object was a tx input or dynamic field--need to account for it in input SUI
                    // note: if an object is created by the tx, then wrapped, it will not appear here
                    let (input_sui, input_storage_rebate) = self.get_input_sui(
                        id,
                        *input_version,
                        layout_resolver,
                        do_expensive_checks,
                    )?;
                    total_input_sui += input_sui;
                    total_input_rebate += input_storage_rebate;
                    // else, the wrapped object was either:
                    // 1. freshly created, which means it has 0 contribution to input SUI
                    // 2. unwrapped from another object A, which means its contribution to input SUI will be captured by looking at A
                }
                DeleteKindWithOldVersion::UnwrapThenDelete
                | DeleteKindWithOldVersion::UnwrapThenDeleteDEPRECATED(_) => {
                    // an unwrapped option was wrapped in input object or dynamic field A, which means its contribution to input SUI will
                    // be captured by looking at A
                }
            }
        }

        if do_expensive_checks {
            // note: storage_cost flows into the storage_rebate field of the output objects, which is why it is not accounted for here.
            // similarly, all of the storage_rebate *except* the storage_fund_rebate_inflow gets credited to the gas coin
            // both computation costs and storage rebate inflow are
            total_output_sui +=
                gas_summary.computation_cost + gas_summary.non_refundable_storage_fee;
            if let Some((epoch_fees, epoch_rebates)) = advance_epoch_gas_summary {
                total_input_sui += epoch_fees;
                total_output_sui += epoch_rebates;
            }
            if total_input_sui != total_output_sui {
                return Err(ExecutionError::invariant_violation(
                format!("SUI conservation failed: input={}, output={}, this transaction either mints or burns SUI",
                total_input_sui,
                total_output_sui))
            );
            }
        }

        // all SUI in storage rebate fields of input objects should flow either to the transaction storage rebate, or the non-refundable
        // storage rebate pool
        if total_input_rebate != gas_summary.storage_rebate + gas_summary.non_refundable_storage_fee
        {
            // TODO: re-enable once we fix the edge case with OOG, gas smashing, and storage rebate
            /*return Err(ExecutionError::invariant_violation(
                format!("SUI conservation failed--{} SUI in storage rebate field of input objects, {} SUI in tx storage rebate or tx non-refundable storage rebate",
                total_input_rebate,
                gas_summary.non_refundable_storage_fee))
            );*/
        }

        // all SUI charged for storage should flow into the storage rebate field of some output object
        if gas_summary.storage_cost != total_output_rebate {
            // TODO: re-enable once we fix the edge case with OOG, gas smashing, and storage rebate
            /*return Err(ExecutionError::invariant_violation(
                format!("SUI conservation failed--{} SUI charged for storage, {} SUI in storage rebate field of output objects",
                gas_summary.storage_cost,
                total_output_rebate))
            );*/
        }
        Ok(())
    }
}

impl<'backing> ChildObjectResolver for TemporaryStore<'backing> {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        // there should be no read after delete
        debug_assert!(self.deleted.get(child).is_none());
        let obj_opt = self.written.get(child).map(|(obj, _kind)| obj);
        if obj_opt.is_some() {
            Ok(obj_opt.cloned())
        } else {
            self.store
                .read_child_object(parent, child, child_version_upper_bound)
        }
    }
}

impl<'backing> Storage for TemporaryStore<'backing> {
    fn reset(&mut self) {
        self.written.clear();
        self.deleted.clear();
        self.events.clear();
    }

    fn log_event(&mut self, event: Event) {
        TemporaryStore::log_event(self, event)
    }

    fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        TemporaryStore::read_object(self, id)
    }

    fn apply_object_changes(&mut self, changes: BTreeMap<ObjectID, ObjectChange>) {
        TemporaryStore::apply_object_changes(self, changes)
    }

    fn save_loaded_child_objects(
        &mut self,
        loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    ) {
        TemporaryStore::save_loaded_child_objects(self, loaded_child_objects)
    }
}

impl<'backing> BackingPackageStore for TemporaryStore<'backing> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        if let Some((obj, _)) = self.written.get(package_id) {
            Ok(Some(obj.clone()))
        } else {
            self.store.get_package_object(package_id).map(|obj| {
                // Track object but leave unchanged
                if let Some(v) = obj.clone() {
                    // TODO: Can this lock ever block execution?
                    self.runtime_packages_loaded_from_db
                        .write()
                        .insert(*package_id, v);
                }
                obj
            })
        }
    }
}

impl<'backing> ModuleResolver for TemporaryStore<'backing> {
    type Error = SuiError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        let package_id = &ObjectID::from(*module_id.address());
        let package_obj;
        let package = match self.read_object(package_id) {
            Some(object) => object,
            None => match self.store.get_package_object(package_id)? {
                Some(object) => {
                    package_obj = object;
                    &package_obj
                }
                None => {
                    return Ok(None);
                }
            },
        };
        match &package.data {
            Data::Package(c) => Ok(c
                .serialized_module_map()
                .get(module_id.name().as_str())
                .cloned()),
            _ => Err(SuiError::BadObjectType {
                error: "Expected module object".to_string(),
            }),
        }
    }
}

impl<'backing> ResourceResolver for TemporaryStore<'backing> {
    type Error = SuiError;

    fn get_resource(
        &self,
        address: &AccountAddress,
        struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        let object = match self.read_object(&ObjectID::from(*address)) {
            Some(x) => x,
            None => match self.read_object(&ObjectID::from(*address)) {
                None => return Ok(None),
                Some(x) => {
                    if !x.is_immutable() {
                        fp_bail!(SuiError::ExecutionInvariantViolation);
                    }
                    x
                }
            },
        };

        match &object.data {
            Data::Move(m) => {
                assert!(
                    m.is_type(struct_tag),
                    "Invariant violation: ill-typed object in storage \
                or bad object request from caller"
                );
                Ok(Some(m.contents().to_vec()))
            }
            other => unimplemented!(
                "Bad object lookup: expected Move object, but got {:?}",
                other
            ),
        }
    }
}

impl<'backing> ParentSync for TemporaryStore<'backing> {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        self.store.get_latest_parent_entry_ref(object_id)
    }
}

impl<'backing> GetModule for TemporaryStore<'backing> {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, module_id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        let package_id = &ObjectID::from(*module_id.address());
        if let Some((obj, _)) = self.written.get(package_id) {
            Ok(Some(
                obj.data
                    .try_as_package()
                    .expect("Bad object type--expected package")
                    .deserialize_module(
                        &module_id.name().to_owned(),
                        self.protocol_config.move_binary_format_version(),
                        self.protocol_config.no_extraneous_module_bytes(),
                    )?,
            ))
        } else {
            self.store.get_module_by_id(module_id)
        }
    }
}
