// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use better_any::{Tid, TidAble};
use linked_hash_map::LinkedHashMap;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress, annotated_value as A, annotated_visitor as AV, effects::Op,
    language_storage::StructTag, runtime_value as R, vm_status::StatusCode,
};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    values::{GlobalValue, Value},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use sui_protocol_config::{check_limit_by_meter, LimitThresholdCrossed, ProtocolConfig};
use sui_types::{
    base_types::{MoveObjectType, ObjectID, SequenceNumber, SuiAddress},
    error::{ExecutionError, ExecutionErrorKind, VMMemoryLimitExceededSubStatusCode},
    execution::DynamicallyLoadedObjectMetadata,
    id::UID,
    metrics::LimitsMetrics,
    object::{MoveObject, Owner},
    storage::{ChildObjectResolver, DeleteKind, WriteKind},
    SUI_CLOCK_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID,
};

pub(crate) mod object_store;

use object_store::ObjectStore;

use self::object_store::{ChildObjectEffect, ObjectResult};

use super::get_object_id;

pub enum ObjectEvent {
    /// Transfer to a new address or object. Or make it shared or immutable.
    Transfer(Owner, MoveObject),
    /// An object ID is deleted
    DeleteObjectID(ObjectID),
}

// LinkedHashSet has a bug for accessing the back/last element
type Set<K> = LinkedHashMap<K, ()>;

#[derive(Default)]
pub(crate) struct TestInventories {
    pub(crate) objects: BTreeMap<ObjectID, Value>,
    // address inventories. Most recent objects are at the back of the set
    pub(crate) address_inventories: BTreeMap<SuiAddress, BTreeMap<Type, Set<ObjectID>>>,
    // global inventories.Most recent objects are at the back of the set
    pub(crate) shared_inventory: BTreeMap<Type, Set<ObjectID>>,
    pub(crate) immutable_inventory: BTreeMap<Type, Set<ObjectID>>,
    pub(crate) taken_immutable_values: BTreeMap<Type, BTreeMap<ObjectID, Value>>,
    // object has been taken from the inventory
    pub(crate) taken: BTreeMap<ObjectID, Owner>,
}

pub struct RuntimeResults {
    pub writes: LinkedHashMap<ObjectID, (WriteKind, Owner, Type, Value)>,
    pub deletions: LinkedHashMap<ObjectID, DeleteKind>,
    pub user_events: Vec<(Type, StructTag, Value)>,
    // loaded child objects and their versions
    pub loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
}

#[derive(Default)]
pub(crate) struct ObjectRuntimeState {
    pub(crate) input_objects: BTreeMap<ObjectID, Owner>,
    // new ids from object::new
    new_ids: Set<ObjectID>,
    // ids passed to object::delete
    deleted_ids: Set<ObjectID>,
    // transfers to a new owner (shared, immutable, object, or account address)
    // TODO these struct tags can be removed if type_to_type_tag was exposed in the session
    transfers: LinkedHashMap<ObjectID, (Owner, Type, Value)>,
    events: Vec<(Type, StructTag, Value)>,
}

#[derive(Clone)]
pub(crate) struct LocalProtocolConfig {
    pub(crate) max_num_deleted_move_object_ids: u64,
    pub(crate) max_num_deleted_move_object_ids_system_tx: u64,
    pub(crate) max_num_event_emit: u64,
    pub(crate) max_num_new_move_object_ids: u64,
    pub(crate) max_num_new_move_object_ids_system_tx: u64,
    pub(crate) max_num_transferred_move_object_ids: u64,
    pub(crate) max_num_transferred_move_object_ids_system_tx: u64,
    pub(crate) max_event_emit_size: u64,
    pub(crate) object_runtime_max_num_cached_objects: u64,
    pub(crate) object_runtime_max_num_cached_objects_system_tx: u64,
    pub(crate) object_runtime_max_num_store_entries: u64,
    pub(crate) object_runtime_max_num_store_entries_system_tx: u64,
}

impl LocalProtocolConfig {
    fn new(constants: &ProtocolConfig) -> Self {
        Self {
            max_num_deleted_move_object_ids: constants.max_num_deleted_move_object_ids(),
            max_num_event_emit: constants.max_num_event_emit(),
            max_num_new_move_object_ids: constants.max_num_new_move_object_ids(),
            max_num_transferred_move_object_ids: constants.max_num_transferred_move_object_ids(),
            max_event_emit_size: constants.max_event_emit_size(),
            max_num_deleted_move_object_ids_system_tx: constants
                .max_num_deleted_move_object_ids_system_tx(),
            max_num_new_move_object_ids_system_tx: constants
                .max_num_new_move_object_ids_system_tx(),
            max_num_transferred_move_object_ids_system_tx: constants
                .max_num_transferred_move_object_ids_system_tx(),

            object_runtime_max_num_cached_objects: constants
                .object_runtime_max_num_cached_objects(),
            object_runtime_max_num_cached_objects_system_tx: constants
                .object_runtime_max_num_cached_objects_system_tx(),
            object_runtime_max_num_store_entries: constants.object_runtime_max_num_store_entries(),
            object_runtime_max_num_store_entries_system_tx: constants
                .object_runtime_max_num_store_entries_system_tx(),
        }
    }
}

#[derive(Tid)]
pub struct ObjectRuntime<'a> {
    object_store: ObjectStore<'a>,
    // inventories for test scenario
    pub(crate) test_inventories: TestInventories,
    // the internal state
    pub(crate) state: ObjectRuntimeState,
    // whether or not this TX is gas metered
    is_metered: bool,
    // FIXED BEHAVIOR if true, correctly take the loaded object versions from the object store
    // LEGACY if false, recalculate the loaded child object versions from child object changes
    loaded_child_objects_fixed: bool,

    pub(crate) constants: LocalProtocolConfig,
    pub(crate) metrics: Arc<LimitsMetrics>,
}

pub enum TransferResult {
    New,
    SameOwner,
    OwnerChanged,
}

pub struct InputObject {
    pub contained_uids: BTreeSet<ObjectID>,
    pub version: SequenceNumber,
    pub owner: Owner,
}

impl TestInventories {
    fn new() -> Self {
        Self::default()
    }
}

impl<'a> ObjectRuntime<'a> {
    pub fn new(
        object_resolver: &'a dyn ChildObjectResolver,
        input_objects: BTreeMap<ObjectID, InputObject>,
        is_metered: bool,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
    ) -> Self {
        let mut input_object_owners = BTreeMap::new();
        let mut root_version = BTreeMap::new();
        for (id, input_object) in input_objects {
            let InputObject {
                contained_uids,
                version,
                owner,
            } = input_object;
            input_object_owners.insert(id, owner);
            debug_assert!(contained_uids.contains(&id));
            for contained_uid in contained_uids {
                root_version.insert(contained_uid, version);
            }
        }
        Self {
            object_store: ObjectStore::new(
                object_resolver,
                root_version,
                is_metered,
                LocalProtocolConfig::new(protocol_config),
                metrics.clone(),
            ),
            test_inventories: TestInventories::new(),
            state: ObjectRuntimeState {
                input_objects: input_object_owners,
                new_ids: Set::new(),
                deleted_ids: Set::new(),
                transfers: LinkedHashMap::new(),
                events: vec![],
            },
            is_metered,
            loaded_child_objects_fixed: protocol_config.loaded_child_objects_fixed(),
            constants: LocalProtocolConfig::new(protocol_config),
            metrics,
        }
    }

    pub fn new_id(&mut self, id: ObjectID) -> PartialVMResult<()> {
        // If metered, we use the metered limit (non system tx limit) as the hard limit
        // This macro takes care of that
        if let LimitThresholdCrossed::Hard(_, lim) = check_limit_by_meter!(
            self.is_metered,
            self.state.new_ids.len(),
            self.constants.max_num_new_move_object_ids,
            self.constants.max_num_new_move_object_ids_system_tx,
            self.metrics.excessive_new_move_object_ids
        ) {
            return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                .with_message(format!("Creating more than {} IDs is not allowed", lim))
                .with_sub_status(
                    VMMemoryLimitExceededSubStatusCode::NEW_ID_COUNT_LIMIT_EXCEEDED as u64,
                ));
        };

        // remove from deleted_ids for the case in dynamic fields where the Field object was deleted
        // and then re-added in a single transaction
        if self.state.deleted_ids.remove(&id).is_none() {
            // mark the id as new
            self.state.new_ids.insert(id, ());
        }
        Ok(())
    }

    pub fn delete_id(&mut self, id: ObjectID) -> PartialVMResult<()> {
        // This is defensive because `self.state.deleted_ids` may not indeed
        // be called based on the `was_new` flag
        // Metered transactions don't have limits for now

        if let LimitThresholdCrossed::Hard(_, lim) = check_limit_by_meter!(
            self.is_metered,
            self.state.deleted_ids.len(),
            self.constants.max_num_deleted_move_object_ids,
            self.constants.max_num_deleted_move_object_ids_system_tx,
            self.metrics.excessive_deleted_move_object_ids
        ) {
            return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                .with_message(format!("Deleting more than {} IDs is not allowed", lim))
                .with_sub_status(
                    VMMemoryLimitExceededSubStatusCode::DELETED_ID_COUNT_LIMIT_EXCEEDED as u64,
                ));
        };

        let was_new = self.state.new_ids.remove(&id).is_some();
        if !was_new {
            self.state.deleted_ids.insert(id, ());
        }
        Ok(())
    }

    pub fn new_ids(&self) -> &Set<ObjectID> {
        &self.state.new_ids
    }

    pub fn transfer(
        &mut self,
        owner: Owner,
        ty: Type,
        obj: Value,
    ) -> PartialVMResult<TransferResult> {
        let id: ObjectID = get_object_id(obj.copy_value()?)?
            .value_as::<AccountAddress>()?
            .into();
        // - An object is new if it is contained in the new ids or if it is one of the objects
        //   created during genesis (the system state object or clock).
        // - Otherwise, check the input objects for the previous owner
        // - If it was not in the input objects, it must have been wrapped or must have been a
        //   child object
        let is_framework_obj = [SUI_SYSTEM_STATE_OBJECT_ID, SUI_CLOCK_OBJECT_ID].contains(&id);
        let transfer_result = if self.state.new_ids.contains_key(&id) || is_framework_obj {
            TransferResult::New
        } else if let Some(prev_owner) = self.state.input_objects.get(&id) {
            match (&owner, prev_owner) {
                // don't use == for dummy values in Shared owner
                (Owner::Shared { .. }, Owner::Shared { .. }) => TransferResult::SameOwner,
                (new, old) if new == old => TransferResult::SameOwner,
                _ => TransferResult::OwnerChanged,
            }
        } else {
            TransferResult::OwnerChanged
        };

        // Metered transactions don't have limits for now

        if let LimitThresholdCrossed::Hard(_, lim) = check_limit_by_meter!(
            // TODO: is this not redundant? Metered TX implies framework obj cannot be transferred
            self.is_metered && !is_framework_obj, // We have higher limits for unmetered transactions and framework obj
            self.state.transfers.len(),
            self.constants.max_num_transferred_move_object_ids,
            self.constants.max_num_transferred_move_object_ids_system_tx,
            self.metrics.excessive_transferred_move_object_ids
        ) {
            return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                .with_message(format!("Transferring more than {} IDs is not allowed", lim))
                .with_sub_status(
                    VMMemoryLimitExceededSubStatusCode::TRANSFER_ID_COUNT_LIMIT_EXCEEDED as u64,
                ));
        };

        self.state.transfers.insert(id, (owner, ty, obj));
        Ok(transfer_result)
    }

    pub fn emit_event(&mut self, ty: Type, tag: StructTag, event: Value) -> PartialVMResult<()> {
        if self.state.events.len() >= (self.constants.max_num_event_emit as usize) {
            return Err(max_event_error(self.constants.max_num_event_emit));
        }
        self.state.events.push((ty, tag, event));
        Ok(())
    }

    pub fn take_user_events(&mut self) -> Vec<(Type, StructTag, Value)> {
        std::mem::take(&mut self.state.events)
    }

    pub(crate) fn child_object_exists(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
    ) -> PartialVMResult<bool> {
        self.object_store.object_exists(parent, child)
    }

    pub(crate) fn child_object_exists_and_has_type(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_type: &MoveObjectType,
    ) -> PartialVMResult<bool> {
        self.object_store
            .object_exists_and_has_type(parent, child, child_type)
    }

    pub(crate) fn get_or_fetch_child_object(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_layout: &R::MoveTypeLayout,
        child_fully_annotated_layout: &A::MoveTypeLayout,
        child_move_type: MoveObjectType,
    ) -> PartialVMResult<ObjectResult<&mut GlobalValue>> {
        let res = self.object_store.get_or_fetch_object(
            parent,
            child,
            child_ty,
            child_layout,
            child_fully_annotated_layout,
            child_move_type,
        )?;
        Ok(match res {
            ObjectResult::MismatchedType => ObjectResult::MismatchedType,
            ObjectResult::Loaded(child_object) => ObjectResult::Loaded(&mut child_object.value),
        })
    }

    pub(crate) fn add_child_object(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_move_type: MoveObjectType,
        child_value: Value,
    ) -> PartialVMResult<()> {
        self.object_store
            .add_object(parent, child, child_ty, child_move_type, child_value)
    }

    // returns None if a child object is still borrowed
    pub(crate) fn take_state(&mut self) -> ObjectRuntimeState {
        std::mem::take(&mut self.state)
    }

    pub fn finish(
        mut self,
        by_value_inputs: BTreeSet<ObjectID>,
        external_transfers: BTreeSet<ObjectID>,
    ) -> Result<RuntimeResults, ExecutionError> {
        let (loaded_child_objects, child_effects) = self.object_store.take_effects();
        self.state.finish(
            by_value_inputs,
            external_transfers,
            loaded_child_objects,
            child_effects,
            self.loaded_child_objects_fixed,
        )
    }

    pub(crate) fn all_active_child_objects(
        &self,
    ) -> impl Iterator<Item = (&ObjectID, &Type, Value)> {
        self.object_store.all_active_objects()
    }

    pub fn loaded_child_objects(&self) -> BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata> {
        self.object_store
            .cached_objects()
            .iter()
            .filter_map(|(id, obj_opt)| {
                obj_opt.as_ref().map(|obj| {
                    (
                        *id,
                        DynamicallyLoadedObjectMetadata {
                            version: obj.version(),
                            digest: obj.digest(),
                            storage_rebate: obj.storage_rebate,
                            owner: obj.owner.clone(),
                            previous_transaction: obj.previous_transaction,
                        },
                    )
                })
            })
            .collect()
    }
}

pub fn max_event_error(max_events: u64) -> PartialVMError {
    PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
        .with_message(format!(
            "Emitting more than {} events is not allowed",
            max_events
        ))
        .with_sub_status(VMMemoryLimitExceededSubStatusCode::EVENT_COUNT_LIMIT_EXCEEDED as u64)
}

impl ObjectRuntimeState {
    /// Update `state_view` with the effects of successfully executing a transaction:
    /// - Given the effects `Op<Value>` of child objects, processes the changes in terms of
    ///   object writes/deletes
    /// - Process `transfers` and `input_objects` to determine whether the type of change
    ///   (WriteKind) to the object
    /// - Process `deleted_ids` with previously determined information to determine the
    ///   DeleteKind
    /// - Passes through user events
    pub(crate) fn finish(
        mut self,
        by_value_inputs: BTreeSet<ObjectID>,
        external_transfers: BTreeSet<ObjectID>,
        mut loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
        child_object_effects: BTreeMap<ObjectID, ChildObjectEffect>,
        loaded_child_objects_fixed: bool,
    ) -> Result<RuntimeResults, ExecutionError> {
        let mut wrapped_children = BTreeSet::new();
        if !loaded_child_objects_fixed {
            loaded_child_objects = BTreeMap::new();
            for (child, child_object_effect) in &child_object_effects {
                if let Some(version) = child_object_effect.loaded_version {
                    loaded_child_objects.insert(*child, version);
                }
            }
        }
        for (child, child_object_effect) in child_object_effects {
            let ChildObjectEffect {
                owner: parent,
                loaded_version,
                ty,
                effect,
            } = child_object_effect;

            match effect {
                // was modified, so mark it as mutated and transferred
                Op::Modify(v) => {
                    debug_assert!(!self.transfers.contains_key(&child));
                    debug_assert!(!self.new_ids.contains_key(&child));
                    debug_assert!(loaded_version.is_some());
                    self.transfers
                        .insert(child, (Owner::ObjectOwner(parent.into()), ty, v));
                }

                Op::New(v) => {
                    debug_assert!(!self.transfers.contains_key(&child));
                    self.transfers
                        .insert(child, (Owner::ObjectOwner(parent.into()), ty, v));
                }
                // was transferred so not actually deleted
                Op::Delete if self.transfers.contains_key(&child) => {
                    debug_assert!(!self.deleted_ids.contains_key(&child));
                }
                // ID was deleted too was deleted so mark as deleted
                Op::Delete if self.deleted_ids.contains_key(&child) => {
                    debug_assert!(!self.transfers.contains_key(&child));
                    debug_assert!(!self.new_ids.contains_key(&child));
                }
                // was new so the object is transient and does not need to be marked as deleted
                Op::Delete if self.new_ids.contains_key(&child) => {}
                // child was transferred externally to the runtime
                Op::Delete if external_transfers.contains(&child) => {}
                // otherwise it must have been wrapped
                Op::Delete => {
                    wrapped_children.insert(child);
                }
            }
        }
        let ObjectRuntimeState {
            input_objects,
            new_ids,
            deleted_ids,
            transfers,
            events: user_events,
        } = self;
        let input_owner_map = input_objects
            .iter()
            .filter_map(|(id, owner)| match owner {
                Owner::AddressOwner(_) | Owner::Shared { .. } | Owner::Immutable => None,
                Owner::ObjectOwner(parent) => Some((*id, (*parent).into())),
                Owner::ConsensusV2 { .. } => {
                    unimplemented!("ConsensusV2 does not exist for this execution version")
                }
            })
            .collect();
        // update the input owners with the new owners from transfers
        // reports an error on cycles
        // TODO can we have cycles in the new system?
        update_owner_map(
            input_owner_map,
            transfers
                .iter()
                .map(|(id, (owner, _, _))| (*id, owner.clone())),
        )?;
        // determine write kinds
        let writes: LinkedHashMap<_, _> = transfers
            .into_iter()
            .map(|(id, (owner, type_, value))| {
                let write_kind =
                    if input_objects.contains_key(&id) || loaded_child_objects.contains_key(&id) {
                        debug_assert!(!new_ids.contains_key(&id));
                        WriteKind::Mutate
                    } else if id == SUI_SYSTEM_STATE_OBJECT_ID || new_ids.contains_key(&id) {
                        // SUI_SYSTEM_STATE_OBJECT_ID is only transferred during genesis
                        // TODO find a way to insert this in the new_ids during genesis transactions
                        WriteKind::Create
                    } else {
                        WriteKind::Unwrap
                    };
                (id, (write_kind, owner, type_, value))
            })
            .collect();
        // determine delete kinds
        let mut deletions: LinkedHashMap<_, _> = deleted_ids
            .into_iter()
            .map(|(id, ())| {
                debug_assert!(!new_ids.contains_key(&id));
                let delete_kind =
                    if input_objects.contains_key(&id) || loaded_child_objects.contains_key(&id) {
                        DeleteKind::Normal
                    } else {
                        DeleteKind::UnwrapThenDelete
                    };
                (id, delete_kind)
            })
            .collect();
        // remaining by value objects must be wrapped
        let remaining_by_value_objects = by_value_inputs
            .into_iter()
            .filter(|id| {
                !writes.contains_key(id)
                    && !deletions.contains_key(id)
                    && !external_transfers.contains(id)
            })
            .collect::<Vec<_>>();
        for id in remaining_by_value_objects {
            deletions.insert(id, DeleteKind::Wrap);
        }
        // children that weren't deleted or transferred must be wrapped
        for id in wrapped_children {
            deletions.insert(id, DeleteKind::Wrap);
        }

        debug_assert!(writes.keys().all(|id| !deletions.contains_key(id)));
        debug_assert!(deletions.keys().all(|id| !writes.contains_key(id)));
        Ok(RuntimeResults {
            writes,
            deletions,
            user_events,
            loaded_child_objects,
        })
    }
}

fn update_owner_map(
    mut object_owner_map: BTreeMap<ObjectID, ObjectID>,
    transfers: impl IntoIterator<Item = (ObjectID, Owner)>,
) -> Result<(), ExecutionError> {
    for (id, recipient) in transfers {
        object_owner_map.remove(&id);
        match recipient {
            Owner::AddressOwner(_) | Owner::Shared { .. } | Owner::Immutable => (),
            Owner::ObjectOwner(new_owner) => {
                let new_owner: ObjectID = new_owner.into();
                let mut cur = new_owner;
                loop {
                    if cur == id {
                        return Err(ExecutionError::from_kind(
                            ExecutionErrorKind::CircularObjectOwnership { object: cur },
                        ));
                    }
                    if let Some(parent) = object_owner_map.get(&cur) {
                        cur = *parent;
                    } else {
                        break;
                    }
                }
                object_owner_map.insert(id, new_owner);
            }
            Owner::ConsensusV2 { .. } => {
                unimplemented!("ConsensusV2 does not exist for this execution version")
            }
        }
    }
    Ok(())
}

/// WARNING! This function assumes that the bcs bytes have already been validated,
/// and it will give an invariant violation otherwise.
/// In short, we are relying on the invariant that the bytes are valid for objects
/// in storage.  We do not need this invariant for dev-inspect, as the programmable
/// transaction execution will validate the bytes before we get to this point.
pub fn get_all_uids(
    fully_annotated_layout: &A::MoveTypeLayout,
    bcs_bytes: &[u8],
) -> Result<BTreeSet<ObjectID>, /* invariant violation */ String> {
    let mut ids = BTreeSet::new();
    struct UIDTraversal<'i>(&'i mut BTreeSet<ObjectID>);
    struct UIDCollector<'i>(&'i mut BTreeSet<ObjectID>);

    impl<'i, 'b, 'l> AV::Traversal<'b, 'l> for UIDTraversal<'i> {
        type Error = AV::Error;

        fn traverse_struct(
            &mut self,
            driver: &mut AV::StructDriver<'_, 'b, 'l>,
        ) -> Result<(), Self::Error> {
            if driver.struct_layout().type_ == UID::type_() {
                while driver.next_field(&mut UIDCollector(self.0))?.is_some() {}
            } else {
                while driver.next_field(self)?.is_some() {}
            }
            Ok(())
        }
    }

    impl<'i, 'b, 'l> AV::Traversal<'b, 'l> for UIDCollector<'i> {
        type Error = AV::Error;
        fn traverse_address(
            &mut self,
            _driver: &AV::ValueDriver<'_, 'b, 'l>,
            value: AccountAddress,
        ) -> Result<(), Self::Error> {
            self.0.insert(value.into());
            Ok(())
        }
    }

    A::MoveValue::visit_deserialize(
        bcs_bytes,
        fully_annotated_layout,
        &mut UIDTraversal(&mut ids),
    )
    .map_err(|e| format!("Failed to deserialize. {e:?}"))?;
    Ok(ids)
}
