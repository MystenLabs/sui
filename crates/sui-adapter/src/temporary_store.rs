// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use std::collections::{BTreeMap, HashSet};
use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::error::{ExecutionError, SuiError, SuiResult};
use sui_types::fp_bail;
use sui_types::messages::{ExecutionStatus, InputObjects, TransactionEffects};
use sui_types::object::{Data, Object};
use sui_types::storage::{BackingPackageStore, DeleteKind, ParentSync, Storage};
use sui_types::{
    event::Event,
    gas::{GasCostSummary, SuiGasStatus},
    object::Owner,
};

pub struct InnerTemporaryStore {
    pub objects: BTreeMap<ObjectID, Object>,
    pub mutable_inputs: Vec<ObjectRef>,
    pub written: BTreeMap<ObjectID, (ObjectRef, Object)>,
    pub deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
}

pub struct TemporaryStore<S> {
    // The backing store for retrieving Move packages onchain.
    // When executing a Move call, the dependent packages are not going to be
    // in the input objects. They will be fetched from the backing store.
    // Also used for fetching the backing parent_sync to get the last known version for wrapped
    // objects
    store: S,
    tx_digest: TransactionDigest,
    objects: BTreeMap<ObjectID, Object>,
    mutable_inputs: Vec<ObjectRef>,      // Inputs that are mutable
    written: BTreeMap<ObjectID, Object>, // Objects written
    /// Objects actively deleted.
    /// Child count is Some for Normal/UnwrapThenDelete events, and is None for wraps
    deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    /// Ordered sequence of events emitted by execution
    events: Vec<Event>,
    // New object IDs created during the transaction, needed for
    // telling apart unwrapped objects.
    created_object_ids: HashSet<ObjectID>,
}

macro_rules! into_inner {
    ($store:ident) => {{
        let written = $store
            .written
            .into_iter()
            .map(|(id, obj)| (id, (obj.compute_object_reference(), obj)))
            .collect();
        InnerTemporaryStore {
            objects: $store.objects,
            mutable_inputs: $store.mutable_inputs,
            written,
            deleted: $store.deleted,
        }
    }};
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
            objects,
            mutable_inputs,
            written: BTreeMap::new(),
            deleted: BTreeMap::new(),
            events: Vec::new(),
            created_object_ids: HashSet::new(),
        }
    }

    // Helpers to access private fields
    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.objects
    }

    pub fn written(&self) -> &BTreeMap<ObjectID, Object> {
        &self.written
    }

    pub fn deleted(&self) -> &BTreeMap<ObjectID, (SequenceNumber, DeleteKind)> {
        &self.deleted
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(self) -> InnerTemporaryStore {
        #[cfg(debug_assertions)]
        {
            self.check_invariants();
        }
        into_inner!(self)
    }

    /// For every object from active_inputs (i.e. all mutable objects), if they are not
    /// mutated during the transaction execution, force mutating them by incrementing the
    /// sequence number. This is required to achieve safety.
    /// We skip the gas object, because gas object will be updated separately.
    pub fn ensure_active_inputs_mutated(&mut self, gas_object_id: &ObjectID) {
        for (id, _seq, _) in &self.mutable_inputs {
            if id == gas_object_id {
                continue;
            }
            if !self.written.contains_key(id) && !self.deleted.contains_key(id) {
                let mut object = self.objects[id].clone();
                // Active input object must be Move object.
                object.data.try_as_move_mut().unwrap().increment_version();
                self.written.insert(*id, object);
            }
        }
    }

    /// For every object changes, charge gas accordingly. Since by this point we haven't charged gas yet,
    /// the gas object hasn't been mutated yet. Passing in `gas_object_size` so that we can also charge
    /// for the gas object mutation in advance.
    pub fn charge_gas_for_storage_changes(
        &mut self,
        gas_status: &mut SuiGasStatus,
        gas_object: &mut Object,
    ) -> Result<(), ExecutionError> {
        let mut objects_to_update = vec![];
        // Also charge gas for mutating the gas object in advance.
        let gas_object_size = gas_object.object_size_for_gas_metering();
        gas_object.storage_rebate = gas_status.charge_storage_mutation(
            gas_object_size,
            gas_object_size,
            gas_object.storage_rebate,
        )?;
        objects_to_update.push(gas_object.clone());

        for (object_id, object) in &mut self.written {
            let (old_object_size, storage_rebate) =
                if let Some(old_object) = self.objects.get(object_id) {
                    (
                        old_object.object_size_for_gas_metering(),
                        old_object.storage_rebate,
                    )
                } else {
                    (0, 0)
                };
            let new_storage_rebate = gas_status.charge_storage_mutation(
                old_object_size,
                object.object_size_for_gas_metering(),
                storage_rebate,
            )?;
            if !object.is_immutable() {
                // We don't need to set storage rebate for immutable objects, as they will
                // never be deleted.
                object.storage_rebate = new_storage_rebate;
                objects_to_update.push(object.clone());
            }
        }

        for object_id in self.deleted.keys() {
            // If an object is in `self.deleted`, and also in `self.objects`, we give storage rebate.
            // Otherwise if an object is in `self.deleted` but not in `self.objects`, it means this
            // object was unwrapped and then deleted. The rebate would have been provided already when
            // mutating the object that wrapped this object.
            if let Some(old_object) = self.objects.get(object_id) {
                gas_status.charge_storage_mutation(
                    old_object.object_size_for_gas_metering(),
                    0,
                    old_object.storage_rebate,
                )?;
            }
        }

        // Write all objects at the end only if all previous gas charges succeeded.
        // This avoids polluting the temporary store state if this function failed.
        for object in objects_to_update {
            self.write_object(object);
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
        let written = self
            .written
            .iter()
            .map(|(id, obj)| (*id, (obj.compute_object_reference(), obj.owner)))
            .collect::<BTreeMap<_, _>>();

        // In the case of special transactions that don't require a gas object,
        // we don't really care about the effects to gas, just use the input for it.
        let updated_gas_object_info = if gas_object_ref.0 == ObjectID::ZERO {
            (gas_object_ref, Owner::AddressOwner(SuiAddress::default()))
        } else {
            written[&gas_object_ref.0]
        };
        let mut created = vec![];
        let mut mutated = vec![];
        let mut unwrapped = vec![];
        for (id, object_ref_and_owner) in written {
            match (
                self.created_object_ids.contains(&id),
                self.objects.contains_key(&id),
            ) {
                (true, _) => created.push(object_ref_and_owner),
                (false, true) => mutated.push(object_ref_and_owner),
                (false, false) => {
                    // wrapped objects must have their version set to 1 + the last known version in
                    // the `parent_sync`
                    debug_assert!(object_ref_and_owner.0 .1.value() > 1);
                    unwrapped.push(object_ref_and_owner)
                }
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
        let inner = into_inner!(self);

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
            events: self.events,
            dependencies: transaction_dependencies,
        };
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

                self.mutable_inputs.iter().all(|elt| !used.insert(&elt.0))
            },
            "Mutable input neither written nor deleted."
        );

        debug_assert!(
            {
                let input_ids = self.objects.clone().into_keys().collect();
                self.created_object_ids.is_disjoint(&input_ids)
            },
            "Newly created object IDs showed up in the input",
        );
    }
}

impl<S> Storage for TemporaryStore<S> {
    /// Resets any mutations and deletions recorded in the store.
    fn reset(&mut self) {
        self.written.clear();
        self.deleted.clear();
        self.events.clear();
        self.created_object_ids.clear();
    }

    fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        // there should be no read after delete
        debug_assert!(self.deleted.get(id) == None);
        self.written.get(id).or_else(|| self.objects.get(id))
    }

    fn set_create_object_ids(&mut self, ids: HashSet<ObjectID>) {
        self.created_object_ids = ids;
    }

    // Invariant: A key assumption of the write-delete logic
    // is that an entry is not both added and deleted by the
    // caller.

    fn write_object(&mut self, mut object: Object) {
        // there should be no write after delete
        debug_assert!(self.deleted.get(&object.id()) == None);
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
        self.written.insert(object.id(), object);
    }

    fn delete_object(&mut self, id: &ObjectID, version: SequenceNumber, kind: DeleteKind) {
        // there should be no deletion after write
        debug_assert!(self.written.get(id) == None);
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

    fn log_event(&mut self, event: Event) {
        self.events.push(event)
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
                assert!(struct_tag == &m.type_, "Invariant violation: ill-typed object in storage or bad object request from caller\
");
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
