// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::{LinkageResolver, ModuleResolver, ResourceResolver};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_protocol_config::ProtocolConfig;
use tracing::trace;

use crate::committee::EpochId;
use crate::effects::{TransactionEffects, TransactionEvents};
use crate::execution_status::ExecutionStatus;
use crate::storage::ObjectStore;
use crate::sui_system_state::{
    get_sui_system_state, get_sui_system_state_wrapper, AdvanceEpochParams, SuiSystemState,
};
use crate::{
    base_types::{
        ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    },
    error::{ExecutionError, SuiError, SuiResult},
    event::Event,
    fp_bail, gas,
    gas::{GasCostSummary, SuiGasStatus, SuiGasStatusAPI},
    messages::InputObjects,
    object::Owner,
    object::{Data, Object},
    storage::{
        BackingPackageStore, ChildObjectResolver, DeleteKind, ObjectChange, ParentSync, Storage,
        WriteKind,
    },
};
use crate::{is_system_package, SUI_SYSTEM_STATE_OBJECT_ID};

pub type WrittenObjects = BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>;
pub type ObjectMap = BTreeMap<ObjectID, Object>;
pub type TxCoins = (ObjectMap, WrittenObjects);

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct InnerTemporaryStore {
    pub objects: ObjectMap,
    pub mutable_inputs: Vec<ObjectRef>,
    pub written: WrittenObjects,
    // deleted or wrapped or unwrap-then-delete
    pub deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    pub loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    pub events: TransactionEvents,
    pub max_binary_format_version: u32,
    pub no_extraneous_module_bytes: bool,
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
    /// Child objects loaded during dynamic field opers
    /// Currently onply populated for full nodes, not for validators
    loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    /// Ordered sequence of events emitted by execution
    events: Vec<Event>,
    gas_charged: Option<(ObjectID, GasCostSummary)>,
    storage_rebate_rate: u64,
    protocol_config: ProtocolConfig,
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
            protocol_config: protocol_config.clone(),
            loaded_child_objects: BTreeMap::new(),
        }
    }

    /// WARNING! Should only be used for dry run and dev inspect!
    /// In dry run and dev inspect, you might load a dynamic field that is actually too new for
    /// the transaction. Ideally, we would want to load the "correct" dynamic fields, but as that
    /// is not easily determined, we instead set the lamport version MAX, which is a valid lamport
    /// version for any object used in the transaction (preventing internal assertions or
    /// invariant violations from being triggered)
    pub fn new_for_mock_transaction(
        store: S,
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
            gas_charged: None,
            storage_rebate_rate: protocol_config.storage_rebate_rate(),
            protocol_config: protocol_config.clone(),
            loaded_child_objects: BTreeMap::new(),
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
            max_binary_format_version: self.protocol_config.move_binary_format_version(),
            loaded_child_objects: self.loaded_child_objects,
            no_extraneous_module_bytes: self.protocol_config.no_extraneous_module_bytes(),
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

        let protocol_version = self.protocol_config.version;
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

    pub fn smash_gas(&mut self, gas: &[ObjectRef]) -> Result<ObjectRef, ExecutionError> {
        if gas.len() > 1 {
            // sum the value of all gas coins
            let new_balance = gas
                .iter()
                .map(|obj_ref| {
                    let obj = self.objects().get(&obj_ref.0).unwrap();
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
                    Ok(move_obj.get_coin_value_unsafe())
                })
                .collect::<Result<Vec<u64>, ExecutionError>>()?
                .iter()
                .sum();
            // unwrap safe because we checked that this exists in `self.objects()` above
            let mut primary_gas_object = self.objects().get(&gas[0].0).unwrap().clone();
            // delete all gas objects except the primary_gas_object
            for (id, version, _digest) in &gas[1..] {
                debug_assert_ne!(*id, primary_gas_object.id());
                self.delete_object(id, *version, DeleteKind::Normal)
            }
            // unwrap is safe because we checked that the primary gas object was a coin object above.
            primary_gas_object
                .data
                .try_as_move_mut()
                .unwrap()
                .set_coin_value_unsafe(new_balance);
            self.write_object(primary_gas_object, WriteKind::Mutate);
        }
        Ok(gas[0])
    }

    pub fn delete_object(&mut self, id: &ObjectID, version: SequenceNumber, kind: DeleteKind) {
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
                panic!("Internal invariant violation in tx {digest}: Deleting immutable object {id}, version {version}, delete kind {kind}")
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
    pub fn reset(&mut self, gas: &[ObjectRef], gas_status: &mut SuiGasStatus) {
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

impl<S: ObjectStore> TemporaryStore<S> {
    /// returns lists of (objects whose owner we must authenticate, objects whose owner has already been authenticated)
    fn get_objects_to_authenticate(
        &self,
        sender: &SuiAddress,
        gas: &[ObjectRef],
        is_epoch_change: bool,
    ) -> SuiResult<(Vec<ObjectID>, HashSet<ObjectID>)> {
        let gas_objs: HashSet<&ObjectID> = gas.iter().map(|g| &g.0).collect();
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

        for (id, (_version, kind)) in &self.deleted {
            if authenticated_objs.contains(id) || gas_objs.contains(id) {
                continue;
            }
            match kind {
                DeleteKind::Normal | DeleteKind::Wrap => {
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
                DeleteKind::UnwrapThenDelete => {
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
        gas: &[ObjectRef],
        is_epoch_change: bool,
    ) -> SuiResult<()> {
        let (mut objects_to_authenticate, mut authenticated_objects) =
            self.get_objects_to_authenticate(sender, gas, is_epoch_change)?;

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

//==============================================================================
// Charge gas legacy - start
// This is the original gas charging code, all code between comment
// "Charge gas legacy - start/end" is exclusively for legacy gas
//==============================================================================
impl<S: ObjectStore> TemporaryStore<S> {
    /// 1. Compute tx storage gas costs and tx storage rebates, update storage_rebate field of mutated objects
    /// 2. Deduct computation gas costs and storage costs to `gas_object_id`, credit storage rebates to `gas_object_id`.
    /// gas_object_id can be None if this is a system transaction.
    // The happy path of this function follows (1) + (2) and is fairly simple. Most of the complexity is in the unhappy paths:
    // - if execution aborted before calling this function, we have to dump all writes + re-smash gas, then charge for storage
    // - if we run out of gas while charging for storage, we have to dump all writes + re-smash gas, then charge for storage again
    pub fn charge_gas_legacy<T>(
        &mut self,
        gas_object_id: ObjectID,
        gas_status: &mut SuiGasStatus,
        execution_result: &mut Result<T, ExecutionError>,
        gas: &[ObjectRef],
    ) {
        // at this point, we have done *all* charging for computation,
        // but have not yet set the storage rebate or storage gas units
        assert!(gas_status.storage_rebate() == 0);
        assert!(gas_status.storage_gas_units() == 0);

        // bucketize computation cost
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
        gas::deduct_gas_legacy(
            &mut gas_object,
            gas_used,
            cost_summary.sender_rebate(self.storage_rebate_rate),
        );
        trace!(gas_used, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");
        self.write_object(gas_object, WriteKind::Mutate);
        self.gas_charged = Some((gas_object_id, cost_summary));
    }

    /// Return the storage rebate and size of `id` at input
    fn get_input_storage_rebate_and_size(
        &self,
        id: &ObjectID,
        expected_version: SequenceNumber,
    ) -> Result<(u64, usize), ExecutionError> {
        if let Some(old_obj) = self.input_objects.get(id) {
            Ok((
                old_obj.storage_rebate,
                old_obj.object_size_for_gas_metering(),
            ))
        } else {
            // else, this is a dynamic field, not an input object
            if let Ok(Some(old_obj)) = self.store.get_object(id) {
                if old_obj.version() != expected_version {
                    return Err(ExecutionError::invariant_violation(
                        "Expected to find old object with version {expected_version}",
                    ));
                }
                Ok((
                    old_obj.storage_rebate,
                    old_obj.object_size_for_gas_metering(),
                ))
            } else {
                Err(ExecutionError::invariant_violation(
                    "Looking up storage rebate of mutated object should not fail",
                ))
            }
        }
    }

    /// Compute storage gas for each mutable input object (including the gas coin), and each created object.
    /// Compute storage refunds for each deleted object
    /// Will *not* charge any computation gas. Returns the total size in bytes of all deleted objects + all mutated objects,
    /// which the caller can use to charge computation gas
    /// gas_object_id can be None if this is a system transaction.
    fn charge_gas_for_storage_changes(
        &mut self,
        gas_status: &mut SuiGasStatus,
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
            let (old_storage_rebate, old_object_size) = match write_kind {
                WriteKind::Create | WriteKind::Unwrap => (0, 0),
                WriteKind::Mutate => {
                    if let Some(old_obj) = self.input_objects.get(object_id) {
                        (
                            old_obj.storage_rebate,
                            old_obj.object_size_for_gas_metering(),
                        )
                    } else {
                        // else, this is an input object, not a dynamic field
                        if let Ok(Some(old_obj)) = self.store.get_object(object_id) {
                            let expected_version = object.version();
                            if old_obj.version() != expected_version {
                                return Err(ExecutionError::invariant_violation(
                                    "Expected to find old object with version {expected_version}",
                                ));
                            }
                            (
                                old_obj.storage_rebate,
                                old_obj.object_size_for_gas_metering(),
                            )
                        } else {
                            return Err(ExecutionError::invariant_violation(
                                "Looking up storage rebate of mutated object should not fail",
                            ));
                        }
                    }
                }
            };
            let new_object_size = object.object_size_for_gas_metering();
            let new_storage_rebate =
                gas_status.charge_storage_mutation(new_object_size, old_storage_rebate)?;
            object.storage_rebate = new_storage_rebate;
            if !object.is_immutable() {
                objects_to_update.push((object.clone(), *write_kind));
            }
            total_bytes_written_deleted += old_object_size + new_object_size;
        }

        for (object_id, (version, kind)) in &self.deleted {
            match kind {
                DeleteKind::Wrap | DeleteKind::Normal => {
                    let (storage_rebate, object_size) =
                        self.get_input_storage_rebate_and_size(object_id, *version)?;
                    gas_status.charge_storage_mutation(0, storage_rebate)?;
                    total_bytes_written_deleted += object_size;
                }
                DeleteKind::UnwrapThenDelete => {
                    // an unwrapped object does not have a storage rebate, we will charge for storage changes via its wrapper object
                }
            }
        }

        // Write all objects at the end only if all previous gas charges succeeded.
        // This avoids polluting the temporary store state if this function failed.
        for (object, write_kind) in objects_to_update {
            self.write_object(object, write_kind);
        }
        Ok(total_bytes_written_deleted as u64)
    }
}
//==============================================================================
// Charge gas legacy - end
//==============================================================================

//==============================================================================
// Charge gas current - start
// This is the new/current/latest gas charging code, all code between comment
// "Charge gas current - start/end" is exclusively for latest gas
//==============================================================================
impl<S: ObjectStore> TemporaryStore<S> {
    /// 1. Compute tx storage gas costs and tx storage rebates, update storage_rebate field of mutated objects
    /// 2. Deduct computation gas costs and storage costs to `gas_object_id`, credit storage rebates to `gas_object_id`.
    // The happy path of this function follows (1) + (2) and is fairly simple. Most of the complexity is in the unhappy paths:
    // - if execution aborted before calling this function, we have to dump all writes + re-smash gas, then charge for storage
    // - if we run out of gas while charging for storage, we have to dump all writes + re-smash gas, then charge for storage again
    pub fn charge_gas<T>(
        &mut self,
        gas_object_id: Option<ObjectID>,
        gas_status: &mut SuiGasStatus,
        execution_result: &mut Result<T, ExecutionError>,
        gas: &[ObjectRef],
    ) -> GasCostSummary {
        // at this point, we have done *all* charging for computation,
        // but have not yet set the storage rebate or storage gas units
        if self.protocol_config.gas_model_version() < 2 {
            assert!(gas_status.storage_rebate() == 0);
            assert!(gas_status.storage_gas_units() == 0);
        } else {
            debug_assert!(gas_status.storage_rebate() == 0);
            debug_assert!(gas_status.storage_gas_units() == 0);
        }

        if gas_object_id.is_some() {
            // bucketize computation cost
            if let Err(err) = gas_status.bucketize_computation() {
                if execution_result.is_ok() {
                    *execution_result = Err(err);
                }
            }

            // On error we need to dump writes, deletes, etc before charging storage gas
            if execution_result.is_err() {
                self.reset(gas, gas_status);
            }
        }

        // compute and collect storage charges
        self.ensure_gas_and_input_mutated(gas_object_id);
        self.collect_storage_and_rebate(gas_status);
        // system transactions (None gas_object_id)  do not have gas and so do not charge
        // for storage, however they track storage values to check for conservation rules
        if let Some(gas_object_id) = gas_object_id {
            if self.protocol_config.gas_model_version() < 4 {
                self.handle_storage_and_rebate_v1(gas, gas_object_id, gas_status, execution_result)
            } else {
                self.handle_storage_and_rebate_v2(gas, gas_object_id, gas_status, execution_result)
            }

            let cost_summary = gas_status.summary();
            let gas_used = cost_summary.net_gas_usage();

            let mut gas_object = self.read_object(&gas_object_id).unwrap().clone();
            gas::deduct_gas(&mut gas_object, gas_used);
            trace!(gas_used, gas_obj_id =? gas_object.id(), gas_obj_ver =? gas_object.version(), "Updated gas object");

            self.write_object(gas_object, WriteKind::Mutate);
            self.gas_charged = Some((gas_object_id, cost_summary.clone()));
            cost_summary
        } else {
            GasCostSummary::default()
        }
    }

    fn handle_storage_and_rebate_v1<T>(
        &mut self,
        gas: &[ObjectRef],
        gas_object_id: ObjectID,
        gas_status: &mut SuiGasStatus,
        execution_result: &mut Result<T, ExecutionError>,
    ) {
        if let Err(err) = gas_status.charge_storage_and_rebate() {
            self.reset(gas, gas_status);
            gas_status.adjust_computation_on_out_of_gas();
            self.ensure_gas_and_input_mutated(Some(gas_object_id));
            self.collect_rebate(gas_status);
            if execution_result.is_ok() {
                *execution_result = Err(err);
            }
        }
    }

    fn handle_storage_and_rebate_v2<T>(
        &mut self,
        gas: &[ObjectRef],
        gas_object_id: ObjectID,
        gas_status: &mut SuiGasStatus,
        execution_result: &mut Result<T, ExecutionError>,
    ) {
        if let Err(err) = gas_status.charge_storage_and_rebate() {
            // we run out of gas charging storage, reset and try charging for storage again.
            // Input objects are touched and so they have a storage cost
            self.reset(gas, gas_status);
            self.ensure_gas_and_input_mutated(Some(gas_object_id));
            self.collect_storage_and_rebate(gas_status);
            if let Err(err) = gas_status.charge_storage_and_rebate() {
                // we run out of gas attempting to charge for the input objects exclusively,
                // deal with this edge case by not charging for storage
                self.reset(gas, gas_status);
                gas_status.adjust_computation_on_out_of_gas();
                self.ensure_gas_and_input_mutated(Some(gas_object_id));
                self.collect_rebate(gas_status);
                if execution_result.is_ok() {
                    *execution_result = Err(err);
                }
            } else if execution_result.is_ok() {
                *execution_result = Err(err);
            }
        }
    }

    /// Return the storage rebate of object `id`
    fn get_input_storage_rebate(&self, id: &ObjectID, expected_version: SequenceNumber) -> u64 {
        if let Some(old_obj) = self.input_objects.get(id) {
            old_obj.storage_rebate
        } else {
            // else, this is a dynamic field, not an input object
            if self.protocol_config.gas_model_version() < 2 {
                if let Ok(Some(old_obj)) = self.store.get_object(id) {
                    if old_obj.version() != expected_version {
                        // not a lot we can do safely and under this condition everything is broken
                        panic!(
                            "Expected to find old object with version {expected_version}, found {}",
                            old_obj.version(),
                        );
                    }
                    old_obj.storage_rebate
                } else {
                    // not a lot we can do safely and under this condition everything is broken
                    panic!("Looking up storage rebate of mutated object should not fail")
                }
            } else {
                // let's keep the if/else on gas version well separated
                #[allow(clippy::collapsible-else-if)]
                if let Ok(Some(old_obj)) = self.store.get_object_by_key(id, expected_version) {
                    old_obj.storage_rebate
                } else {
                    // not a lot we can do safely and under this condition everything is broken
                    panic!("Looking up storage rebate of mutated object should not fail")
                }
            }
        }
    }

    fn ensure_gas_and_input_mutated(&mut self, gas_object_id: Option<ObjectID>) {
        if let Some(gas_object_id) = gas_object_id {
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
    fn collect_storage_and_rebate(&mut self, gas_status: &mut SuiGasStatus) {
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
                        if self.protocol_config.gas_model_version() < 2 {
                            if let Ok(Some(old_obj)) = self.store.get_object(object_id) {
                                let expected_version = object.version();
                                if old_obj.version() != expected_version {
                                    // not a lot we can do safely and under this condition everything is broken
                                    panic!(
                                        "Expected to find old object with version {expected_version}, found {}",
                                        old_obj.version(),
                                    );
                                }
                                old_obj.storage_rebate
                            } else {
                                // not a lot we can do safely and under this condition everything is broken
                                panic!(
                                    "Looking up storage rebate of mutated object should not fail"
                                );
                            }
                        } else {
                            let expected_version = object.version();
                            if let Ok(Some(old_obj)) =
                                self.store.get_object_by_key(object_id, expected_version)
                            {
                                old_obj.storage_rebate
                            } else {
                                // not a lot we can do safely and under this condition everything is broken
                                panic!(
                                    "Looking up storage rebate of mutated object should not fail"
                                );
                            }
                        }
                    }
                }
            };
            // new object size
            let new_object_size = object.object_size_for_gas_metering();
            // track changes and compute the new object `storage_rebate`
            let new_storage_rebate =
                gas_status.track_storage_mutation(new_object_size, old_storage_rebate);
            object.storage_rebate = new_storage_rebate;
            if !object.is_immutable() {
                objects_to_update.push((object.clone(), *write_kind));
            }
        }

        self.collect_rebate(gas_status);

        // Write all objects at the end only if all previous gas charges succeeded.
        // This avoids polluting the temporary store state if this function failed.
        for (object, write_kind) in objects_to_update {
            self.write_object(object, write_kind);
        }
    }

    fn collect_rebate(&self, gas_status: &mut SuiGasStatus) {
        for (object_id, (version, kind)) in &self.deleted {
            match kind {
                DeleteKind::Wrap | DeleteKind::Normal => {
                    // get and track the deleted object `storage_rebate`
                    let storage_rebate = self.get_input_storage_rebate(object_id, *version);
                    gas_status.track_storage_mutation(0, storage_rebate);
                }
                DeleteKind::UnwrapThenDelete => {
                    // an unwrapped object does not have a storage rebate, we will charge for storage changes via its wrapper object
                }
            }
        }
    }
}
//==============================================================================
// Charge gas current - end
//==============================================================================

impl<S: ObjectStore> TemporaryStore<S> {
    pub fn advance_epoch_safe_mode(
        &mut self,
        params: &AdvanceEpochParams,
        protocol_config: &ProtocolConfig,
    ) {
        let wrapper = get_sui_system_state_wrapper(&self.store)
            .expect("System state wrapper object must exist");
        let new_object = wrapper.advance_epoch_safe_mode(params, &self.store, protocol_config);
        self.write_object(new_object, WriteKind::Mutate);
    }
}

impl<S: GetModule + ObjectStore + BackingPackageStore> TemporaryStore<S> {
    fn get_input_sui(
        &self,
        id: &ObjectID,
        expected_version: SequenceNumber,
        do_expensive_checks: bool,
    ) -> Result<(u64, u64), ExecutionError> {
        if let Some(obj) = self.input_objects.get(id) {
            // the assumption here is that if it is in the input objects must be the right one
            if obj.version() != expected_version {
                return Err(ExecutionError::invariant_violation(format!("Version mismatching when resolving input object to check conservation--expected {}, got {}", expected_version, obj.version())));
            }
            let input_sui = if do_expensive_checks {
                obj.get_total_sui(&self).map_err(|_e| {
                    ExecutionError::invariant_violation(
                        "Failed looking up output SUI in SUI conservation checking",
                    )
                })?
            } else {
                0
            };
            Ok((input_sui, obj.storage_rebate))
        } else {
            // not in input objects, must be a dynamic field
            if self.protocol_config.gas_model_version() < 2 {
                let obj = self
                    .store
                    .get_object(id)
                    .map_err(|_e| {
                        ExecutionError::invariant_violation(
                            "Failed looking up input object in SUI conservation checking",
                        )
                    })?
                    .ok_or_else(|| {
                        ExecutionError::invariant_violation(
                            "Failed looking up input object in SUI conservation checking",
                        )
                    })?;
                if obj.version() != expected_version {
                    return Err(ExecutionError::invariant_violation(format!("Version mismatching when resolving dynamic field to check conservation--expected {}, got {}", expected_version, obj.version())));
                }
                let input_sui = if do_expensive_checks {
                    obj.get_total_sui(&self).map_err(|_e| {
                        ExecutionError::invariant_violation(
                            "Failed looking up output SUI in SUI conservation checking",
                        )
                    })?
                } else {
                    0
                };
                Ok((input_sui, obj.storage_rebate))
            } else {
                let obj = self
                    .store
                    .get_object_by_key(id, expected_version)
                    .map_err(|_e| {
                        ExecutionError::invariant_violation(
                            "Failed looking up input object in SUI conservation checking",
                        )
                    })?
                    .ok_or_else(|| {
                        ExecutionError::invariant_violation(
                            "Failed looking up input object in SUI conservation checking",
                        )
                    })?;
                let input_sui = if do_expensive_checks {
                    obj.get_total_sui(&self).map_err(|_e| {
                        ExecutionError::invariant_violation(
                            "Failed looking up output SUI in SUI conservation checking",
                        )
                    })?
                } else {
                    0
                };
                Ok((input_sui, obj.storage_rebate))
            }
        }
    }

    /// Check that this transaction neither creates nor destroys SUI. This should hold for all txes except
    /// the epoch change tx, which mints staking rewards equal to the gas fees burned in the previous epoch.
    /// Specifically, this checks two key invariants about storage fees and storage rebate:
    /// 1. all SUI in storage rebate fields of input objects should flow either to the transaction storage rebate, or the transaction non-refundable storage rebate
    /// 2. all SUI charged for storage should flow into the storage rebate field of some output object
    /// If `do_expensive_checks` is true, this will also check a third invariant:
    /// 3. all SUI in input objects (including coins etc in the Move part of an object) should flow either to an output object, or be burned as part of computation fees or non-refundable storage rebate
    /// This function is intended to be called *after* we have charged for gas + applied the storage rebate to the gas object,
    /// but *before* we have updated object versions.
    /// if `do_expensive_checks` is false, this function will only check conservation of object storage rea
    /// `epoch_fees` and `epoch_rebates` are only set for advance epoch transactions.
    /// The advance epoch transaction would mint `epoch_fees` amount of SUI, and burn
    /// `epoch_rebates` amount of SUI. We need these information for conservation check.
    pub fn check_sui_conserved(
        &self,
        advance_epoch_gas_summary: Option<(u64, u64)>,
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
                    let (input_sui, input_storage_rebate) =
                        self.get_input_sui(id, input_version, do_expensive_checks)?;
                    total_input_sui += input_sui;
                    if do_expensive_checks {
                        total_output_sui += output_obj.get_total_sui(&self).map_err(|_e| {
                            ExecutionError::invariant_violation(
                                "Failed looking up output SUI in SUI conservation checking",
                            )
                        })?;
                    }
                    total_input_rebate += input_storage_rebate;
                    total_output_rebate += output_obj.storage_rebate;
                }
                WriteKind::Create => {
                    // created objects did not exist at input, and thus contribute 0 to input SUI
                    if do_expensive_checks {
                        total_output_sui += output_obj.get_total_sui(&self).map_err(|_e| {
                            ExecutionError::invariant_violation(
                                "Failed looking up output SUI in SUI conservation checking",
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
                        total_output_sui += output_obj.get_total_sui(&self).map_err(|_e| {
                            ExecutionError::invariant_violation(
                                "Failed looking up output SUI in SUI conservation checking",
                            )
                        })?;
                    }
                    total_output_rebate += output_obj.storage_rebate;
                }
            }
        }
        for (id, (input_version, kind)) in &self.deleted {
            match kind {
                DeleteKind::Normal => {
                    let (input_sui, input_storage_rebate) =
                        self.get_input_sui(id, *input_version, do_expensive_checks)?;
                    total_input_sui += input_sui;
                    total_input_rebate += input_storage_rebate;
                }
                DeleteKind::Wrap => {
                    // wrapped object was a tx input or dynamic field--need to account for it in input SUI
                    // note: if an object is created by the tx, then wrapped, it will not appear here
                    let (input_sui, input_storage_rebate) =
                        self.get_input_sui(id, *input_version, do_expensive_checks)?;
                    total_input_sui += input_sui;
                    total_input_rebate += input_storage_rebate;
                    // else, the wrapped object was either:
                    // 1. freshly created, which means it has 0 contribution to input SUI
                    // 2. unwrapped from another object A, which means its contribution to input SUI will be captured by looking at A
                }
                DeleteKind::UnwrapThenDelete => {
                    // an unwrapped option was wrapped in input object or dynamic field A, which means its contribution to input SUI will
                    // be captured by looking at A
                }
            }
        }

        // we do account for the "storage rebate inflow" (portion of the storage rebate which flows back into the storage fund).
        let gas_summary = &self
            .gas_charged
            .as_ref()
            .map(|(_, summary)| summary.clone())
            .unwrap_or_default();

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

    fn save_loaded_child_objects(
        &mut self,
        loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    ) {
        TemporaryStore::save_loaded_child_objects(self, loaded_child_objects)
    }
}

impl<S: BackingPackageStore> BackingPackageStore for TemporaryStore<S> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        self.store.get_package_object(package_id)
    }
}

/// TODO: Proper implementation of re-linking (currently the default implementation does nothing).
impl<S> LinkageResolver for TemporaryStore<S> {
    type Error = SuiError;
}

impl<S: BackingPackageStore> ModuleResolver for TemporaryStore<S> {
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
