// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod object_store;

use self::object_store::{ChildObjectEffectV0, ChildObjectEffects, ObjectResult};
use super::get_object_id;
use better_any::{Tid, TidAble};
use indexmap::map::IndexMap;
use indexmap::set::IndexSet;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveTypeLayout, MoveValue},
    annotated_visitor as AV,
    effects::Op,
    language_storage::StructTag,
    runtime_value as R,
    vm_status::StatusCode,
};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    values::{GlobalValue, Value},
};
use object_store::{ActiveChildObject, ChildObjectStore};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use sui_protocol_config::{check_limit_by_meter, LimitThresholdCrossed, ProtocolConfig};
use sui_types::{
    base_types::{MoveObjectType, ObjectID, SequenceNumber, SuiAddress},
    committee::EpochId,
    error::{ExecutionError, ExecutionErrorKind, VMMemoryLimitExceededSubStatusCode},
    execution::DynamicallyLoadedObjectMetadata,
    id::UID,
    metrics::LimitsMetrics,
    object::{MoveObject, Owner},
    storage::ChildObjectResolver,
    SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_BRIDGE_OBJECT_ID, SUI_CLOCK_OBJECT_ID,
    SUI_DENY_LIST_OBJECT_ID, SUI_RANDOMNESS_STATE_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID,
};
use tracing::error;

pub enum ObjectEvent {
    /// Transfer to a new address or object. Or make it shared or immutable.
    Transfer(Owner, MoveObject),
    /// An object ID is deleted
    DeleteObjectID(ObjectID),
}

type Set<K> = IndexSet<K>;

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
    // allocated receiving tickets
    pub(crate) allocated_tickets: BTreeMap<ObjectID, (DynamicallyLoadedObjectMetadata, Value)>,
}

pub struct LoadedRuntimeObject {
    pub version: SequenceNumber,
    pub is_modified: bool,
}

pub struct RuntimeResults {
    pub writes: IndexMap<ObjectID, (Owner, Type, Value)>,
    pub user_events: Vec<(Type, StructTag, Value)>,
    // Loaded child objects, their loaded version/digest and whether they were modified.
    pub loaded_child_objects: BTreeMap<ObjectID, LoadedRuntimeObject>,
    pub created_object_ids: Set<ObjectID>,
    pub deleted_object_ids: Set<ObjectID>,
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
    transfers: IndexMap<ObjectID, (Owner, Type, Value)>,
    events: Vec<(Type, StructTag, Value)>,
    // total size of events emitted so far
    total_events_size: u64,
    received: IndexMap<ObjectID, DynamicallyLoadedObjectMetadata>,
}

#[derive(Tid)]
pub struct ObjectRuntime<'a> {
    child_object_store: ChildObjectStore<'a>,
    // inventories for test scenario
    pub(crate) test_inventories: TestInventories,
    // the internal state
    pub(crate) state: ObjectRuntimeState,
    // whether or not this TX is gas metered
    is_metered: bool,

    pub(crate) protocol_config: &'a ProtocolConfig,
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
        protocol_config: &'a ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        epoch_id: EpochId,
    ) -> Self {
        let mut input_object_owners = BTreeMap::new();
        let mut root_version = BTreeMap::new();
        let mut wrapped_object_containers = BTreeMap::new();
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
                if contained_uid != id {
                    let prev = wrapped_object_containers.insert(contained_uid, id);
                    debug_assert!(prev.is_none());
                }
            }
        }
        Self {
            child_object_store: ChildObjectStore::new(
                object_resolver,
                root_version,
                wrapped_object_containers,
                is_metered,
                protocol_config,
                metrics.clone(),
                epoch_id,
            ),
            test_inventories: TestInventories::new(),
            state: ObjectRuntimeState {
                input_objects: input_object_owners,
                new_ids: Set::new(),
                deleted_ids: Set::new(),
                transfers: IndexMap::new(),
                events: vec![],
                total_events_size: 0,
                received: IndexMap::new(),
            },
            is_metered,
            protocol_config,
            metrics,
        }
    }

    pub fn new_id(&mut self, id: ObjectID) -> PartialVMResult<()> {
        // If metered, we use the metered limit (non system tx limit) as the hard limit
        // This macro takes care of that
        if let LimitThresholdCrossed::Hard(_, lim) = check_limit_by_meter!(
            self.is_metered,
            self.state.new_ids.len(),
            self.protocol_config.max_num_new_move_object_ids(),
            self.protocol_config.max_num_new_move_object_ids_system_tx(),
            self.metrics.excessive_new_move_object_ids
        ) {
            return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                .with_message(format!("Creating more than {} IDs is not allowed", lim))
                .with_sub_status(
                    VMMemoryLimitExceededSubStatusCode::NEW_ID_COUNT_LIMIT_EXCEEDED as u64,
                ));
        };

        // remove from deleted_ids for the case in dynamic fields where the Field object was deleted
        // and then re-added in a single transaction. In that case, we also skip adding it
        // to new_ids.
        let was_present = self.state.deleted_ids.shift_remove(&id);
        if !was_present {
            // mark the id as new
            self.state.new_ids.insert(id);
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
            self.protocol_config.max_num_deleted_move_object_ids(),
            self.protocol_config
                .max_num_deleted_move_object_ids_system_tx(),
            self.metrics.excessive_deleted_move_object_ids
        ) {
            return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                .with_message(format!("Deleting more than {} IDs is not allowed", lim))
                .with_sub_status(
                    VMMemoryLimitExceededSubStatusCode::DELETED_ID_COUNT_LIMIT_EXCEEDED as u64,
                ));
        };

        let was_new = self.state.new_ids.shift_remove(&id);
        if !was_new {
            self.state.deleted_ids.insert(id);
        }
        Ok(())
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
        let is_framework_obj = [
            SUI_SYSTEM_STATE_OBJECT_ID,
            SUI_CLOCK_OBJECT_ID,
            SUI_AUTHENTICATOR_STATE_OBJECT_ID,
            SUI_RANDOMNESS_STATE_OBJECT_ID,
            SUI_DENY_LIST_OBJECT_ID,
            SUI_BRIDGE_OBJECT_ID,
        ]
        .contains(&id);
        let transfer_result = if self.state.new_ids.contains(&id) {
            TransferResult::New
        } else if is_framework_obj {
            // framework objects are always created when they are transferred, but the id is
            // hard-coded so it is not yet in new_ids
            self.state.new_ids.insert(id);
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
            self.protocol_config.max_num_transferred_move_object_ids(),
            self.protocol_config
                .max_num_transferred_move_object_ids_system_tx(),
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
        if self.state.events.len() >= (self.protocol_config.max_num_event_emit() as usize) {
            return Err(max_event_error(self.protocol_config.max_num_event_emit()));
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
        self.child_object_store.object_exists(parent, child)
    }

    pub(crate) fn child_object_exists_and_has_type(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_type: &MoveObjectType,
    ) -> PartialVMResult<bool> {
        self.child_object_store
            .object_exists_and_has_type(parent, child, child_type)
    }

    pub(super) fn receive_object(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_version: SequenceNumber,
        child_ty: &Type,
        child_layout: &R::MoveTypeLayout,
        child_fully_annotated_layout: &MoveTypeLayout,
        child_move_type: MoveObjectType,
    ) -> PartialVMResult<Option<ObjectResult<Value>>> {
        let Some((value, obj_meta)) = self.child_object_store.receive_object(
            parent,
            child,
            child_version,
            child_ty,
            child_layout,
            child_fully_annotated_layout,
            child_move_type,
        )?
        else {
            return Ok(None);
        };
        // NB: It is important that the object only be added to the received set after it has been
        // fully authenticated and loaded.
        if self.state.received.insert(child, obj_meta).is_some() {
            // We should never hit this -- it means that we have received the same object twice which
            // means we have a duplicated a receiving ticket somehow.
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(format!(
                    "Object {child} at version {child_version} already received. This can only happen \
                    if multiple `Receiving` arguments exist for the same object in the transaction which is impossible."
                )),
            );
        }
        Ok(Some(value))
    }

    pub(crate) fn get_or_fetch_child_object(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_layout: &R::MoveTypeLayout,
        child_fully_annotated_layout: &MoveTypeLayout,
        child_move_type: MoveObjectType,
    ) -> PartialVMResult<ObjectResult<&mut GlobalValue>> {
        let res = self.child_object_store.get_or_fetch_object(
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
        self.child_object_store
            .add_object(parent, child, child_ty, child_move_type, child_value)
    }

    pub(crate) fn config_setting_unsequenced_read(
        &mut self,
        config_id: ObjectID,
        name_df_id: ObjectID,
        field_setting_ty: &Type,
        field_setting_layout: &R::MoveTypeLayout,
        field_setting_object_type: &MoveObjectType,
    ) -> Option<Value> {
        match self.child_object_store.config_setting_unsequenced_read(
            config_id,
            name_df_id,
            field_setting_ty,
            field_setting_layout,
            field_setting_object_type,
        ) {
            Err(e) => {
                error!(
                    "Failed to read config setting.
                    config_id: {config_id},
                    name_df_id: {name_df_id},
                    field_setting_object_type:  {field_setting_object_type:?},
                    error: {e}"
                );
                None
            }
            Ok(ObjectResult::MismatchedType) | Ok(ObjectResult::Loaded(None)) => None,
            Ok(ObjectResult::Loaded(Some(value))) => Some(value),
        }
    }

    pub(super) fn config_setting_cache_update(
        &mut self,
        config_id: ObjectID,
        name_df_id: ObjectID,
        setting_value_object_type: MoveObjectType,
        value: Option<Value>,
    ) {
        self.child_object_store.config_setting_cache_update(
            config_id,
            name_df_id,
            setting_value_object_type,
            value,
        )
    }

    // returns None if a child object is still borrowed
    pub(crate) fn take_state(&mut self) -> ObjectRuntimeState {
        std::mem::take(&mut self.state)
    }

    pub fn finish(mut self) -> Result<RuntimeResults, ExecutionError> {
        let loaded_child_objects = self.loaded_runtime_objects();
        let child_effects = self.child_object_store.take_effects();
        self.state.finish(loaded_child_objects, child_effects)
    }

    pub(crate) fn all_active_child_objects(&self) -> impl Iterator<Item = ActiveChildObject<'_>> {
        self.child_object_store.all_active_objects()
    }

    pub fn loaded_runtime_objects(&self) -> BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata> {
        // The loaded child objects, and the received objects, should be disjoint. If they are not,
        // this is an error since it could lead to incorrect transaction dependency computations.
        debug_assert!(self
            .child_object_store
            .cached_objects()
            .keys()
            .all(|id| !self.state.received.contains_key(id)));
        self.child_object_store
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
            .chain(
                self.state
                    .received
                    .iter()
                    .map(|(id, meta)| (*id, meta.clone())),
            )
            .collect()
    }

    /// A map from wrapped objects to the object that wraps them at the beginning of the
    /// transaction.
    pub fn wrapped_object_containers(&self) -> BTreeMap<ObjectID, ObjectID> {
        self.child_object_store.wrapped_object_containers().clone()
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
        loaded_child_objects: BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
        child_object_effects: ChildObjectEffects,
    ) -> Result<RuntimeResults, ExecutionError> {
        let mut loaded_child_objects: BTreeMap<_, _> = loaded_child_objects
            .into_iter()
            .map(|(id, metadata)| {
                (
                    id,
                    LoadedRuntimeObject {
                        version: metadata.version,
                        is_modified: false,
                    },
                )
            })
            .collect();
        self.apply_child_object_effects(&mut loaded_child_objects, child_object_effects);
        let ObjectRuntimeState {
            input_objects: _,
            new_ids,
            deleted_ids,
            transfers,
            events: user_events,
            total_events_size: _,
            received,
        } = self;

        // Check new owners from transfers, reports an error on cycles.
        // TODO can we have cycles in the new system?
        check_circular_ownership(
            transfers
                .iter()
                .map(|(id, (owner, _, _))| (*id, owner.clone())),
        )?;
        // For both written_objects and deleted_ids, we need to mark the loaded child object as modified.
        // These may not be covered in the child object effects if they are taken out in one PT command and then
        // transferred/deleted in a different command. Marking them as modified will allow us properly determine their
        // mutation category in effects.
        // TODO: This could get error-prone quickly: what if we forgot to mark an object as modified? There may be a cleaner
        // sulution.
        let written_objects: IndexMap<_, _> = transfers
            .into_iter()
            .map(|(id, (owner, type_, value))| {
                if let Some(loaded_child) = loaded_child_objects.get_mut(&id) {
                    loaded_child.is_modified = true;
                }
                (id, (owner, type_, value))
            })
            .collect();
        for deleted_id in &deleted_ids {
            if let Some(loaded_child) = loaded_child_objects.get_mut(deleted_id) {
                loaded_child.is_modified = true;
            }
        }

        // Any received objects are viewed as modified. They had to be loaded in order to be
        // received so they must be in the loaded_child_objects map otherwise it's an invariant
        // violation.
        for (received_object, _) in received.into_iter() {
            match loaded_child_objects.get_mut(&received_object) {
                Some(loaded_child) => {
                    loaded_child.is_modified = true;
                }
                None => {
                    return Err(ExecutionError::invariant_violation(format!(
                        "Failed to find received UID {received_object} in loaded child objects."
                    )))
                }
            }
        }

        Ok(RuntimeResults {
            writes: written_objects,
            user_events,
            loaded_child_objects,
            created_object_ids: new_ids,
            deleted_object_ids: deleted_ids,
        })
    }

    pub fn events(&self) -> &[(Type, StructTag, Value)] {
        &self.events
    }

    pub fn total_events_size(&self) -> u64 {
        self.total_events_size
    }

    pub fn incr_total_events_size(&mut self, size: u64) {
        self.total_events_size += size;
    }

    fn apply_child_object_effects(
        &mut self,
        loaded_child_objects: &mut BTreeMap<ObjectID, LoadedRuntimeObject>,
        child_object_effects: ChildObjectEffects,
    ) {
        match child_object_effects {
            ChildObjectEffects::V0(child_object_effects) => {
                self.apply_child_object_effects_v0(loaded_child_objects, child_object_effects)
            }
        }
    }

    fn apply_child_object_effects_v0(
        &mut self,
        loaded_child_objects: &mut BTreeMap<ObjectID, LoadedRuntimeObject>,
        child_object_effects: BTreeMap<ObjectID, ChildObjectEffectV0>,
    ) {
        for (child, child_object_effect) in child_object_effects {
            let ChildObjectEffectV0 {
                owner: parent,
                ty,
                effect,
            } = child_object_effect;

            if let Some(loaded_child) = loaded_child_objects.get_mut(&child) {
                loaded_child.is_modified = true;
            }

            match effect {
                // was modified, so mark it as mutated and transferred
                Op::Modify(v) => {
                    debug_assert!(!self.transfers.contains_key(&child));
                    debug_assert!(!self.new_ids.contains(&child));
                    debug_assert!(loaded_child_objects.contains_key(&child));
                    self.transfers
                        .insert(child, (Owner::ObjectOwner(parent.into()), ty, v));
                }

                Op::New(v) => {
                    debug_assert!(!self.transfers.contains_key(&child));
                    self.transfers
                        .insert(child, (Owner::ObjectOwner(parent.into()), ty, v));
                }

                Op::Delete => {
                    // was transferred so not actually deleted
                    if self.transfers.contains_key(&child) {
                        debug_assert!(!self.deleted_ids.contains(&child));
                    }
                    // ID was deleted too was deleted so mark as deleted
                    if self.deleted_ids.contains(&child) {
                        debug_assert!(!self.transfers.contains_key(&child));
                        debug_assert!(!self.new_ids.contains(&child));
                    }
                }
            }
        }
    }
}

fn check_circular_ownership(
    transfers: impl IntoIterator<Item = (ObjectID, Owner)>,
) -> Result<(), ExecutionError> {
    let mut object_owner_map = BTreeMap::new();
    for (id, recipient) in transfers {
        object_owner_map.remove(&id);
        match recipient {
            Owner::AddressOwner(_)
            | Owner::Shared { .. }
            | Owner::Immutable
            | Owner::ConsensusV2 { .. } => (),
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
    fully_annotated_layout: &MoveTypeLayout,
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

    MoveValue::visit_deserialize(
        bcs_bytes,
        fully_annotated_layout,
        &mut UIDTraversal(&mut ids),
    )
    .map_err(|e| format!("Failed to deserialize. {e:?}"))?;
    Ok(ids)
}
