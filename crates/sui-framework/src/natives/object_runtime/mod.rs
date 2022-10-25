// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use better_any::{Tid, TidAble};
use linked_hash_map::LinkedHashMap;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    account_address::AccountAddress, effects::Op, language_storage::StructTag,
    value::MoveTypeLayout,
};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    values::{GlobalValue, Value},
};
use std::collections::{BTreeMap, BTreeSet};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    error::{ExecutionError, ExecutionErrorKind},
    object::{MoveObject, Owner},
    storage::{ChildObjectResolver, DeleteKind, WriteKind},
    SUI_SYSTEM_STATE_OBJECT_ID,
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
    pub writes: LinkedHashMap<ObjectID, (WriteKind, Owner, Type, StructTag, Value)>,
    pub deletions: LinkedHashMap<ObjectID, DeleteKind>,
    pub user_events: Vec<(Type, StructTag, Value)>,
    // loaded child objects and their versions
    pub loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
}

#[derive(Default)]
pub(crate) struct ObjectRuntimeState {
    pub(crate) input_objects: BTreeMap<ObjectID, (/* by_value */ bool, Owner)>,
    // new ids from object::new
    new_ids: Set<ObjectID>,
    // ids passed to object::delete
    deleted_ids: Set<ObjectID>,
    // transfers to a new owner (shared, immutable, object, or account address)
    // TODO these struct tags can be removed if type_to_type_tag was exposed in the session
    transfers: LinkedHashMap<ObjectID, (Owner, Type, StructTag, Value)>,
    events: Vec<(Type, StructTag, Value)>,
}

#[derive(Tid)]
pub struct ObjectRuntime<'a> {
    object_store: ObjectStore<'a>,
    // inventories for test scenario
    pub(crate) test_inventories: TestInventories,
    // the internal state
    pub(crate) state: ObjectRuntimeState,
}

impl TestInventories {
    fn new() -> Self {
        Self::default()
    }
}

impl<'a> ObjectRuntime<'a> {
    pub fn new(
        object_resolver: Box<dyn ChildObjectResolver + 'a>,
        input_objects: BTreeMap<ObjectID, (/* by_value */ bool, Owner)>,
    ) -> Self {
        Self {
            object_store: ObjectStore::new(object_resolver),
            test_inventories: TestInventories::new(),
            state: ObjectRuntimeState {
                input_objects,
                new_ids: Set::new(),
                deleted_ids: Set::new(),
                transfers: LinkedHashMap::new(),
                events: vec![],
            },
        }
    }

    pub fn new_id(&mut self, id: ObjectID) {
        self.state.new_ids.insert(id, ());
    }

    pub fn delete_id(&mut self, id: ObjectID) {
        let was_new = self.state.new_ids.remove(&id).is_some();
        if !was_new {
            self.state.deleted_ids.insert(id, ());
        }
    }

    pub fn transfer(
        &mut self,
        owner: Owner,
        ty: Type,
        tag: StructTag,
        obj: Value,
    ) -> PartialVMResult<()> {
        let id: ObjectID = get_object_id(obj.copy_value()?)?
            .value_as::<AccountAddress>()?
            .into();
        self.state.transfers.insert(id, (owner, ty, tag, obj));
        Ok(())
    }

    pub fn emit_event(&mut self, ty: Type, tag: StructTag, event: Value) {
        self.state.events.push((ty, tag, event))
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
        child_tag: StructTag,
    ) -> PartialVMResult<bool> {
        self.object_store
            .object_exists_and_has_type(parent, child, child_tag)
    }

    pub(crate) fn get_or_fetch_child_object(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_layout: MoveTypeLayout,
        child_tag: StructTag,
    ) -> PartialVMResult<ObjectResult<&mut GlobalValue>> {
        let res = self.object_store.get_or_fetch_object(
            parent,
            child,
            child_ty,
            child_layout,
            child_tag,
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
        child_tag: StructTag,
        child_value: Value,
    ) -> PartialVMResult<()> {
        self.object_store
            .add_object(parent, child, child_ty, child_tag, child_value)
    }

    // returns None if a child object is still borrowed
    pub(crate) fn take_state(&mut self) -> ObjectRuntimeState {
        std::mem::take(&mut self.state)
    }

    pub fn finish(mut self) -> Result<RuntimeResults, ExecutionError> {
        let child_effects = self.object_store.take_effects();
        self.state.finish(child_effects)
    }

    pub(crate) fn all_active_child_objects(
        &self,
    ) -> impl Iterator<Item = (&ObjectID, &Type, Value)> {
        self.object_store.all_active_objects()
    }
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
        child_object_effects: BTreeMap<ObjectID, ChildObjectEffect>,
    ) -> Result<RuntimeResults, ExecutionError> {
        let mut wrapped_children = BTreeSet::new();
        let mut loaded_child_objects = BTreeMap::new();
        for (child, child_object_effect) in child_object_effects {
            let ChildObjectEffect {
                owner: parent,
                loaded_version,
                ty,
                tag,
                effect,
            } = child_object_effect;
            if let Some(v) = loaded_version {
                loaded_child_objects.insert(child, v);
            }
            match effect {
                // was modified, so mark it as mutated and transferred
                Op::Modify(v) => {
                    debug_assert!(!self.transfers.contains_key(&child));
                    debug_assert!(!self.new_ids.contains_key(&child));
                    debug_assert!(loaded_version.is_some());
                    self.transfers
                        .insert(child, (Owner::ObjectOwner(parent.into()), ty, tag, v));
                }

                Op::New(v) => {
                    debug_assert!(!self.transfers.contains_key(&child));
                    self.transfers
                        .insert(child, (Owner::ObjectOwner(parent.into()), ty, tag, v));
                }
                // was transferred so not actually deleted
                Op::Delete if self.transfers.contains_key(&child) => {
                    debug_assert!(!self.deleted_ids.contains_key(&child));
                }
                // ID was deleted too was deleted so mark as deleted
                Op::Delete if self.deleted_ids.contains_key(&child) => {
                    debug_assert!(!self.transfers.contains_key(&child));
                }
                // was new so the object is transient and does not need to be marked as deleted
                Op::Delete if self.new_ids.contains_key(&child) => {}
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
            .filter_map(|(id, (_by_value, owner))| match owner {
                Owner::AddressOwner(_) | Owner::Shared { .. } | Owner::Immutable => None,
                Owner::ObjectOwner(parent) => Some((*id, (*parent).into())),
            })
            .collect();
        // update the input owners with the new owners from transfers
        // reports an error on cycles
        // TODO can we have cycles in the new system?
        update_owner_map(
            input_owner_map,
            transfers.iter().map(|(id, (owner, _, _, _))| (*id, *owner)),
        )?;
        // determine write kinds
        let writes: LinkedHashMap<_, _> = transfers
            .into_iter()
            .map(|(id, (owner, type_, tag, value))| {
                let write_kind =
                    if input_objects.contains_key(&id) || loaded_child_objects.contains_key(&id) {
                        debug_assert!(!new_ids.contains_key(&id));
                        WriteKind::Mutate
                    } else if id == SUI_SYSTEM_STATE_OBJECT_ID || new_ids.contains_key(&id) {
                        WriteKind::Create
                    } else {
                        WriteKind::Unwrap
                    };
                (id, (write_kind, owner, type_, tag, value))
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
        let remaining_by_value_objects = input_objects
            .into_iter()
            .filter(|(id, (by_value, _))| {
                *by_value && !writes.contains_key(id) && !deletions.contains_key(id)
            })
            .map(|(id, _)| id)
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
                        return Err(ExecutionErrorKind::circular_object_ownership(cur).into());
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
