// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gas_charger::GasCharger;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::StructTag;
use move_core_types::resolver::ResourceResolver;
use parking_lot::RwLock;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::VersionDigest;
use sui_types::committee::EpochId;
use sui_types::digests::ObjectDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::execution::{
    DynamicallyLoadedObjectMetadata, ExecutionResults, ExecutionResultsV2, SharedInput,
};
use sui_types::execution_config_utils::to_binary_config;
use sui_types::execution_status::ExecutionStatus;
use sui_types::inner_temporary_store::InnerTemporaryStore;
use sui_types::layout_resolver::LayoutResolver;
use sui_types::storage::{BackingStore, DenyListResult, PackageObject};
use sui_types::sui_system_state::{get_sui_system_state_wrapper, AdvanceEpochParams};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest},
    effects::EffectsObjectChange,
    error::{ExecutionError, SuiError, SuiResult},
    fp_bail,
    gas::GasCostSummary,
    object::Owner,
    object::{Data, Object},
    storage::{BackingPackageStore, ChildObjectResolver, ParentSync, Storage},
    transaction::InputObjects,
};
use sui_types::{is_system_package, SUI_SYSTEM_STATE_OBJECT_ID};

pub struct TemporaryStore<'backing> {
    // The backing store for retrieving Move packages onchain.
    // When executing a Move call, the dependent packages are not going to be
    // in the input objects. They will be fetched from the backing store.
    // Also used for fetching the backing parent_sync to get the last known version for wrapped
    // objects
    store: &'backing dyn BackingStore,
    tx_digest: TransactionDigest,
    input_objects: BTreeMap<ObjectID, Object>,
    deleted_consensus_objects: BTreeMap<ObjectID, SequenceNumber>,
    /// The version to assign to all objects written by the transaction using this store.
    lamport_timestamp: SequenceNumber,
    mutable_input_refs: BTreeMap<ObjectID, (VersionDigest, Owner)>, // Inputs that are mutable
    execution_results: ExecutionResultsV2,
    /// Objects that were loaded during execution (dynamic fields + received objects).
    loaded_runtime_objects: BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
    /// A map from wrapped object to its container. Used during expensive invariant checks.
    wrapped_object_containers: BTreeMap<ObjectID, ObjectID>,
    protocol_config: &'backing ProtocolConfig,

    /// Every package that was loaded from DB store during execution.
    /// These packages were not previously loaded into the temporary store.
    runtime_packages_loaded_from_db: RwLock<BTreeMap<ObjectID, PackageObject>>,

    /// The set of objects that we may receive during execution. Not guaranteed to receive all, or
    /// any of the objects referenced in this set.
    receiving_objects: Vec<ObjectRef>,
}

impl<'backing> TemporaryStore<'backing> {
    /// Creates a new store associated with an authority store, and populates it with
    /// initial objects.
    pub fn new(
        store: &'backing dyn BackingStore,
        input_objects: InputObjects,
        receiving_objects: Vec<ObjectRef>,
        tx_digest: TransactionDigest,
        protocol_config: &'backing ProtocolConfig,
    ) -> Self {
        let mutable_input_refs = input_objects.mutable_inputs();
        let lamport_timestamp = input_objects.lamport_timestamp(&receiving_objects);
        let deleted_consensus_objects = input_objects.deleted_consensus_objects();
        let objects = input_objects.into_object_map();
        #[cfg(debug_assertions)]
        {
            // Ensure that input objects and receiving objects must not overlap.
            assert!(objects
                .keys()
                .collect::<HashSet<_>>()
                .intersection(
                    &receiving_objects
                        .iter()
                        .map(|oref| &oref.0)
                        .collect::<HashSet<_>>()
                )
                .next()
                .is_none());
        }
        Self {
            store,
            tx_digest,
            input_objects: objects,
            deleted_consensus_objects,
            lamport_timestamp,
            mutable_input_refs,
            execution_results: ExecutionResultsV2::default(),
            protocol_config,
            loaded_runtime_objects: BTreeMap::new(),
            wrapped_object_containers: BTreeMap::new(),
            runtime_packages_loaded_from_db: RwLock::new(BTreeMap::new()),
            receiving_objects,
        }
    }

    // Helpers to access private fields
    pub fn objects(&self) -> &BTreeMap<ObjectID, Object> {
        &self.input_objects
    }

    pub fn update_object_version_and_prev_tx(&mut self) {
        self.execution_results.update_version_and_previous_tx(
            self.lamport_timestamp,
            self.tx_digest,
            &self.input_objects,
            false,
        );

        #[cfg(debug_assertions)]
        {
            self.check_invariants();
        }
    }

    /// Break up the structure and return its internal stores (objects, active_inputs, written, deleted)
    pub fn into_inner(self) -> InnerTemporaryStore {
        let results = self.execution_results;
        InnerTemporaryStore {
            input_objects: self.input_objects,
            mutable_inputs: self.mutable_input_refs,
            deleted_consensus_objects: self.deleted_consensus_objects,
            written: results.written_objects,
            events: TransactionEvents {
                data: results.user_events,
            },
            loaded_runtime_objects: self.loaded_runtime_objects,
            runtime_packages_loaded_from_db: self.runtime_packages_loaded_from_db.into_inner(),
            lamport_version: self.lamport_timestamp,
            binary_config: to_binary_config(self.protocol_config),
        }
    }

    /// For every object from active_inputs (i.e. all mutable objects), if they are not
    /// mutated during the transaction execution, force mutating them by incrementing the
    /// sequence number. This is required to achieve safety.
    pub(crate) fn ensure_active_inputs_mutated(&mut self) {
        let mut to_be_updated = vec![];
        for id in self.mutable_input_refs.keys() {
            if !self.execution_results.modified_objects.contains(id) {
                // We cannot update here but have to push to `to_be_updated` and update later
                // because the for loop is holding a reference to `self`, and calling
                // `self.mutate_input_object` requires a mutable reference to `self`.
                to_be_updated.push(self.input_objects[id].clone());
            }
        }
        for object in to_be_updated {
            // The object must be mutated as it was present in the input objects
            self.mutate_input_object(object.clone());
        }
    }

    fn get_object_changes(&self) -> BTreeMap<ObjectID, EffectsObjectChange> {
        let results = &self.execution_results;
        let all_ids = results
            .created_object_ids
            .iter()
            .chain(&results.deleted_object_ids)
            .chain(&results.modified_objects)
            .chain(results.written_objects.keys())
            .collect::<BTreeSet<_>>();
        all_ids
            .into_iter()
            .map(|id| {
                (
                    *id,
                    EffectsObjectChange::new(
                        self.get_object_modified_at(id)
                            .map(|metadata| ((metadata.version, metadata.digest), metadata.owner)),
                        results.written_objects.get(id),
                        results.created_object_ids.contains(id),
                        results.deleted_object_ids.contains(id),
                    ),
                )
            })
            .collect()
    }

    pub fn into_effects(
        mut self,
        shared_object_refs: Vec<SharedInput>,
        transaction_digest: &TransactionDigest,
        mut transaction_dependencies: BTreeSet<TransactionDigest>,
        gas_cost_summary: GasCostSummary,
        status: ExecutionStatus,
        gas_charger: &mut GasCharger,
        epoch: EpochId,
    ) -> (InnerTemporaryStore, TransactionEffects) {
        self.update_object_version_and_prev_tx();

        // Regardless of execution status (including aborts), we insert the previous transaction
        // for any successfully received objects during the transaction.
        for (id, expected_version, expected_digest) in &self.receiving_objects {
            // If the receiving object is in the loaded runtime objects, then that means that it
            // was actually successfully loaded (so existed, and there was authenticated mutable
            // access to it). So we insert the previous transaction as a dependency.
            if let Some(obj_meta) = self.loaded_runtime_objects.get(id) {
                // Check that the expected version, digest, and owner match the loaded version,
                // digest, and owner. If they don't then don't register a dependency.
                // This is because this could be "spoofed" by loading a dynamic object field.
                let loaded_via_receive = obj_meta.version == *expected_version
                    && obj_meta.digest == *expected_digest
                    && obj_meta.owner.is_address_owned();
                if loaded_via_receive {
                    transaction_dependencies.insert(obj_meta.previous_transaction);
                }
            }
        }

        if self.protocol_config.enable_effects_v2() {
            self.into_effects_v2(
                shared_object_refs,
                transaction_digest,
                transaction_dependencies,
                gas_cost_summary,
                status,
                gas_charger,
                epoch,
            )
        } else {
            let shared_object_refs = shared_object_refs
                .into_iter()
                .map(|shared_input| match shared_input {
                    SharedInput::Existing(oref) => oref,
                    SharedInput::Deleted(_) => {
                        unreachable!("Shared object deletion not supported in effects v1")
                    }
                    SharedInput::Cancelled(_) => {
                        unreachable!("Per object congestion control not supported in effects v1.")
                    }
                })
                .collect();
            self.into_effects_v1(
                shared_object_refs,
                transaction_digest,
                transaction_dependencies,
                gas_cost_summary,
                status,
                gas_charger,
                epoch,
            )
        }
    }

    fn into_effects_v1(
        self,
        shared_object_refs: Vec<ObjectRef>,
        transaction_digest: &TransactionDigest,
        transaction_dependencies: BTreeSet<TransactionDigest>,
        gas_cost_summary: GasCostSummary,
        status: ExecutionStatus,
        gas_charger: &mut GasCharger,
        epoch: EpochId,
    ) -> (InnerTemporaryStore, TransactionEffects) {
        let updated_gas_object_info = if let Some(coin_id) = gas_charger.gas_coin() {
            let object = &self.execution_results.written_objects[&coin_id];
            (object.compute_object_reference(), object.owner.clone())
        } else {
            (
                (ObjectID::ZERO, SequenceNumber::default(), ObjectDigest::MIN),
                Owner::AddressOwner(SuiAddress::default()),
            )
        };
        let lampot_version = self.lamport_timestamp;

        let mut created = vec![];
        let mut mutated = vec![];
        let mut unwrapped = vec![];
        let mut deleted = vec![];
        let mut unwrapped_then_deleted = vec![];
        let mut wrapped = vec![];
        // It is important that we constructs `modified_at_versions` and `deleted_at_versions`
        // separately, and merge them latter to achieve the exact same order as in v1.
        let mut modified_at_versions = vec![];
        let mut deleted_at_versions = vec![];
        self.execution_results
            .written_objects
            .iter()
            .for_each(|(id, object)| {
                let object_ref = object.compute_object_reference();
                let owner = object.owner.clone();
                if let Some(old_object_meta) = self.get_object_modified_at(id) {
                    modified_at_versions.push((*id, old_object_meta.version));
                    mutated.push((object_ref, owner));
                } else if self.execution_results.created_object_ids.contains(id) {
                    created.push((object_ref, owner));
                } else {
                    unwrapped.push((object_ref, owner));
                }
            });
        self.execution_results
            .modified_objects
            .iter()
            .filter(|id| !self.execution_results.written_objects.contains_key(id))
            .for_each(|id| {
                let old_object_meta = self.get_object_modified_at(id).unwrap();
                deleted_at_versions.push((*id, old_object_meta.version));
                if self.execution_results.deleted_object_ids.contains(id) {
                    deleted.push((*id, lampot_version, ObjectDigest::OBJECT_DIGEST_DELETED));
                } else {
                    wrapped.push((*id, lampot_version, ObjectDigest::OBJECT_DIGEST_WRAPPED));
                }
            });
        self.execution_results
            .deleted_object_ids
            .iter()
            .filter(|id| !self.execution_results.modified_objects.contains(id))
            .for_each(|id| {
                unwrapped_then_deleted.push((
                    *id,
                    lampot_version,
                    ObjectDigest::OBJECT_DIGEST_DELETED,
                ));
            });
        modified_at_versions.extend(deleted_at_versions);

        let inner = self.into_inner();
        let effects = TransactionEffects::new_from_execution_v1(
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
            transaction_dependencies.into_iter().collect(),
        );
        (inner, effects)
    }

    fn into_effects_v2(
        self,
        shared_object_refs: Vec<SharedInput>,
        transaction_digest: &TransactionDigest,
        transaction_dependencies: BTreeSet<TransactionDigest>,
        gas_cost_summary: GasCostSummary,
        status: ExecutionStatus,
        gas_charger: &mut GasCharger,
        epoch: EpochId,
    ) -> (InnerTemporaryStore, TransactionEffects) {
        // In the case of special transactions that don't require a gas object,
        // we don't really care about the effects to gas, just use the input for it.
        // Gas coins are guaranteed to be at least size 1 and if more than 1
        // the first coin is where all the others are merged.
        let gas_coin = gas_charger.gas_coin();

        let object_changes = self.get_object_changes();

        let lamport_version = self.lamport_timestamp;
        let inner = self.into_inner();

        let effects = TransactionEffects::new_from_execution_v2(
            status,
            epoch,
            gas_cost_summary,
            // TODO: Provide the list of read-only shared objects directly.
            shared_object_refs,
            BTreeSet::new(),
            *transaction_digest,
            lamport_version,
            object_changes,
            gas_coin,
            if inner.events.data.is_empty() {
                None
            } else {
                Some(inner.events.digest())
            },
            transaction_dependencies.into_iter().collect(),
        );

        (inner, effects)
    }

    /// An internal check of the invariants (will only fire in debug)
    #[cfg(debug_assertions)]
    fn check_invariants(&self) {
        // Check not both deleted and written
        debug_assert!(
            {
                self.execution_results
                    .written_objects
                    .keys()
                    .all(|id| !self.execution_results.deleted_object_ids.contains(id))
            },
            "Object both written and deleted."
        );

        // Check all mutable inputs are modified
        debug_assert!(
            {
                self.mutable_input_refs
                    .keys()
                    .all(|id| self.execution_results.modified_objects.contains(id))
            },
            "Mutable input not modified."
        );

        debug_assert!(
            {
                self.execution_results
                    .written_objects
                    .values()
                    .all(|obj| obj.previous_transaction == self.tx_digest)
            },
            "Object previous transaction not properly set",
        );
    }

    /// Mutate a mutable input object. This is used to mutate input objects outside of PT execution.
    pub fn mutate_input_object(&mut self, object: Object) {
        let id = object.id();
        debug_assert!(self.input_objects.contains_key(&id));
        debug_assert!(!object.is_immutable());
        self.execution_results.modified_objects.insert(id);
        self.execution_results.written_objects.insert(id, object);
    }

    /// Mutate a child object outside of PT. This should be used extremely rarely.
    /// Currently it's only used by advance_epoch_safe_mode because it's all native
    /// without PT. This should almost never be used otherwise.
    pub fn mutate_child_object(&mut self, old_object: Object, new_object: Object) {
        let id = new_object.id();
        let old_ref = old_object.compute_object_reference();
        debug_assert_eq!(old_ref.0, id);
        self.loaded_runtime_objects.insert(
            id,
            DynamicallyLoadedObjectMetadata {
                version: old_ref.1,
                digest: old_ref.2,
                owner: old_object.owner.clone(),
                storage_rebate: old_object.storage_rebate,
                previous_transaction: old_object.previous_transaction,
            },
        );
        self.execution_results.modified_objects.insert(id);
        self.execution_results
            .written_objects
            .insert(id, new_object);
    }

    /// Upgrade system package during epoch change. This requires special treatment
    /// since the system package to be upgraded is not in the input objects.
    /// We could probably fix above to make it less special.
    pub fn upgrade_system_package(&mut self, package: Object) {
        let id = package.id();
        assert!(package.is_package() && is_system_package(id));
        self.execution_results.modified_objects.insert(id);
        self.execution_results.written_objects.insert(id, package);
    }

    /// Crate a new objcet. This is used to create objects outside of PT execution.
    pub fn create_object(&mut self, object: Object) {
        // Created mutable objects' versions are set to the store's lamport timestamp when it is
        // committed to effects. Creating an object at a non-zero version risks violating the
        // lamport timestamp invariant (that a transaction's lamport timestamp is strictly greater
        // than all versions witnessed by the transaction).
        debug_assert!(
            object.is_immutable() || object.version() == SequenceNumber::MIN,
            "Created mutable objects should not have a version set",
        );
        let id = object.id();
        self.execution_results.created_object_ids.insert(id);
        self.execution_results.written_objects.insert(id, object);
    }

    /// Delete a mutable input object. This is used to delete input objects outside of PT execution.
    pub fn delete_input_object(&mut self, id: &ObjectID) {
        // there should be no deletion after write
        debug_assert!(!self.execution_results.written_objects.contains_key(id));
        debug_assert!(self.input_objects.contains_key(id));
        self.execution_results.modified_objects.insert(*id);
        self.execution_results.deleted_object_ids.insert(*id);
    }

    pub fn drop_writes(&mut self) {
        self.execution_results.drop_writes();
    }

    pub fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        // there should be no read after delete
        debug_assert!(!self.execution_results.deleted_object_ids.contains(id));
        self.execution_results
            .written_objects
            .get(id)
            .or_else(|| self.input_objects.get(id))
    }

    pub fn save_loaded_runtime_objects(
        &mut self,
        loaded_runtime_objects: BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
    ) {
        #[cfg(debug_assertions)]
        {
            for (id, v1) in &loaded_runtime_objects {
                if let Some(v2) = self.loaded_runtime_objects.get(id) {
                    assert_eq!(v1, v2);
                }
            }
            for (id, v1) in &self.loaded_runtime_objects {
                if let Some(v2) = loaded_runtime_objects.get(id) {
                    assert_eq!(v1, v2);
                }
            }
        }
        // Merge the two maps because we may be calling the execution engine more than once
        // (e.g. in advance epoch transaction, where we may be publishing a new system package).
        self.loaded_runtime_objects.extend(loaded_runtime_objects);
    }

    pub fn save_wrapped_object_containers(
        &mut self,
        wrapped_object_containers: BTreeMap<ObjectID, ObjectID>,
    ) {
        #[cfg(debug_assertions)]
        {
            for (id, container1) in &wrapped_object_containers {
                if let Some(container2) = self.wrapped_object_containers.get(id) {
                    assert_eq!(container1, container2);
                }
            }
            for (id, container1) in &self.wrapped_object_containers {
                if let Some(container2) = wrapped_object_containers.get(id) {
                    assert_eq!(container1, container2);
                }
            }
        }
        // Merge the two maps because we may be calling the execution engine more than once
        // (e.g. in advance epoch transaction, where we may be publishing a new system package).
        self.wrapped_object_containers
            .extend(wrapped_object_containers);
    }

    pub fn estimate_effects_size_upperbound(&self) -> usize {
        if self.protocol_config.enable_effects_v2() {
            TransactionEffects::estimate_effects_size_upperbound_v2(
                self.execution_results.written_objects.len(),
                self.execution_results.modified_objects.len(),
                self.input_objects.len(),
            )
        } else {
            let num_deletes = self.execution_results.deleted_object_ids.len()
                + self
                    .execution_results
                    .modified_objects
                    .iter()
                    .filter(|id| {
                        // Filter for wrapped objects.
                        !self.execution_results.written_objects.contains_key(id)
                            && !self.execution_results.deleted_object_ids.contains(id)
                    })
                    .count();
            // In the worst case, the number of deps is equal to the number of input objects
            TransactionEffects::estimate_effects_size_upperbound_v1(
                self.execution_results.written_objects.len(),
                self.mutable_input_refs.len(),
                num_deletes,
                self.input_objects.len(),
            )
        }
    }

    pub fn written_objects_size(&self) -> usize {
        self.execution_results
            .written_objects
            .values()
            .fold(0, |sum, obj| sum + obj.object_size_for_gas_metering())
    }

    /// If there are unmetered storage rebate (due to system transaction), we put them into
    /// the storage rebate of 0x5 object.
    /// TODO: This will not work for potential future new system transactions if 0x5 is not in the input.
    /// We should fix this.
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
            .expect("0x5 object must be mutated in system tx with unmetered storage rebate")
            .clone();
        // In unmetered execution, storage_rebate field of mutated object must be 0.
        // If not, we would be dropping SUI on the floor by overriding it.
        assert_eq!(system_state_wrapper.storage_rebate, 0);
        system_state_wrapper.storage_rebate = unmetered_storage_rebate;
        self.mutate_input_object(system_state_wrapper);
    }

    /// Given an object ID, if it's not modified, returns None.
    /// Otherwise returns its metadata, including version, digest, owner and storage rebate.
    /// A modified object must be either a mutable input, or a loaded child object.
    /// The only exception is when we upgrade system packages, in which case the upgraded
    /// system packages are not part of input, but are modified.
    fn get_object_modified_at(
        &self,
        object_id: &ObjectID,
    ) -> Option<DynamicallyLoadedObjectMetadata> {
        if self.execution_results.modified_objects.contains(object_id) {
            Some(
                self.mutable_input_refs
                    .get(object_id)
                    .map(
                        |((version, digest), owner)| DynamicallyLoadedObjectMetadata {
                            version: *version,
                            digest: *digest,
                            owner: owner.clone(),
                            // It's guaranteed that a mutable input object is an input object.
                            storage_rebate: self.input_objects[object_id].storage_rebate,
                            previous_transaction: self.input_objects[object_id]
                                .previous_transaction,
                        },
                    )
                    .or_else(|| self.loaded_runtime_objects.get(object_id).cloned())
                    .unwrap_or_else(|| {
                        debug_assert!(is_system_package(*object_id));
                        let package_obj =
                            self.store.get_package_object(object_id).unwrap().unwrap();
                        let obj = package_obj.object();
                        DynamicallyLoadedObjectMetadata {
                            version: obj.version(),
                            digest: obj.digest(),
                            owner: obj.owner.clone(),
                            storage_rebate: obj.storage_rebate,
                            previous_transaction: obj.previous_transaction,
                        }
                    }),
            )
        } else {
            None
        }
    }
}

impl<'backing> TemporaryStore<'backing> {
    // check that every object read is owned directly or indirectly by sender, sponsor,
    // or a shared object input
    pub fn check_ownership_invariants(
        &self,
        sender: &SuiAddress,
        gas_charger: &mut GasCharger,
        mutable_inputs: &HashSet<ObjectID>,
        is_epoch_change: bool,
    ) -> SuiResult<()> {
        let gas_objs: HashSet<&ObjectID> = gas_charger.gas_coins().iter().map(|g| &g.0).collect();
        // mark input objects as authenticated
        let mut authenticated_for_mutation: HashSet<_> = self
            .input_objects
            .iter()
            .filter_map(|(id, obj)| {
                if gas_objs.contains(id) {
                    // gas could be owned by either the sender (common case) or sponsor
                    // (if this is a sponsored tx, which we do not know inside this function).
                    // Either way, no object ownership chain should be rooted in a gas object
                    // thus, consider object authenticated, but don't add it to authenticated_objs
                    return None;
                }
                match &obj.owner {
                    Owner::AddressOwner(a) => {
                        assert!(sender == a, "Input object not owned by sender");
                        Some(id)
                    }
                    Owner::Shared { .. } => Some(id),
                    Owner::Immutable => {
                        // object is authenticated, but it cannot own other objects,
                        // so we should not add it to `authenticated_objs`
                        // However, we would definitely want to add immutable objects
                        // to the set of authenticated roots if we were doing runtime
                        // checks inside the VM instead of after-the-fact in the temporary
                        // store. Here, we choose not to add them because this will catch a
                        // bug where we mutate or delete an object that belongs to an immutable
                        // object (though it will show up somewhat opaquely as an authentication
                        // failure), whereas adding the immutable object to the roots will prevent
                        // us from catching this.
                        None
                    }
                    Owner::ObjectOwner(_parent) => {
                        unreachable!("Input objects must be address owned, shared, or immutable")
                    }
                    Owner::ConsensusV2 { .. } => {
                        unimplemented!("ConsensusV2 does not exist for this execution version")
                    }
                }
            })
            .filter(|id| {
                // remove any non-mutable inputs. This will remove deleted or readonly shared
                // objects
                mutable_inputs.contains(id)
            })
            .copied()
            .collect();

        // check all modified objects are authenticated (excluding gas objects)
        let mut objects_to_authenticate = self
            .execution_results
            .modified_objects
            .iter()
            .filter(|id| !gas_objs.contains(id))
            .copied()
            .collect::<Vec<_>>();
        // Map from an ObjectID to the ObjectID that covers it.
        while let Some(to_authenticate) = objects_to_authenticate.pop() {
            if authenticated_for_mutation.contains(&to_authenticate) {
                // object has been authenticated
                continue;
            }
            let wrapped_parent = self.wrapped_object_containers.get(&to_authenticate);
            let parent = if let Some(container_id) = wrapped_parent {
                // If the object is wrapped, then the container must be authenticated.
                // For example, the ID is for a wrapped table or bag.
                *container_id
            } else {
                let Some(old_obj) = self.store.get_object(&to_authenticate) else {
                    panic!(
                        "
                        Failed to load object {to_authenticate:?}. \n\
                        If it cannot be loaded, \
                        we would expect it to be in the wrapped object map: {:?}",
                        &self.wrapped_object_containers
                    )
                };
                match &old_obj.owner {
                    Owner::ObjectOwner(parent) => ObjectID::from(*parent),
                    Owner::AddressOwner(parent) => {
                        // For Receiving<_> objects, the address owner is actually an object.
                        // If it was actually an address, we should have caught it as an input and
                        // it would already have been in authenticated_for_mutation
                        ObjectID::from(*parent)
                    }
                    owner @ Owner::Shared { .. } => panic!(
                        "Unauthenticated root at {to_authenticate:?} with owner {owner:?}\n\
                        Potentially covering objects in: {authenticated_for_mutation:#?}",
                    ),
                    Owner::Immutable => {
                        assert!(
                            is_epoch_change,
                            "Immutable objects cannot be written, except for \
                            Sui Framework/Move stdlib upgrades at epoch change boundaries"
                        );
                        // Note: this assumes that the only immutable objects an epoch change
                        // tx can update are system packages,
                        // but in principle we could allow others.
                        assert!(
                            is_system_package(to_authenticate),
                            "Only system packages can be upgraded"
                        );
                        continue;
                    }
                    Owner::ConsensusV2 { .. } => {
                        unimplemented!("ConsensusV2 does not exist for this execution version")
                    }
                }
            };
            // we now assume the object is authenticated and must check the parent
            authenticated_for_mutation.insert(to_authenticate);
            objects_to_authenticate.push(parent);
        }
        Ok(())
    }
}

impl<'backing> TemporaryStore<'backing> {
    /// Track storage gas for each mutable input object (including the gas coin)
    /// and each created object. Compute storage refunds for each deleted object.
    /// Will *not* charge anything, gas status keeps track of storage cost and rebate.
    /// All objects will be updated with their new (current) storage rebate/cost.
    /// `SuiGasStatus` `storage_rebate` and `storage_gas_units` track the transaction
    /// overall storage rebate and cost.
    pub(crate) fn collect_storage_and_rebate(&mut self, gas_charger: &mut GasCharger) {
        // Use two loops because we cannot mut iterate written while calling get_object_modified_at.
        let old_storage_rebates: Vec<_> = self
            .execution_results
            .written_objects
            .keys()
            .map(|object_id| {
                self.get_object_modified_at(object_id)
                    .map(|metadata| metadata.storage_rebate)
                    .unwrap_or_default()
            })
            .collect();
        for (object, old_storage_rebate) in self
            .execution_results
            .written_objects
            .values_mut()
            .zip(old_storage_rebates)
        {
            // new object size
            let new_object_size = object.object_size_for_gas_metering();
            // track changes and compute the new object `storage_rebate`
            let new_storage_rebate = gas_charger.track_storage_mutation(
                object.id(),
                new_object_size,
                old_storage_rebate,
            );
            object.storage_rebate = new_storage_rebate;
        }

        self.collect_rebate(gas_charger);
    }

    pub(crate) fn collect_rebate(&self, gas_charger: &mut GasCharger) {
        for object_id in &self.execution_results.modified_objects {
            if self
                .execution_results
                .written_objects
                .contains_key(object_id)
            {
                continue;
            }
            // get and track the deleted object `storage_rebate`
            let storage_rebate = self
                .get_object_modified_at(object_id)
                // Unwrap is safe because this loop iterates through all modified objects.
                .unwrap()
                .storage_rebate;
            gas_charger.track_storage_mutation(*object_id, 0, storage_rebate);
        }
    }

    pub fn check_execution_results_consistency(&self) -> Result<(), ExecutionError> {
        assert_invariant!(
            self.execution_results
                .created_object_ids
                .iter()
                .all(|id| !self.execution_results.deleted_object_ids.contains(id)
                    && !self.execution_results.modified_objects.contains(id)),
            "Created object IDs cannot also be deleted or modified"
        );
        assert_invariant!(
            self.execution_results.modified_objects.iter().all(|id| {
                self.mutable_input_refs.contains_key(id)
                    || self.loaded_runtime_objects.contains_key(id)
                    || is_system_package(*id)
            }),
            "A modified object must be either a mutable input, a loaded child object, or a system package"
        );
        Ok(())
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
        let (old_object, new_object) =
            wrapper.advance_epoch_safe_mode(params, self.store.as_object_store(), protocol_config);
        self.mutate_child_object(old_object, new_object);
    }
}

type ModifiedObjectInfo<'a> = (
    ObjectID,
    // old object metadata, including version, digest, owner, and storage rebate.
    Option<DynamicallyLoadedObjectMetadata>,
    Option<&'a Object>,
);

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
            let Some(obj) = self.store.get_object_by_key(id, expected_version) else {
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
        self.execution_results
            .modified_objects
            .iter()
            .map(|id| {
                let metadata = self.get_object_modified_at(id);
                let output = self.execution_results.written_objects.get(id);
                (*id, metadata, output)
            })
            .chain(
                self.execution_results
                    .written_objects
                    .iter()
                    .filter_map(|(id, object)| {
                        if self.execution_results.modified_objects.contains(id) {
                            None
                        } else {
                            Some((*id, None, Some(object)))
                        }
                    }),
            )
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
            if let Some(input) = input {
                total_input_rebate += input.storage_rebate;
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
            if let Some(input) = input {
                total_input_sui += self.get_input_sui(&id, input.version, layout_resolver)?;
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
        let obj_opt = self.execution_results.written_objects.get(child);
        if obj_opt.is_some() {
            Ok(obj_opt.cloned())
        } else {
            self.store
                .read_child_object(parent, child, child_version_upper_bound)
        }
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> SuiResult<Option<Object>> {
        // You should never be able to try and receive an object after deleting it or writing it in the same
        // transaction since `Receiving` doesn't have copy.
        debug_assert!(!self
            .execution_results
            .written_objects
            .contains_key(receiving_object_id));
        debug_assert!(!self
            .execution_results
            .deleted_object_ids
            .contains(receiving_object_id));
        self.store.get_object_received_at_version(
            owner,
            receiving_object_id,
            receive_object_at_version,
            epoch_id,
            use_object_per_epoch_marker_table_v2,
        )
    }
}

impl<'backing> Storage for TemporaryStore<'backing> {
    fn reset(&mut self) {
        self.drop_writes();
    }

    fn read_object(&self, id: &ObjectID) -> Option<&Object> {
        TemporaryStore::read_object(self, id)
    }

    /// Take execution results v2, and translate it back to be compatible with effects v1.
    fn record_execution_results(&mut self, results: ExecutionResults) {
        let ExecutionResults::V2(results) = results else {
            panic!("ExecutionResults::V2 expected in sui-execution v1 and above");
        };
        // It's important to merge instead of override results because it's
        // possible to execute PT more than once during tx execution.
        self.execution_results.merge_results(results);
    }

    fn save_loaded_runtime_objects(
        &mut self,
        loaded_runtime_objects: BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
    ) {
        TemporaryStore::save_loaded_runtime_objects(self, loaded_runtime_objects)
    }

    fn save_wrapped_object_containers(
        &mut self,
        wrapped_object_containers: BTreeMap<ObjectID, ObjectID>,
    ) {
        TemporaryStore::save_wrapped_object_containers(self, wrapped_object_containers)
    }

    fn check_coin_deny_list(
        &self,
        _written_objects: &BTreeMap<ObjectID, Object>,
    ) -> DenyListResult {
        unreachable!("Coin denylist v2 is not supported in sui-execution v2");
    }
}

impl<'backing> BackingPackageStore for TemporaryStore<'backing> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        // We first check the objects in the temporary store because in non-production code path,
        // it is possible to read packages that are just written in the same transaction.
        // This can happen for example when we run the expensive conservation checks, where we may
        // look into the types of each written object in the output, and some of them need the
        // newly written packages for type checking.
        // In production path though, this should never happen.
        if let Some(obj) = self.execution_results.written_objects.get(package_id) {
            Ok(Some(PackageObject::new(obj.clone())))
        } else {
            self.store.get_package_object(package_id).inspect(|obj| {
                // Track object but leave unchanged
                if let Some(v) = obj {
                    if !self
                        .runtime_packages_loaded_from_db
                        .read()
                        .contains_key(package_id)
                    {
                        // TODO: Can this lock ever block execution?
                        // TODO: Another way to avoid the cost of maintaining this map is to not
                        // enable it in normal runs, and if a fork is detected, rerun it with a flag
                        // turned on and start populating this field.
                        self.runtime_packages_loaded_from_db
                            .write()
                            .insert(*package_id, v.clone());
                    }
                }
            })
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
    fn get_latest_parent_entry_ref_deprecated(&self, _object_id: ObjectID) -> Option<ObjectRef> {
        unreachable!("Never called in newer protocol versions")
    }
}
