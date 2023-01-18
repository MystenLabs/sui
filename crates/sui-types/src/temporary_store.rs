// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::ops::Neg;

use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_protocol_constants::STORAGE_REBATE_RATE;
use tracing::trace;

use crate::coin::Coin;
use crate::event::BalanceChangeType;
use crate::storage::SingleTxContext;
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

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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
    /// The version to assign to all objects written by the transaction using this store.
    lamport_timestamp: SequenceNumber,
    mutable_input_refs: Vec<ObjectRef>, // Inputs that are mutable
    // When an object is being written, we need to ensure that a few invariants hold.
    // It's critical that we always call write_object to update `written`, instead of writing
    // into written directly.
    written: BTreeMap<ObjectID, (SingleTxContext, Object, WriteKind)>, // Objects written
    /// Objects actively deleted.
    deleted: BTreeMap<ObjectID, (SingleTxContext, SequenceNumber, DeleteKind)>,
    /// Ordered sequence of events emitted by execution
    events: Vec<Event>,
    gas_charged: Option<(SuiAddress, ObjectID, GasCostSummary)>,
}

impl<S> TemporaryStore<S> {
    /// Creates a new store associated with an authority store, and populates it with
    /// initial objects.
    pub fn new(store: S, input_objects: InputObjects, tx_digest: TransactionDigest) -> Self {
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

        // account balances aggregated by Sender, Coin type, and Owner.
        let mut balances: HashMap<(SuiAddress, StructTag, Owner), i128> = HashMap::new();

        // Extract gas id and charged gas amount, this can be None for unmetered transactions.
        let (gas_id, gas_charged) =
            if let Some((sender, coin_id, ref gas_charged)) = self.gas_charged {
                // Safe to unwrap, gas must be an input object.
                let gas = &self.input_objects[&coin_id];
                // Emit event for gas charges.
                events.push(Event::balance_change1(
                    sender,
                    BalanceChangeType::Gas,
                    gas.owner,
                    gas.type_().unwrap(),
                    gas_charged.net_gas_usage().neg() as i128,
                ));
                (Some(coin_id), gas_charged.net_gas_usage() as i128)
            } else {
                // Gas charge can be None for genesis transactions.
                (None, 0)
            };

        for (id, (ctx, mut obj, kind)) in self.written {
            // Update the version for the written object, as long as it is a move object and not a
            // package (whose versions are fixed to 1)
            if let Some(obj) = obj.data.try_as_move_mut() {
                obj.increment_version_to(self.lamport_timestamp);
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

            // Create events for writes
            let old_obj = self.input_objects.get(&id);
            let written_events = Self::create_written_events(
                ctx,
                kind,
                id,
                &obj,
                old_obj,
                gas_id,
                gas_charged,
                &mut balances,
            );
            events.extend(written_events);
            written.insert(id, (obj.compute_object_reference(), obj, kind));
        }

        for (id, (ctx, mut version, kind)) in self.deleted {
            // Update the version, post-delete.
            version.increment_to(self.lamport_timestamp);

            // Create events for each deleted changes
            let deleted_obj = self.input_objects.get(&id);
            let balance = deleted_obj
                .and_then(|o| Coin::extract_balance_if_coin(o).ok())
                .flatten();

            match (deleted_obj, balance) {
                // Object is an owned (provided as input) coin object, create a spend event for the remaining balance.
                (Some(deleted_obj), Some(balance)) => {
                    let balance = balance as i128;
                    let coin_type = deleted_obj.type_().unwrap();
                    *balances
                        .entry((ctx.sender, coin_type.clone(), deleted_obj.owner))
                        .or_default() -= balance;
                }
                // If deleted object is not owned coin, emit a delete event.
                _ => {
                    events.push(Event::DeleteObject {
                        package_id: ctx.package_id,
                        transaction_module: ctx.transaction_module.clone(),
                        sender: ctx.sender,
                        object_id: id,
                        version,
                    });
                }
            };
            deleted.insert(id, (version, kind));
        }

        let balance_events =
            balances
                .into_iter()
                .filter_map(|((sender, coin_type, owner), amount)| {
                    if amount != 0 {
                        let balance_change_type = if amount.is_negative() {
                            BalanceChangeType::Pay
                        } else {
                            BalanceChangeType::Receive
                        };
                        Some(Event::balance_change1(
                            sender,
                            balance_change_type,
                            owner,
                            &coin_type,
                            amount,
                        ))
                    } else {
                        None
                    }
                });
        // Combine object events with move events.
        events.extend(balance_events);
        events.extend(self.events);

        let store = InnerTemporaryStore {
            objects: self.input_objects,
            mutable_inputs: self.mutable_input_refs,
            written,
            deleted,
        };
        (store, events)
    }

    fn create_written_events(
        ctx: SingleTxContext,
        kind: WriteKind,
        id: ObjectID,
        obj: &Object,
        old_obj: Option<&Object>,
        gas_id: Option<ObjectID>,
        gas_charged: i128,
        balances: &mut HashMap<(SuiAddress, StructTag, Owner), i128>,
    ) -> Vec<Event> {
        match (kind, Coin::extract_balance_if_coin(obj), old_obj) {
            // For mutation of existing coin, we need to compute the coin balance delta
            // and emit appropriate event depends on ownership changes
            (WriteKind::Mutate, Ok(Some(_)), Some(old_obj)) => {
                Self::process_coin_balance(&ctx, gas_id, obj, old_obj, gas_charged, balances);
                vec![]
            }
            // For all other coin change (unwrap/create), we emit full balance transfer event to the new address owner.
            (_, Ok(Some(balance)), _) => {
                if let Owner::AddressOwner(_) = obj.owner {
                    *balances
                        .entry((ctx.sender, obj.type_().unwrap().clone(), obj.owner))
                        .or_default() += balance as i128;
                }
                vec![]
            }
            // For non-coin mutation
            (WriteKind::Mutate, Ok(None), old_obj) | (WriteKind::Unwrap, Ok(None), old_obj) => {
                // We emit transfer object event for ownership changes
                // if old object is none (unwrapping object) or if old owner != new owner.
                let mut events = vec![];
                if old_obj.map(|o| o.owner) != Some(obj.owner) {
                    events.push(Event::transfer_object(
                        &ctx,
                        obj.owner,
                        // Safe to unwrap, package cannot mutate
                        obj.data.type_().unwrap().to_string(),
                        obj.id(),
                        obj.version(),
                    ));
                }
                // Emit mutate event if there are data changes.
                if old_obj.is_some() && old_obj.unwrap().data != obj.data {
                    events.push(Event::MutateObject {
                        package_id: ctx.package_id,
                        transaction_module: ctx.transaction_module,
                        sender: ctx.sender,
                        object_type: obj.data.type_().unwrap().to_string(),
                        object_id: obj.id(),
                        version: obj.version(),
                    });
                }
                events
            }
            // For create object, if the object type is package, emit a Publish event, else emit NewObject event.
            (WriteKind::Create, Ok(None), _) => {
                vec![if obj.is_package() {
                    Event::Publish {
                        sender: ctx.sender,
                        package_id: id,
                        version: obj.version(),
                        digest: obj.digest(),
                    }
                } else {
                    Event::new_object(
                        &ctx,
                        obj.owner,
                        obj.type_().unwrap().to_string(),
                        id,
                        obj.version(),
                    )
                }]
            }
            _ => vec![],
        }
    }

    fn process_coin_balance(
        ctx: &SingleTxContext,
        gas_id: Option<ObjectID>,
        coin: &Object,
        old_coin: &Object,
        gas_charged: i128,
        balances: &mut HashMap<(SuiAddress, StructTag, Owner), i128>,
    ) {
        // We know this is a coin, safe to unwrap.
        let coin_object_type = coin.type_().unwrap();
        let old_balance = Coin::extract_balance_if_coin(old_coin);
        let balance = Coin::extract_balance_if_coin(coin);

        if let (Ok(Some(old_balance)), Ok(Some(balance))) = (old_balance, balance) {
            let old_balance = old_balance as i128;
            let balance = balance as i128;

            // Deduct gas from the old balance if the object is also the gas coin.
            let old_balance = if Some(coin.id()) == gas_id {
                old_balance - gas_charged
            } else {
                old_balance
            };

            match (old_coin.owner == coin.owner, old_balance.cmp(&balance)) {
                // same owner, old balance > new balance, spending balance.
                // For the spend event, we are spending from the old coin so the event will use the old coin version and owner info.
                (true, Ordering::Greater) => {
                    *balances
                        .entry((ctx.sender, coin_object_type.clone(), old_coin.owner))
                        .or_default() += balance - old_balance
                }
                // Same owner, balance increased.
                (true, Ordering::Less) => {
                    *balances
                        .entry((ctx.sender, coin_object_type.clone(), coin.owner))
                        .or_default() += balance - old_balance
                }
                // ownership changed, add an event for spending and one for receiving.
                (false, _) => {
                    *balances
                        .entry((ctx.sender, coin_object_type.clone(), old_coin.owner))
                        .or_default() -= old_balance;

                    *balances
                        .entry((ctx.sender, coin_object_type.clone(), coin.owner))
                        .or_default() += balance;
                }
                _ => {}
            }
        };
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
                // We cannot update here but have to push to `to_be_updated` and update later
                // because the for loop is holding a reference to `self`, and calling
                // `self.write_object` requires a mutable reference to `self`.
                to_be_updated.push(self.input_objects[id].clone());
            }
        }
        for object in to_be_updated {
            // The object must be mutated as it was present in the input objects
            self.write_object(
                &SingleTxContext::unused_input(sender),
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
            SingleTxContext::gas(sender),
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
        mut self,
        shared_object_refs: Vec<ObjectRef>,
        transaction_digest: &TransactionDigest,
        transaction_dependencies: Vec<TransactionDigest>,
        gas_cost_summary: GasCostSummary,
        status: ExecutionStatus,
        gas_object_ref: ObjectRef,
    ) -> (InnerTemporaryStore, TransactionEffects) {
        let mut modified_at_versions = vec![];

        // Remember the versions objects were updated from in case of rollback.
        self.written.iter_mut().for_each(|(id, (_, obj, kind))| {
            if *kind == WriteKind::Mutate {
                modified_at_versions.push((*id, obj.version()))
            }
        });

        self.deleted.iter_mut().for_each(|(id, (_, version, _))| {
            modified_at_versions.push((*id, *version));
        });

        let (inner, events) = self.into_inner();

        // In the case of special transactions that don't require a gas object,
        // we don't really care about the effects to gas, just use the input for it.
        let updated_gas_object_info = if gas_object_ref.0 == ObjectID::ZERO {
            (gas_object_ref, Owner::AddressOwner(SuiAddress::default()))
        } else {
            let (obj_ref, object, _kind) = &inner.written[&gas_object_ref.0];
            (*obj_ref, object.owner)
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
        for (id, (version, kind)) in &inner.deleted {
            match kind {
                DeleteKind::Normal | DeleteKind::UnwrapThenDelete => {
                    deleted.push((*id, *version, ObjectDigest::OBJECT_DIGEST_DELETED))
                }
                DeleteKind::Wrap => {
                    wrapped.push((*id, *version, ObjectDigest::OBJECT_DIGEST_WRAPPED))
                }
            }
        }

        let effects = TransactionEffects {
            status,
            gas_used: gas_cost_summary,
            modified_at_versions,
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

    pub fn write_object(&mut self, ctx: &SingleTxContext, mut object: Object, kind: WriteKind) {
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
        self.written
            .insert(object.id(), (ctx.clone(), object, kind));
    }

    pub fn charge_gas<T>(
        &mut self,
        sender: SuiAddress,
        gas_object_id: ObjectID,
        gas_status: &mut SuiGasStatus<'_>,
        result: &mut Result<T, ExecutionError>,
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
        let gas_rebate = (cost_summary.storage_rebate as f64 * STORAGE_REBATE_RATE).round() as u64;
        // We must re-fetch the gas object from the temporary store, as it may have been reset
        // previously in the case of error.
        let mut gas_object = self.read_object(&gas_object_id).unwrap().clone();
        gas::deduct_gas(&mut gas_object, gas_used, gas_rebate);
        trace!(gas_used, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");

        // Do not overwrite inner transaction context for gas charge
        let ctx = if let Some((ctx, ..)) = self.written.get(&gas_object_id) {
            ctx.clone()
        } else {
            SingleTxContext::gas(sender)
        };
        self.write_object(&ctx, gas_object, WriteKind::Mutate);
        self.gas_charged = Some((sender, gas_object_id, cost_summary));
    }

    pub fn delete_object(
        &mut self,
        ctx: &SingleTxContext,
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

        // For object deletion, we will increment the version when converting the store to effects
        // so the object will eventually show up in the parent_sync table with a new version.
        self.deleted.insert(*id, (ctx.clone(), version, kind));
    }

    /// Resets any mutations and deletions recorded in the store.
    pub fn reset(&mut self) {
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
            .map(|(_, obj, _kind)| obj)
            .or_else(|| self.input_objects.get(id))
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
        let obj_opt = self.written.get(child).map(|(_, obj, _kind)| obj);
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
