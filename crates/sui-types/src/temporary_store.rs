// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{
        ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    },
    error::{ExecutionError, SuiError, SuiResult},
    event::Event,
    fp_bail,
    gas::{GasCostSummary, SuiGasStatus},
    messages::{ExecutionStatus, InputObjects, TransactionEffects},
    object::Owner,
    object::{Data, Object},
    storage::{
        BackingPackageStore, ChildObjectResolver, DeleteKind, ObjectChange, ParentSync, Storage,
        WriteKind,
    },
};
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use std::collections::BTreeMap;

pub struct InnerTemporaryStore {
    pub objects: BTreeMap<ObjectID, Object>,
    pub mutable_inputs: Vec<ObjectRef>,
    pub written: BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>,
    pub deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
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
}

pub struct TemporaryStore<S> {
    // The backing store for retrieving Move packages onchain.
    // When executing a Move call, the dependent packages are not going to be
    // in the input objects. They will be fetched from the backing store.
    // Also used for fetching the backing parent_sync to get the last known version for wrapped
    // objects
    store: S,
    tx_digest: TransactionDigest,
    input_objects: BTreeMap<ObjectID, Object>,
    mutable_input_refs: Vec<ObjectRef>, // Inputs that are mutable
    // When an object is being written, we need to ensure that a few invariants hold.
    // It's critical that we always call write_object to update `_written`, instead of writing
    // into _written directly.
    _written: BTreeMap<ObjectID, (Object, WriteKind)>, // Objects written
    /// Objects actively deleted.
    deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    /// Ordered sequence of events emitted by execution
    events: Vec<Event>,
}

impl<S> TemporaryStore<S> {
    /// Creates a new store associated with an authority store, and populates it with
    /// initial objects.
    pub fn new(store: S, input_objects: InputObjects, tx_digest: TransactionDigest) -> Self {
        let mutable_inputs = input_objects.mutable_inputs();
        let objects = input_objects.into_object_map();
        Self {
            store,
            tx_digest,
            input_objects: objects,
            mutable_input_refs: mutable_inputs,
            _written: BTreeMap::new(),
            deleted: BTreeMap::new(),
            events: Vec::new(),
        }
    }

    // Helpers to access private fields
    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.input_objects
    }

    pub fn deleted(&self) -> &BTreeMap<ObjectID, (SequenceNumber, DeleteKind)> {
        &self.deleted
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(self) -> (InnerTemporaryStore, Vec<Event>) {
        #[cfg(debug_assertions)]
        {
            self.check_invariants();
        }
        let written = self
            ._written
            .into_iter()
            .map(|(id, (obj, kind))| (id, (obj.compute_object_reference(), obj, kind)))
            .collect();
        let deleted = self
            .deleted
            .into_iter()
            .map(|(id, (seq, kind))| (id, (seq, kind)))
            .collect();
        (
            InnerTemporaryStore {
                objects: self.input_objects,
                mutable_inputs: self.mutable_input_refs,
                written,
                deleted,
            },
            self.events,
        )
    }

    /// For every object from active_inputs (i.e. all mutable objects), if they are not
    /// mutated during the transaction execution, force mutating them by incrementing the
    /// sequence number. This is required to achieve safety.
    /// We skip the gas object, because gas object will be updated separately.
    pub fn ensure_active_inputs_mutated(&mut self, gas_object_id: &ObjectID) {
        let mut to_be_updated = vec![];
        for (id, _seq, _) in &self.mutable_input_refs {
            if id == gas_object_id {
                continue;
            }
            if !self._written.contains_key(id) && !self.deleted.contains_key(id) {
                let mut object = self.input_objects[id].clone();
                // Active input object must be Move object.
                object.data.try_as_move_mut().unwrap().increment_version();
                // We cannot update here but have to push to `to_be_updated` and update latter
                // because the for loop is holding a reference to `self`, and calling
                // `self.write_object` requires a mutable reference to `self`.
                to_be_updated.push(object);
            }
        }
        for object in to_be_updated {
            // The object must be mutated as it was present in the input objects
            self.write_object(object, WriteKind::Mutate);
        }
    }

    /// For every object changes, charge gas accordingly. Since by this point we haven't charged gas yet,
    /// the gas object hasn't been mutated yet. Passing in `gas_object_size` so that we can also charge
    /// for the gas object mutation in advance.
    pub fn charge_gas_for_storage_changes(
        &mut self,
        gas_status: &mut SuiGasStatus<'_>,
        gas_object: &mut Object,
    ) -> Result<(), ExecutionError> {
        let mut objects_to_update = vec![];
        // Also charge gas for mutating the gas object in advance.
        let gas_object_size = gas_object.object_size_for_gas_metering();
        gas_object.storage_rebate = gas_status.charge_storage_mutation(
            gas_object_size,
            gas_object_size,
            gas_object.storage_rebate.into(),
        )?;
        objects_to_update.push((gas_object.clone(), WriteKind::Mutate));

        for (object_id, (object, write_kind)) in &mut self._written {
            let (old_object_size, storage_rebate) = self
                .input_objects
                .get(object_id)
                .map(|old| (old.object_size_for_gas_metering(), old.storage_rebate))
                .unwrap_or((0, 0));
            let new_storage_rebate = gas_status.charge_storage_mutation(
                old_object_size,
                object.object_size_for_gas_metering(),
                storage_rebate.into(),
            )?;
            if !object.is_immutable() {
                // We don't need to set storage rebate for immutable objects, as they will
                // never be deleted.
                object.storage_rebate = new_storage_rebate;
                objects_to_update.push((object.clone(), *write_kind));
            }
        }

        for object_id in self.deleted.keys() {
            // If an object is in `self.deleted`, and also in `self.objects`, we give storage rebate.
            // Otherwise if an object is in `self.deleted` but not in `self.objects`, it means this
            // object was unwrapped and then deleted. The rebate would have been provided already when
            // mutating the object that wrapped this object.
            if let Some(old_object) = self.input_objects.get(object_id) {
                gas_status.charge_storage_mutation(
                    old_object.object_size_for_gas_metering(),
                    0,
                    old_object.storage_rebate.into(),
                )?;
            }
        }

        // Write all objects at the end only if all previous gas charges succeeded.
        // This avoids polluting the temporary store state if this function failed.
        for (object, write_kind) in objects_to_update {
            self.write_object(object, write_kind);
        }

        Ok(())
    }

    pub fn to_effects(
        self,
        shared_object_refs: Vec<ObjectRef>,
        transaction_digest: &TransactionDigest,
        transaction_dependencies: Vec<TransactionDigest>,
        gas_cost_summary: GasCostSummary,
        status: ExecutionStatus,
        gas_object_ref: ObjectRef,
    ) -> (InnerTemporaryStore, TransactionEffects) {
        let written: BTreeMap<ObjectID, (ObjectRef, Owner, WriteKind)> = self
            ._written
            .iter()
            .map(|(id, (obj, write_kind))| {
                let obj_ref = obj.compute_object_reference();
                (*id, (obj_ref, obj.owner, *write_kind))
            })
            .collect();

        // In the case of special transactions that don't require a gas object,
        // we don't really care about the effects to gas, just use the input for it.
        let updated_gas_object_info = if gas_object_ref.0 == ObjectID::ZERO {
            (gas_object_ref, Owner::AddressOwner(SuiAddress::default()))
        } else {
            let (obj_ref, owner, _kind) = written[&gas_object_ref.0];
            (obj_ref, owner)
        };
        let mut mutated = vec![];
        let mut created = vec![];
        let mut unwrapped = vec![];
        for (_id, (object_ref, owner, kind)) in written {
            match kind {
                WriteKind::Mutate => mutated.push((object_ref, owner)),
                WriteKind::Create => created.push((object_ref, owner)),
                WriteKind::Unwrap => unwrapped.push((object_ref, owner)),
            }
        }

        let mut deleted = vec![];
        let mut wrapped = vec![];
        for (id, (version, kind)) in &self.deleted {
            match kind {
                DeleteKind::Normal | DeleteKind::UnwrapThenDelete => {
                    deleted.push((*id, *version, ObjectDigest::OBJECT_DIGEST_DELETED))
                }
                DeleteKind::Wrap => {
                    wrapped.push((*id, *version, ObjectDigest::OBJECT_DIGEST_WRAPPED))
                }
            }
        }
        let (inner, events) = self.into_inner();

        let effects = TransactionEffects {
            status,
            gas_used: gas_cost_summary,
            shared_objects: shared_object_refs,
            transaction_digest: *transaction_digest,
            created,
            mutated,
            unwrapped,
            deleted,
            wrapped,
            gas_object: updated_gas_object_info,
            events,
            dependencies: transaction_dependencies,
        };
        (inner, effects)
    }

    /// An internal check of the invariants (will only fire in debug)
    #[cfg(debug_assertions)]
    fn check_invariants(&self) {
        use std::collections::HashSet;
        // Check not both deleted and written
        debug_assert!(
            {
                let mut used = HashSet::new();
                self._written.iter().all(|(elt, _)| used.insert(elt));
                self.deleted.iter().all(move |elt| used.insert(elt.0))
            },
            "Object both written and deleted."
        );

        // Check all mutable inputs are either written or deleted
        debug_assert!(
            {
                let mut used = HashSet::new();
                self._written.iter().all(|(elt, _)| used.insert(elt));
                self.deleted.iter().all(|elt| used.insert(elt.0));

                self.mutable_input_refs
                    .iter()
                    .all(|elt| !used.insert(&elt.0))
            },
            "Mutable input neither written nor deleted."
        );

        debug_assert!(
            {
                self._written
                    .iter()
                    .all(|(_, (obj, _))| obj.previous_transaction == self.tx_digest)
            },
            "Object previous transaction not properly set",
        );
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

        // The adapter is not very disciplined at filling in the correct
        // previous transaction digest, so we ensure it is correct here.
        object.previous_transaction = self.tx_digest;
        self._written.insert(object.id(), (object, kind));
    }

    pub fn delete_object(&mut self, id: &ObjectID, version: SequenceNumber, kind: DeleteKind) {
        // there should be no deletion after write
        debug_assert!(self._written.get(id).is_none());
        // Check it is not read-only
        #[cfg(test)] // Movevm should ensure this
        if let Some(object) = self.read_object(id) {
            if object.is_immutable() {
                // This is an internal invariant violation. Move only allows us to
                // mutate objects if they are &mut so they cannot be read-only.
                panic!("Internal invariant violation: Deleting a read-only object.")
            }
        }

        // For object deletion, we increment their version so that they will
        // eventually show up in the parent_sync table with an updated version.
        self.deleted.insert(*id, (version.increment(), kind));
    }

    /// Resets any mutations and deletions recorded in the store.
    pub fn reset(&mut self) {
        self._written.clear();
        self.deleted.clear();
        self.events.clear();
    }

    pub fn log_event(&mut self, event: Event) {
        self.events.push(event)
    }

    pub fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        // there should be no read after delete
        debug_assert!(self.deleted.get(id).is_none());
        self._written
            .get(id)
            .map(|(obj, _kind)| obj)
            .or_else(|| self.input_objects.get(id))
    }

    pub fn apply_object_changes(&mut self, changes: BTreeMap<ObjectID, ObjectChange>) {
        for (id, change) in changes {
            match change {
                ObjectChange::Write(new_value, kind) => self.write_object(new_value, kind),
                ObjectChange::Delete(version, kind) => self.delete_object(&id, version, kind),
            }
        }
    }
}

impl<S: ChildObjectResolver> ChildObjectResolver for TemporaryStore<S> {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        // there should be no read after delete
        debug_assert!(self.deleted.get(child).is_none());
        let obj_opt = self._written.get(child).map(|(obj, _kind)| obj);
        if obj_opt.is_some() {
            Ok(obj_opt.cloned())
        } else {
            self.store.read_child_object(parent, child)
        }
    }
}

impl<S: ChildObjectResolver> Storage for TemporaryStore<S> {
    fn reset(&mut self) {
        TemporaryStore::reset(self)
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
}

impl<S: BackingPackageStore> ModuleResolver for TemporaryStore<S> {
    type Error = SuiError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        let package_id = &ObjectID::from(*module_id.address());
        let package_obj;
        let package = match self.read_object(package_id) {
            Some(object) => object,
            None => match self.store.get_package(package_id)? {
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

impl<S> ResourceResolver for TemporaryStore<S> {
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
                assert_eq!(
                    struct_tag, &m.type_,
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

impl<S: ParentSync> ParentSync for TemporaryStore<S> {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        self.store.get_latest_parent_entry_ref(object_id)
    }
}

/// Create an empty `TemporaryStore` with no backing storage for module resolution.
/// For testing purposes only.
pub fn empty_for_testing() -> TemporaryStore<()> {
    TemporaryStore::new(
        (),
        InputObjects::new(Vec::new()),
        TransactionDigest::genesis(),
    )
}

/// Create a `TemporaryStore` with the given inputs and no backing storage for module resolution.
/// For testing purposes only.
pub fn with_input_objects_for_testing(input_objects: InputObjects) -> TemporaryStore<()> {
    TemporaryStore::new((), input_objects, TransactionDigest::genesis())
}
