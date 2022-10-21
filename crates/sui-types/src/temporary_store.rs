// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ops::Neg;

use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use tracing::trace;

use crate::coin::Coin;
use crate::storage::InnerTxContext;
use crate::{
    base_types::{
        ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    },
    error::{ExecutionError, SuiError, SuiResult},
    event::Event,
    fp_bail, gas,
    gas::{GasCostSummary, SuiGasStatus},
    messages::{ExecutionStatus, InputObjects, TransactionEffects},
    object::Owner,
    object::{Data, Object},
    storage::{
        BackingPackageStore, ChildObjectResolver, DeleteKind, ObjectChange, ParentSync, Storage,
        WriteKind,
    },
};

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
    written: BTreeMap<ObjectID, (InnerTxContext, Object, WriteKind)>, // Objects written
    /// Objects actively deleted.
    deleted: BTreeMap<ObjectID, (InnerTxContext, SequenceNumber, DeleteKind)>,
    /// Ordered sequence of events emitted by execution
    events: Vec<Event>,
    gas_charged: Option<(SuiAddress, ObjectID, GasCostSummary)>,
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
            written: BTreeMap::new(),
            deleted: BTreeMap::new(),
            events: Vec::new(),
            gas_charged: None,
        }
    }

    // Helpers to access private fields
    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.input_objects
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(self) -> (InnerTemporaryStore, Vec<Event>) {
        #[cfg(debug_assertions)]
        {
            self.check_invariants();
        }

        let mut written = BTreeMap::new();
        let mut deleted = BTreeMap::new();
        let mut events = Vec::new();

        let (gas_id, gas_charged) =
            if let Some((sender, coin_id, ref gas_charged)) = self.gas_charged {
                let gas = &self.input_objects[&coin_id];
                // Emit event for gas charges.
                events.push(Event::balance_change(
                    &InnerTxContext::gas(sender),
                    gas.owner,
                    coin_id,
                    gas.version(),
                    gas.type_().unwrap(),
                    gas_charged.net_gas_usage().neg() as i128,
                ));
                (Some(coin_id), gas_charged.net_gas_usage() as i128)
            } else {
                (None, 0)
            };

        for (id, (ctx, obj, kind)) in self.written {
            match kind {
                WriteKind::Mutate => {
                    let old_obj = &self.input_objects[&obj.id()];
                    // Emit coin balance change event for Coin<T> objects
                    if Coin::is_coin(obj.type_().unwrap()) {
                        let new_events = Self::create_balance_change_events(
                            ctx,
                            gas_id,
                            &obj,
                            old_obj,
                            gas_charged,
                        );
                        events.extend(new_events);
                    } else if old_obj.owner != obj.owner {
                        events.push(Event::transfer_object(
                            &ctx,
                            obj.owner,
                            // Safe to unwrap, package cannot mutate
                            obj.data.type_().unwrap().to_string(),
                            obj.id(),
                            obj.version(),
                        ));
                    }
                    // not coin balance change or ownership transfer, do not emit event.
                }
                WriteKind::Create => events.push(
                    if let Some(balance) = Coin::extract_balance_if_coin(&obj).ok().flatten() {
                        Event::balance_change(
                            &ctx,
                            obj.owner,
                            obj.id(),
                            obj.version(),
                            obj.type_().unwrap(),
                            balance as i128,
                        )
                    } else {
                        if obj.is_package() {
                            Event::Publish {
                                sender: ctx.sender,
                                package_id: id,
                            }
                        } else {
                            Event::new_object(&ctx, obj.owner, obj.type_().unwrap().to_string(), id)
                        }
                    },
                ),
                WriteKind::Unwrap => {
                    if let Ok(Some(amount)) = Coin::extract_balance_if_coin(&obj) {
                        events.push(Event::balance_change(
                            &ctx,
                            obj.owner,
                            id,
                            obj.version(),
                            obj.type_().unwrap(),
                            amount as i128,
                        ));
                    } else {
                        events.push(Event::transfer_object(
                            &ctx,
                            obj.owner,
                            // Safe to unwrap, package cannot mutate
                            obj.data.type_().unwrap().to_string(),
                            obj.id(),
                            obj.version(),
                        ));
                    }
                }
            };
            written.insert(id, (obj.compute_object_reference(), obj, kind));
        }

        for (id, (ctx, version, kind)) in self.deleted {
            match kind {
                DeleteKind::Normal | DeleteKind::Wrap => {
                    let deleted_obj = &self.input_objects[&id];
                    if let Ok(Some(balance)) = Coin::extract_balance_if_coin(deleted_obj) {
                        let balance = balance as i128;
                        events.push(Event::balance_change(
                            &ctx,
                            deleted_obj.owner,
                            id,
                            deleted_obj.version(),
                            deleted_obj.type_().unwrap(),
                            balance.neg(),
                        ));
                    } else {
                        events.push(Event::DeleteObject {
                            package_id: ctx.package_id,
                            transaction_module: ctx.transaction_module.clone(),
                            sender: ctx.sender,
                            object_id: id,
                            version,
                        });
                    };
                }
                DeleteKind::UnwrapThenDelete => {
                    events.push(Event::DeleteObject {
                        package_id: ctx.package_id,
                        transaction_module: ctx.transaction_module.clone(),
                        sender: ctx.sender,
                        object_id: id,
                        version,
                    });
                }
            }
            deleted.insert(id, (version, kind));
        }

        events.extend(self.events);

        (
            InnerTemporaryStore {
                objects: self.input_objects,
                mutable_inputs: self.mutable_input_refs,
                written,
                deleted,
            },
            events,
        )
    }

    fn create_balance_change_events(
        ctx: InnerTxContext,
        gas_id: Option<ObjectID>,
        coin: &Object,
        old_coin: &Object,
        gas_charged: i128,
    ) -> Vec<Event> {
        let balance = Coin::extract_balance_if_coin(coin).unwrap().unwrap() as i128;
        let coin_object_type = coin.type_().unwrap();
        let old_balance = Coin::extract_balance_if_coin(old_coin).unwrap().unwrap() as i128;
        let old_balance = if Some(coin.id()) == gas_id {
            old_balance - gas_charged
        } else {
            old_balance
        };
        let mut events = vec![];
        if old_coin.owner == coin.owner {
            // same owner
            if old_balance > balance {
                // spend coin
                events.push(Event::balance_change(
                    &ctx,
                    old_coin.owner,
                    old_coin.id(),
                    old_coin.version(),
                    coin_object_type,
                    balance - old_balance,
                ));
            } else if old_balance < balance {
                // receive
                events.push(Event::balance_change(
                    &ctx,
                    old_coin.owner,
                    coin.id(),
                    coin.version(),
                    coin_object_type,
                    balance - old_balance,
                ));
            }
            // do not emit event if no balance change
        } else {
            // ownership transferred, add an event for spending and one for receiving.
            events.push(Event::balance_change(
                &ctx,
                old_coin.owner,
                coin.id(),
                old_coin.version(),
                coin_object_type,
                old_balance.neg(),
            ));
            events.push(Event::balance_change(
                &ctx,
                coin.owner,
                coin.id(),
                coin.version(),
                coin_object_type,
                balance,
            ));
        }
        events
    }

    /// For every object from active_inputs (i.e. all mutable objects), if they are not
    /// mutated during the transaction execution, force mutating them by incrementing the
    /// sequence number. This is required to achieve safety.
    /// We skip the gas object, because gas object will be updated separately.
    pub fn ensure_active_inputs_mutated(&mut self, sender: SuiAddress, gas_object_id: &ObjectID) {
        let mut to_be_updated = vec![];
        for (id, _seq, _) in &self.mutable_input_refs {
            if id == gas_object_id {
                continue;
            }
            if !self.written.contains_key(id) && !self.deleted.contains_key(id) {
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
            self.write_object(
                &InnerTxContext::unused_input(sender),
                object,
                WriteKind::Mutate,
            );
        }
    }

    /// For every object changes, charge gas accordingly. Since by this point we haven't charged gas yet,
    /// the gas object hasn't been mutated yet. Passing in `gas_object_size` so that we can also charge
    /// for the gas object mutation in advance.
    pub fn charge_gas_for_storage_changes(
        &mut self,
        sender: SuiAddress,
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
        objects_to_update.push((
            InnerTxContext::gas(sender),
            gas_object.clone(),
            WriteKind::Mutate,
        ));

        for (object_id, (ctx, object, write_kind)) in &mut self.written {
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
                objects_to_update.push((ctx.clone(), object.clone(), *write_kind));
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
        for (ctx, object, write_kind) in objects_to_update {
            self.write_object(&ctx, object, write_kind);
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
            .written
            .iter()
            .map(|(id, (_, obj, write_kind))| {
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
        for (id, (_, version, kind)) in &self.deleted {
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
                    .all(|(_, (_, obj, _))| obj.previous_transaction == self.tx_digest)
            },
            "Object previous transaction not properly set",
        );
    }

    // Invariant: A key assumption of the write-delete logic
    // is that an entry is not both added and deleted by the
    // caller.

    pub fn write_object(&mut self, ctx: &InnerTxContext, mut object: Object, kind: WriteKind) {
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
        self.written
            .insert(object.id(), (ctx.clone(), object, kind));
    }

    pub fn charge_gas(
        &mut self,
        sender: SuiAddress,
        gas_object_id: ObjectID,
        gas_status: &mut SuiGasStatus<'_>,
        result: &mut Result<(), ExecutionError>,
    ) {
        // We must call `read_object` instead of getting it from `temporary_store.objects`
        // because a `TransferSui` transaction may have already mutated the gas object and put
        // it in `temporary_store.written`.
        let mut gas_object = self
            .read_object(&gas_object_id)
            .expect("We constructed the object map so it should always have the gas object id")
            .clone();
        trace!(?gas_object_id, "Obtained gas object");
        if let Err(err) = self.charge_gas_for_storage_changes(sender, gas_status, &mut gas_object) {
            // If `result` is already `Err`, we basically have two errors at the same time.
            // Users should be generally more interested in the actual execution error, so we
            // let that shadow the out of gas error. Also in this case, we don't need to reset
            // the `temporary_store` because `charge_gas_for_storage_changes` won't mutate
            // `temporary_store` if gas charge failed.
            //
            // If `result` is `Ok`, now we failed when charging gas, we have to reset
            // the `temporary_store` to eliminate all effects caused by the execution,
            // and re-ensure all mutable objects' versions are incremented.
            if result.is_ok() {
                self.reset();
                self.ensure_active_inputs_mutated(sender, &gas_object_id);
                *result = Err(err);
            }
        }
        let cost_summary = gas_status.summary(result.is_ok());
        let gas_used = cost_summary.gas_used();
        let gas_rebate = cost_summary.storage_rebate;
        // We must re-fetch the gas object from the temporary store, as it may have been reset
        // previously in the case of error.
        let mut gas_object = self.read_object(&gas_object_id).unwrap().clone();
        gas::deduct_gas(&mut gas_object, gas_used, gas_rebate);
        trace!(gas_used, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");

        // Do not overwrite inner transaction context for gas charge
        let ctx = if let Some((ctx, ..)) = self.written.get(&gas_object_id) {
            ctx.clone()
        } else {
            InnerTxContext::gas(sender)
        };
        self.write_object(&ctx, gas_object, WriteKind::Mutate);
        self.gas_charged = Some((sender, gas_object_id, cost_summary));
    }

    pub fn delete_object(
        &mut self,
        ctx: &InnerTxContext,
        id: &ObjectID,
        version: SequenceNumber,
        kind: DeleteKind,
    ) {
        // there should be no deletion after write
        debug_assert!(self.written.get(id).is_none());
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
        self.deleted
            .insert(*id, (ctx.clone(), version.increment(), kind));
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
        self.written
            .get(id)
            .map(|(_, obj, _kind)| obj)
            .or_else(|| self.input_objects.get(id))
    }
}

impl<S> Storage for TemporaryStore<S> {
    /// Resets any mutations and deletions recorded in the store.
    fn reset(&mut self) {
        self.written.clear();
        self.deleted.clear();
        self.events.clear();
    }

    pub fn apply_object_changes(&mut self, changes: BTreeMap<ObjectID, ObjectChange>) {
        for (id, change) in changes {
            match change {
                ObjectChange::Write(ctx, new_value, kind) => {
                    self.write_object(&ctx, new_value, kind)
                }
                ObjectChange::Delete(ctx, version, kind) => {
                    self.delete_object(&ctx, &id, version, kind)
                }
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
