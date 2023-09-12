// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gas_charger::GasCharger;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::committee::EpochId;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::execution::{ExecutionResults, LoadedChildObjectMetadata};
use sui_types::execution_status::ExecutionStatus;
use sui_types::inner_temporary_store::InnerTemporaryStore;
use sui_types::storage::{BackingStore, DeleteKindWithOldVersion};
use sui_types::sui_system_state::{get_sui_system_state_wrapper, AdvanceEpochParams};
use sui_types::type_resolver::LayoutResolver;
use sui_types::{
    base_types::{
        ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    },
    error::{ExecutionError, SuiError, SuiResult},
    event::Event,
    fp_bail,
    gas::GasCostSummary,
    object::Owner,
    object::{Data, Object},
    storage::{
        BackingPackageStore, ChildObjectResolver, DeleteKind, ObjectChange, ParentSync, Storage,
        WriteKind,
    },
    transaction::InputObjects,
};
use sui_types::{is_system_package, SUI_SYSTEM_STATE_OBJECT_ID};

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
    loaded_child_objects: BTreeMap<ObjectID, LoadedChildObjectMetadata>,
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
            loaded_child_objects: self
                .loaded_child_objects
                .into_iter()
                .map(|(id, metadata)| (id, metadata.version))
                .collect(),
            no_extraneous_module_bytes: self.protocol_config.no_extraneous_module_bytes(),
            runtime_packages_loaded_from_db: self.runtime_packages_loaded_from_db.read().clone(),
        }
    }

    /// For every object from active_inputs (i.e. all mutable objects), if they are not
    /// mutated during the transaction execution, force mutating them by incrementing the
    /// sequence number. This is required to achieve safety.
    pub(crate) fn ensure_active_inputs_mutated(&mut self) {
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

    pub fn into_effects(
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
        loaded_child_objects: BTreeMap<ObjectID, LoadedChildObjectMetadata>,
    ) {
        #[cfg(debug_assertions)]
        {
            for (id, v1) in &loaded_child_objects {
                if let Some(v2) = self.loaded_child_objects.get(id) {
                    assert_eq!(v1, v2);
                }
            }
            for (id, v1) in &self.loaded_child_objects {
                if let Some(v2) = loaded_child_objects.get(id) {
                    assert_eq!(v1, v2);
                }
            }
        }
        // Merge the two maps because we may be calling the execution engine more than once
        // (e.g. in advance epoch transaction, where we may be publishing a new system package).
        self.loaded_child_objects.extend(loaded_child_objects);
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
        // A mutated object must either be from input object or child object.
        if let Some(old_obj) = self.input_objects.get(id) {
            old_obj.storage_rebate
        } else if let Some(metadata) = self.loaded_child_objects.get(id) {
            debug_assert_eq!(metadata.version, expected_version);
            metadata.storage_rebate
        } else if let Ok(Some(obj)) = self.store.get_object_by_key(id, expected_version) {
            // The only case where an modified input object is not in the input list nor child object,
            // is when we upgrade a system package during epoch change.
            debug_assert!(obj.is_package());
            obj.storage_rebate
        } else {
            // not a lot we can do safely and under this condition everything is broken
            panic!(
                "Looking up storage rebate of mutated object {:?} should not fail",
                id
            )
        }
    }

    /// Track storage gas for each mutable input object (including the gas coin)
    /// and each created object. Compute storage refunds for each deleted object.
    /// Will *not* charge anything, gas status keeps track of storage cost and rebate.
    /// All objects will be updated with their new (current) storage rebate/cost.
    /// `SuiGasStatus` `storage_rebate` and `storage_gas_units` track the transaction
    /// overall storage rebate and cost.
    pub(crate) fn collect_storage_and_rebate(&mut self, gas_charger: &mut GasCharger) {
        // Use two loops because we cannot mut iterate written while calling get_input_storage_rebate.
        let old_storage_rebates: Vec<_> = self
            .written
            .iter()
            .map(|(object_id, (object, write_kind))| match write_kind {
                WriteKind::Create | WriteKind::Unwrap => 0,
                WriteKind::Mutate => self.get_input_storage_rebate(object_id, object.version()),
            })
            .collect();
        for ((object, _), old_storage_rebate) in self.written.values_mut().zip(old_storage_rebates)
        {
            // new object size
            let new_object_size = object.object_size_for_gas_metering();
            // track changes and compute the new object `storage_rebate`
            let new_storage_rebate =
                gas_charger.track_storage_mutation(new_object_size, old_storage_rebate);
            object.storage_rebate = new_storage_rebate;
        }

        self.collect_rebate(gas_charger);
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

type ModifiedObjectInfo<'a> = (ObjectID, Option<(SequenceNumber, u64)>, Option<&'a Object>);

impl<'backing> TemporaryStore<'backing> {
    fn get_input_sui(
        &self,
        id: &ObjectID,
        expected_version: SequenceNumber,
        layout_resolver: &mut impl LayoutResolver,
    ) -> Result<u64, ExecutionError> {
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
            obj.get_total_sui(layout_resolver).map_err(|e| {
                make_invariant_violation!(
                    "Failed looking up input SUI in SUI conservation checking for input with \
                         type {:?}: {e:#?}",
                    obj.struct_tag(),
                )
            })
        } else {
            // not in input objects, must be a dynamic field
            let Ok(Some(obj))= self.store.get_object_by_key(id, expected_version) else {
                invariant_violation!(
                    "Failed looking up dynamic field {id} in SUI conservation checking"
                );
            };
            obj.get_total_sui(layout_resolver).map_err(|e| {
                make_invariant_violation!(
                    "Failed looking up input SUI in SUI conservation checking for type \
                         {:?}: {e:#?}",
                    obj.struct_tag(),
                )
            })
        }
    }

    /// Return the list of all modified objects, for each object, returns
    /// - Object ID,
    /// - Input: If the object existed prior to this transaction, include their version and storage_rebate,
    /// - Output: If a new version of the object is written, include the new object.
    fn get_modified_objects(&self) -> Vec<ModifiedObjectInfo<'_>> {
        self.written
            .iter()
            .map(|(id, (object, kind))| match kind {
                WriteKind::Mutate => {
                    // When an object is mutated, its version remains the old version until the end.
                    let version = object.version();
                    let storage_rebate = self.get_input_storage_rebate(id, version);
                    (*id, Some((object.version(), storage_rebate)), Some(object))
                }
                WriteKind::Create | WriteKind::Unwrap => (*id, None, Some(object)),
            })
            .chain(self.deleted.iter().filter_map(|(id, kind)| match kind {
                DeleteKindWithOldVersion::Normal(version)
                | DeleteKindWithOldVersion::Wrap(version) => {
                    let storage_rebate = self.get_input_storage_rebate(id, *version);
                    Some((*id, Some((*version, storage_rebate)), None))
                }
                DeleteKindWithOldVersion::UnwrapThenDelete
                | DeleteKindWithOldVersion::UnwrapThenDeleteDEPRECATED(_) => None,
            }))
            .collect()
    }

    /// Check that this transaction neither creates nor destroys SUI. This should hold for all txes
    /// except the epoch change tx, which mints staking rewards equal to the gas fees burned in the
    /// previous epoch.  Specifically, this checks two key invariants about storage
    /// fees and storage rebate:
    ///
    /// 1. all SUI in storage rebate fields of input objects should flow either to the transaction
    ///    storage rebate, or the transaction non-refundable storage rebate
    /// 2. all SUI charged for storage should flow into the storage rebate field of some output
    ///    object
    ///
    /// This function is intended to be called *after* we have charged for
    /// gas + applied the storage rebate to the gas object, but *before* we
    /// have updated object versions.
    pub fn check_sui_conserved(
        &self,
        simple_conservation_checks: bool,
        gas_summary: &GasCostSummary,
    ) -> Result<(), ExecutionError> {
        if !simple_conservation_checks {
            return Ok(());
        }
        // total amount of SUI in storage rebate of input objects
        let mut total_input_rebate = 0;
        // total amount of SUI in storage rebate of output objects
        let mut total_output_rebate = 0;
        for (_, input, output) in self.get_modified_objects() {
            if let Some((_, storage_rebate)) = input {
                total_input_rebate += storage_rebate;
            }
            if let Some(object) = output {
                total_output_rebate += object.storage_rebate;
            }
        }

        if gas_summary.storage_cost == 0 {
            // this condition is usually true when the transaction went OOG and no
            // gas is left for storage charges.
            // The storage cost has to be there at least for the gas coin which
            // will not be deleted even when going to 0.
            // However if the storage cost is 0 and if there is any object touched
            // or deleted the value in input must be equal to the output plus rebate and
            // non refundable.
            // Rebate and non refundable will be positive when there are object deleted
            // (gas smashing being the primary and possibly only example).
            // A more typical condition is for all storage charges in summary to be 0 and
            // then input and output must be the same value
            if total_input_rebate
                != total_output_rebate
                    + gas_summary.storage_rebate
                    + gas_summary.non_refundable_storage_fee
            {
                return Err(ExecutionError::invariant_violation(format!(
                    "SUI conservation failed -- no storage charges in gas summary \
                        and total storage input rebate {} not equal  \
                        to total storage output rebate {}",
                    total_input_rebate, total_output_rebate,
                )));
            }
        } else {
            // all SUI in storage rebate fields of input objects should flow either to
            // the transaction storage rebate, or the non-refundable storage rebate pool
            if total_input_rebate
                != gas_summary.storage_rebate + gas_summary.non_refundable_storage_fee
            {
                return Err(ExecutionError::invariant_violation(format!(
                    "SUI conservation failed -- {} SUI in storage rebate field of input objects, \
                        {} SUI in tx storage rebate or tx non-refundable storage rebate",
                    total_input_rebate, gas_summary.non_refundable_storage_fee,
                )));
            }

            // all SUI charged for storage should flow into the storage rebate field
            // of some output object
            if gas_summary.storage_cost != total_output_rebate {
                return Err(ExecutionError::invariant_violation(format!(
                    "SUI conservation failed -- {} SUI charged for storage, \
                        {} SUI in storage rebate field of output objects",
                    gas_summary.storage_cost, total_output_rebate
                )));
            }
        }
        Ok(())
    }

    /// Check that this transaction neither creates nor destroys SUI.
    /// This more expensive check will check a third invariant on top of the 2 performed
    /// by `check_sui_conserved` above:
    ///
    /// * all SUI in input objects (including coins etc in the Move part of an object) should flow
    ///    either to an output object, or be burned as part of computation fees or non-refundable
    ///    storage rebate
    ///
    /// This function is intended to be called *after* we have charged for gas + applied the
    /// storage rebate to the gas object, but *before* we have updated object versions. The
    /// advance epoch transaction would mint `epoch_fees` amount of SUI, and burn `epoch_rebates`
    /// amount of SUI. We need these information for this check.
    pub fn check_sui_conserved_expensive(
        &self,
        gas_summary: &GasCostSummary,
        advance_epoch_gas_summary: Option<(u64, u64)>,
        layout_resolver: &mut impl LayoutResolver,
    ) -> Result<(), ExecutionError> {
        // total amount of SUI in input objects, including both coins and storage rebates
        let mut total_input_sui = 0;
        // total amount of SUI in output objects, including both coins and storage rebates
        let mut total_output_sui = 0;
        for (id, input, output) in self.get_modified_objects() {
            if let Some((version, _)) = input {
                total_input_sui += self.get_input_sui(&id, version, layout_resolver)?;
            }
            if let Some(object) = output {
                total_output_sui += object.get_total_sui(layout_resolver).map_err(|e| {
                    make_invariant_violation!(
                        "Failed looking up output SUI in SUI conservation checking for \
                         mutated type {:?}: {e:#?}",
                        object.struct_tag(),
                    )
                })?;
            }
        }
        // note: storage_cost flows into the storage_rebate field of the output objects, which is
        // why it is not accounted for here.
        // similarly, all of the storage_rebate *except* the storage_fund_rebate_inflow
        // gets credited to the gas coin both computation costs and storage rebate inflow are
        total_output_sui += gas_summary.computation_cost + gas_summary.non_refundable_storage_fee;
        if let Some((epoch_fees, epoch_rebates)) = advance_epoch_gas_summary {
            total_input_sui += epoch_fees;
            total_output_sui += epoch_rebates;
        }
        if total_input_sui != total_output_sui {
            return Err(ExecutionError::invariant_violation(format!(
                "SUI conservation failed: input={}, output={}, \
                    this transaction either mints or burns SUI",
                total_input_sui, total_output_sui,
            )));
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

    fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        TemporaryStore::read_object(self, id)
    }

    /// Take execution results v2, and translate it back to be compatible with effects v1.
    fn record_execution_results(&mut self, results: ExecutionResults) {
        let ExecutionResults::V2(results) = results else {
            panic!("ExecutionResults::V2 expected in sui-execution v1 and above");
        };
        let mut object_changes = BTreeMap::new();
        for (id, object) in results.written_objects {
            let write_kind = if results.created_object_ids.contains(&id) {
                WriteKind::Create
            } else if results.objects_modified_at.contains_key(&id) {
                WriteKind::Mutate
            } else {
                WriteKind::Unwrap
            };
            object_changes.insert(id, ObjectChange::Write(object, write_kind));
        }

        for id in results.deleted_object_ids {
            let delete_kind: DeleteKindWithOldVersion =
                if let Some((version, ..)) = results.objects_modified_at.get(&id) {
                    DeleteKindWithOldVersion::Normal(*version)
                } else {
                    DeleteKindWithOldVersion::UnwrapThenDelete
                };
            object_changes.insert(id, ObjectChange::Delete(delete_kind));
        }
        for (id, (version, ..)) in results.objects_modified_at {
            object_changes.entry(id).or_insert(ObjectChange::Delete(
                DeleteKindWithOldVersion::Wrap(version),
            ));
        }
        self.apply_object_changes(object_changes);

        for event in results.user_events {
            self.events.push(event);
        }
    }

    fn save_loaded_child_objects(
        &mut self,
        loaded_child_objects: BTreeMap<ObjectID, LoadedChildObjectMetadata>,
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
    fn get_latest_parent_entry_ref_deprecated(
        &self,
        _object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>> {
        unreachable!("Never called in newer protocol versions")
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
