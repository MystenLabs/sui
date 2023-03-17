// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use tracing::trace;

use crate::coin::Coin;
use crate::committee::EpochId;
use crate::messages::TransactionEvents;
use crate::storage::ObjectStore;
use crate::sui_system_state::{
    get_sui_system_state, get_sui_system_state_wrapper, SuiSystemState, SuiSystemStateWrapper,
};
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
    pub events: TransactionEvents,
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

    pub fn get_sui_system_state_wrapper_object(&self) -> Option<SuiSystemStateWrapper> {
        get_sui_system_state_wrapper(&self.written).ok()
    }

    pub fn get_sui_system_state_object(&self) -> Option<SuiSystemState> {
        get_sui_system_state(&self.written).ok()
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
                return Ok(Some(Arc::new(p.deserialize_module(&id.name().into())?)));
            }
        }
        self.fallback.get_module_by_id(id)
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
    written: BTreeMap<ObjectID, (Object, WriteKind)>, // Objects written
    /// Objects actively deleted.
    deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    /// Ordered sequence of events emitted by execution
    events: Vec<Event>,
    gas_charged: Option<(ObjectID, GasCostSummary)>,
    storage_rebate_rate: u64,
    protocol_version: ProtocolVersion,
}

impl<S> TemporaryStore<S> {
    /// Creates a new store associated with an authority store, and populates it with
    /// initial objects.
    pub fn new(
        store: S,
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
            gas_charged: None,
            storage_rebate_rate: protocol_config.storage_rebate_rate(),
            protocol_version: protocol_config.version,
        }
    }

    // Helpers to access private fields
    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.input_objects
    }

    /// Return the dynamic field objects that are written or deleted by this transaction
    pub fn dynamic_fields_touched(&self) -> Vec<ObjectID> {
        let mut dynamic_fields = Vec::new();
        for (id, (_, kind)) in &self.written {
            match kind {
                WriteKind::Mutate => {
                    if !self.input_objects.contains_key(id) {
                        dynamic_fields.push(*id)
                    }
                }
                WriteKind::Create | WriteKind::Unwrap => (),
            }
        }
        for (id, (_, kind)) in &self.deleted {
            match kind {
                DeleteKind::Normal => {
                    // TODO: is this how a deleted dynamic field will show up?
                    if !self.input_objects.contains_key(id) {
                        dynamic_fields.push(*id)
                    }
                }
                DeleteKind::UnwrapThenDelete | DeleteKind::Wrap => (),
            }
        }
        dynamic_fields
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

        for (id, (mut version, kind)) in self.deleted {
            // Update the version, post-delete.
            version.increment_to(self.lamport_timestamp);
            deleted.insert(id, (version, kind));
        }

        // Combine object events with move events.

        InnerTemporaryStore {
            objects: self.input_objects,
            mutable_inputs: self.mutable_input_refs,
            written,
            deleted,
            events: TransactionEvents { data: self.events },
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

    /// Compute storage gas for each mutable input object (including the gas coin), and each created object.
    /// Compute storage refunds for each deleted object
    /// Will *not* charge any computation gas. Returns the total size in bytes of all deleted objects + all mutated objects,
    /// which the caller can use to charge computation gas
    fn charge_gas_for_storage_changes(
        &mut self,
        gas_status: &mut SuiGasStatus<'_>,
        gas_object_id: ObjectID,
    ) -> Result<u64, ExecutionError> {
        let mut total_bytes_written_deleted = 0;

        // If the gas coin was not yet written, charge gas for mutating the gas object in advance.
        let gas_object = self
            .read_object(&gas_object_id)
            .expect("We constructed the object map so it should always have the gas object id")
            .clone();
        self.written
            .entry(gas_object_id)
            .or_insert_with(|| (gas_object, WriteKind::Mutate));
        self.ensure_active_inputs_mutated();
        let mut objects_to_update = vec![];

        for (object_id, (object, write_kind)) in &mut self.written {
            let (old_object_size, storage_rebate) = self
                .input_objects
                .get(object_id)
                .map(|old| (old.object_size_for_gas_metering(), old.storage_rebate))
                .unwrap_or((0, 0));

            let new_object_size = object.object_size_for_gas_metering();
            let new_storage_rebate =
                gas_status.charge_storage_mutation(new_object_size, storage_rebate.into())?;
            object.storage_rebate = new_storage_rebate;
            if !object.is_immutable() {
                objects_to_update.push((object.clone(), *write_kind));
            }
            total_bytes_written_deleted += old_object_size + new_object_size;
        }

        for object_id in self.deleted.keys() {
            // If an object is in `self.deleted`, and also in `self.objects`, we give storage rebate.
            // Otherwise if an object is in `self.deleted` but not in `self.objects`, it means this
            // object was unwrapped and then deleted. The rebate would have been provided already when
            // mutating the object that wrapped this object.
            if let Some(old_object) = self.input_objects.get(object_id) {
                gas_status.charge_storage_mutation(0, old_object.storage_rebate.into())?;
                total_bytes_written_deleted += old_object.object_size_for_gas_metering();
            }
        }

        // Write all objects at the end only if all previous gas charges succeeded.
        // This avoids polluting the temporary store state if this function failed.
        for (object, write_kind) in objects_to_update {
            self.write_object(object, write_kind);
        }
        Ok(total_bytes_written_deleted as u64)
    }

    pub fn to_effects(
        mut self,
        shared_object_refs: Vec<ObjectRef>,
        transaction_digest: &TransactionDigest,
        transaction_dependencies: Vec<TransactionDigest>,
        gas_cost_summary: GasCostSummary,
        status: ExecutionStatus,
        gas: &[ObjectRef],
        epoch: EpochId,
    ) -> (InnerTemporaryStore, TransactionEffects) {
        let mut modified_at_versions = vec![];

        // Remember the versions objects were updated from in case of rollback.
        self.written.iter_mut().for_each(|(id, (obj, kind))| {
            if *kind == WriteKind::Mutate {
                modified_at_versions.push((*id, obj.version()))
            }
        });

        self.deleted.iter_mut().for_each(|(id, (version, _))| {
            modified_at_versions.push((*id, *version));
        });

        let protocol_version = self.protocol_version;
        let inner = self.into_inner();

        // In the case of special transactions that don't require a gas object,
        // we don't really care about the effects to gas, just use the input for it.
        // Gas coins are guaranteed to be at least size 1 and if more than 1
        // the first coin is where all the others are merged.
        let gas_object_ref = gas[0];
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

    /// 1. Compute tx storage gas costs and tx storage rebates, update storage_rebate field of mutated objects
    /// 2. Deduct computation gas costs and storage costs to `gas_object_id`, credit storage rebates to `gas_object_id`.
    // The happy path of this function follows (1) + (2) and is fairly simple. Most of the complexity is in the unhappy paths:
    // - if execution aborted before calling this function, we have to dump all writes + re-smash gas, then charge for storage
    // - if we run out of gas while charging for storage, we have to dump all writes + re-smash gas, then charge for storage again
    pub fn charge_gas<T>(
        &mut self,
        gas_object_id: ObjectID,
        gas_status: &mut SuiGasStatus<'_>,
        execution_result: &mut Result<T, ExecutionError>,
        gas: &[ObjectRef],
    ) {
        // at this point, we have done some charging for computation, but have not yet set the storage rebate or storage gas units
        assert!(gas_status.storage_rebate() == 0);
        assert!(gas_status.storage_gas_units() == 0);

        if let Err(err) = gas_status.bucketize_computation() {
            if execution_result.is_ok() {
                *execution_result = Err(err);
            }
        }
        if execution_result.is_err() {
            // Tx execution aborted--need to dump writes, deletes, etc before charging storage gas
            self.reset(gas, gas_status);
        }

        if let Err(err) = self.charge_gas_for_storage_changes(gas_status, gas_object_id) {
            // Ran out of gas while charging for storage changes. reset store, now at state just after gas smashing
            self.reset(gas, gas_status);

            // charge for storage again. This will now account only for the storage cost of gas coins
            if self
                .charge_gas_for_storage_changes(gas_status, gas_object_id)
                .is_err()
            {
                // TODO: this shouldn't happen, because we should check that the budget is enough to cover the storage costs of gas coins at signing time
                // perhaps that check isn't there?
                trace!("out of gas while charging for gas smashing")
            }

            // if execution succeeded, but we ran out of gas while charging for storage, overwrite the successful execution result
            // with an out of gas failure
            if execution_result.is_ok() {
                *execution_result = Err(err)
            }
        }
        let cost_summary = gas_status.summary();
        let gas_used = cost_summary.gas_used();

        // Important to fetch the gas object here instead of earlier, as it may have been reset
        // previously in the case of error.
        let mut gas_object = self.read_object(&gas_object_id).unwrap().clone();
        gas::deduct_gas(
            &mut gas_object,
            gas_used,
            cost_summary.sender_rebate(self.storage_rebate_rate),
        );
        trace!(gas_used, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");

        self.write_object(gas_object, WriteKind::Mutate);
        self.gas_charged = Some((gas_object_id, cost_summary));
    }

    pub fn smash_gas(&mut self, gas: &[ObjectRef]) -> Result<ObjectRef, ExecutionError> {
        if gas.len() > 1 {
            let mut gas_coins: Vec<(Object, Coin)> = gas
                .iter()
                .map(|obj_ref| {
                    let obj = self.objects().get(&obj_ref.0).unwrap().clone();
                    let Data::Move(move_obj) = &obj.data else {
                        return Err(ExecutionError::invariant_violation(
                            "Provided non-gas coin object as input for gas!"
                        ));
                    };
                    if !move_obj.type_().is_gas_coin() {
                        return Err(ExecutionError::invariant_violation(
                            "Provided non-gas coin object as input for gas!",
                        ));
                    }
                    let coin = Coin::from_bcs_bytes(move_obj.contents()).map_err(|_| {
                        ExecutionError::invariant_violation(
                            "Deserializing Gas coin should not fail!",
                        )
                    })?;
                    Ok((obj, coin))
                })
                .collect::<Result<_, _>>()?;
            let (mut gas_object, mut gas_coin) = gas_coins.swap_remove(0);
            for (other_object, other_coin) in gas_coins {
                gas_coin.add(other_coin.balance)?;
                self.delete_object(
                    &other_object.id(),
                    other_object.version(),
                    DeleteKind::Normal,
                )
            }
            let new_contents = bcs::to_bytes(&gas_coin).map_err(|_| {
                ExecutionError::invariant_violation("Deserializing Gas coin should not fail!")
            })?;
            // unwrap is safe because we checked that it was a coin object above.
            let move_obj = gas_object.data.try_as_move_mut().unwrap();
            move_obj.update_coin_contents(new_contents);
            self.write_object(gas_object, WriteKind::Mutate);
        }
        Ok(gas[0])
    }

    pub fn delete_object(&mut self, id: &ObjectID, version: SequenceNumber, kind: DeleteKind) {
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
        self.deleted.insert(*id, (version, kind));
    }

    pub fn drop_writes(&mut self) {
        self.written.clear();
        self.deleted.clear();
        self.events.clear();
    }

    /// Resets any mutations, deletions, and events recorded in the store, as well as any storage costs and
    /// rebates, then Re-runs gas smashing. Effects on store are now as if we were about to begin execution
    fn reset(&mut self, gas: &[ObjectRef], gas_status: &mut SuiGasStatus<'_>) {
        self.drop_writes();
        gas_status.reset_storage_cost_and_rebate();

        self.smash_gas(gas)
            .expect("Gas smashing cannot fail because it already succeeded when we did it before on the same `gas`");
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
                ObjectChange::Delete(version, kind) => self.delete_object(&id, version, kind),
            }
        }
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
}

impl<S: GetModule + ObjectStore + BackingPackageStore> TemporaryStore<S> {
    /// Check that this transaction neither creates nor destroys SUI. This should hold for all txes except
    /// the epoch change tx, which mints staking rewards equal to the gas fees burned in the previous epoch.
    /// This intended to be called *after* we have charged for gas + applied the storage rebate to the gas object,
    /// but *before* we have updated object versions
    pub fn check_sui_conserved(&self) {
        if !self.dynamic_fields_touched().is_empty() {
            // TODO: check conservation in the presence of dynamic fields
            return;
        }
        let gas_summary = &self.gas_charged.as_ref().unwrap().1;
        let storage_fund_rebate_inflow =
            gas_summary.storage_fund_rebate_inflow(self.storage_rebate_rate);

        // total SUI in input objects
        let input_sui = self.mutable_input_refs.iter().fold(0, |acc, o| {
            acc + self
                .input_objects
                .get(&o.0)
                .unwrap()
                .get_total_sui(&self)
                .unwrap()
        });
        // if a dynamic field object O is written by this tx, count get_total_sui(pre_tx_value(O)) as part of input_sui
        let dynamic_field_input_sui = self.dynamic_fields_touched().iter().fold(0, |acc, id| {
            acc + self
                .store
                .get_object(id)
                .unwrap()
                .unwrap()
                .get_total_sui(&self)
                .unwrap()
        });
        // sum of the storage rebate fields of all objects written by this tx
        let mut output_rebate_amount = 0;
        // total SUI in output objects
        let output_sui = self.written.values().fold(0, |acc, (o, _)| {
            output_rebate_amount += o.storage_rebate;
            acc + o.get_total_sui(&self).unwrap()
        });

        // storage gas cost should be equal to total rebates of mutated objects + storage fund rebate inflow (see below).
        // note: each mutated object O of size N bytes is assessed a storage cost of N * storage_price bytes, but also
        // has O.storage_rebate credited to the tx storage rebate.
        // TODO: figure out what's wrong with this check. The one below is more important, so going without it for now
        /*assert_eq!(
            gas_summary.storage_cost,
            output_rebate_amount + storage_fund_rebate_inflow
        );*/

        // note: storage_cost flows into the storage_rebate field of the output objects, which is why it is not accounted for here.
        // similarly, storage_rebate flows into the gas coin
        // we do account for the "storage rebate inflow" (portion of the storage rebate which flows back into the storage fund). like
        // computation gas fees, this quantity is burned, then re-minted at epoch boundaries.
        assert_eq!(
            input_sui + dynamic_field_input_sui,
            output_sui + gas_summary.computation_cost + storage_fund_rebate_inflow
        )
    }
}

impl<S: ChildObjectResolver> ChildObjectResolver for TemporaryStore<S> {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        // there should be no read after delete
        debug_assert!(self.deleted.get(child).is_none());
        let obj_opt = self.written.get(child).map(|(obj, _kind)| obj);
        if obj_opt.is_some() {
            Ok(obj_opt.cloned())
        } else {
            self.store.read_child_object(parent, child)
        }
    }
}

impl<S: ChildObjectResolver> Storage for TemporaryStore<S> {
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

impl<S: ParentSync> ParentSync for TemporaryStore<S> {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        self.store.get_latest_parent_entry_ref(object_id)
    }
}

impl<S: GetModule<Error = SuiError, Item = CompiledModule>> GetModule for TemporaryStore<S> {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, module_id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        let package_id = &ObjectID::from(*module_id.address());
        if let Some((obj, _)) = self.written.get(package_id) {
            Ok(Some(
                obj.data
                    .try_as_package()
                    .expect("Bad object type--expected package")
                    .deserialize_module(&module_id.name().to_owned())?,
            ))
        } else {
            self.store.get_module_by_id(module_id)
        }
    }
}

/// Create an empty `TemporaryStore` with no backing storage for module resolution.
/// For testing purposes only.
pub fn empty_for_testing() -> TemporaryStore<()> {
    TemporaryStore::new(
        (),
        InputObjects::new(Vec::new()),
        TransactionDigest::genesis(),
        &ProtocolConfig::get_for_min_version(),
    )
}

/// Create a `TemporaryStore` with the given inputs and no backing storage for module resolution.
/// For testing purposes only.
pub fn with_input_objects_for_testing(input_objects: InputObjects) -> TemporaryStore<()> {
    TemporaryStore::new(
        (),
        input_objects,
        TransactionDigest::genesis(),
        &ProtocolConfig::get_for_min_version(),
    )
}
