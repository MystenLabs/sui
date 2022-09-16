// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use better_any::{Tid, TidAble};
use linked_hash_map::LinkedHashMap;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, language_storage::StructTag};
use move_vm_types::{loaded_data::runtime_types::Type, values::Value};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    error::{ExecutionError, ExecutionErrorKind},
    object::{MoveObject, Owner},
    storage::{DeleteKind, ObjectResolver, WriteKind},
    SUI_SYSTEM_STATE_OBJECT_ID,
};

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
    // object has been taken from the inventory
    pub(crate) taken: BTreeMap<ObjectID, Owner>,
}
pub struct RuntimeResults {
    pub writes: LinkedHashMap<ObjectID, (WriteKind, Owner, Type, StructTag, Value)>,
    pub deletions: LinkedHashMap<ObjectID, DeleteKind>,
    pub user_events: Vec<(Type, StructTag, Value)>,
}

#[derive(Default)]
pub(crate) struct ObjectRuntimeState {
    // will eventually need a reference to the state view to access child objects
    // pub(crate) state_view: &'a mut dyn ____,
    pub(crate) input_objects: BTreeMap<ObjectID, (/* by_value */ bool, Owner)>,
    // new ids from object::new
    new_ids: Set<ObjectID>,
    // ids passed to object::delete
    deleted_ids: Set<ObjectID>,
    // transfers to a new owner (shared, immutable, object, or account address)
    // TODO these struct tags can be removed if type_to_type_tag was exposed in the session
    pub(crate) transfers: Vec<(ObjectID, Owner, Type, StructTag, Value)>,
    events: Vec<(Type, StructTag, Value)>,
}

#[derive(Tid)]
pub struct ObjectRuntime<'a> {
    // eventually used to load dynamic child objects
    _object_resolver: Box<dyn ObjectResolver + 'a>,
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
        _object_resolver: Box<dyn ObjectResolver + 'a>,
        input_objects: BTreeMap<ObjectID, (/* by_value */ bool, Owner)>,
    ) -> Self {
        Self {
            _object_resolver,
            test_inventories: TestInventories::new(),
            state: ObjectRuntimeState {
                input_objects,
                new_ids: Set::new(),
                deleted_ids: Set::new(),
                transfers: vec![],
                events: vec![],
            },
        }
    }

    pub fn new_id(&mut self, id: ObjectID) {
        self.state.new_ids.insert(id, ());
    }

    pub fn delete_id(&mut self, id: ObjectID) {
        let was_new = self.state.new_ids.remove(&id).is_some();
        // testing cleanup if it is an address owned or object owned value
        if !self.test_inventories.taken.is_empty() {
            let prev_owner = self.test_inventories.taken.get(&id);
            let is_address_or_object_owned = matches!(
                prev_owner,
                Some(Owner::AddressOwner(_) | Owner::ObjectOwner(_))
            );
            if is_address_or_object_owned {
                self.test_inventories.taken.remove(&id);
            }
        }
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
        // testing cleanup if it is an address owned or object owned value
        // or if it is a shared/imm object being returned
        if !self.test_inventories.taken.is_empty() {
            let prev_owner = self.test_inventories.taken.get(&id);
            let is_address_or_object_owned = matches!(
                prev_owner,
                Some(Owner::AddressOwner(_) | Owner::ObjectOwner(_))
            );
            let is_shared_or_imm_returned = matches!(prev_owner ,
                    Some(a @ (Owner::Shared | Owner::Immutable)) if a == &owner);
            if is_address_or_object_owned || is_shared_or_imm_returned {
                self.test_inventories.taken.remove(&id);
            }
        }
        self.state.transfers.push((id, owner, ty, tag, obj));
        Ok(())
    }

    pub fn emit_event(&mut self, ty: Type, tag: StructTag, event: Value) {
        self.state.events.push((ty, tag, event))
    }

    pub(crate) fn take_state(&mut self) -> ObjectRuntimeState {
        std::mem::take(&mut self.state)
    }

    pub fn finish(self) -> Result<RuntimeResults, ExecutionError> {
        self.state.finish()
    }
}

impl ObjectRuntimeState {
    /// Update `state_view` with the effects of successfully executing a transaction:
    /// - Process `transfers` and `input_objects` to determine whether the type of change
    ///   (WriteKind) to the object
    /// - Process `deleted_ids` with previously determiend information to determine the
    ///   DeleteKind
    /// - Passes through user events
    pub fn finish(self) -> Result<RuntimeResults, ExecutionError> {
        let ObjectRuntimeState {
            input_objects,
            new_ids,
            deleted_ids,
            transfers,
            events: user_events,
        } = self;
        let owner_map = input_objects
            .iter()
            .filter_map(|(id, (_by_value, owner))| match owner {
                Owner::AddressOwner(_) | Owner::Shared | Owner::Immutable => None,
                Owner::ObjectOwner(parent) => Some((*id, (*parent).into())),
            })
            .collect();
        check_for_owner_cycles(
            owner_map,
            transfers.iter().map(|(id, owner, _, _, _)| (*id, *owner)),
        )?;
        let writes: LinkedHashMap<_, _> = transfers
            .into_iter()
            .map(|(id, owner, type_, tag, value)| {
                let write_kind = if input_objects.contains_key(&id) {
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
        let mut deletions: LinkedHashMap<_, _> = deleted_ids
            .into_iter()
            .map(|(id, ())| {
                debug_assert!(!new_ids.contains_key(&id));
                let delete_kind = if input_objects.contains_key(&id) {
                    DeleteKind::Normal
                } else {
                    DeleteKind::UnwrapThenDelete
                };
                (id, delete_kind)
            })
            .collect();
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
        debug_assert!(writes.keys().all(|id| !deletions.contains_key(id)));
        debug_assert!(deletions.keys().all(|id| !writes.contains_key(id)));
        Ok(RuntimeResults {
            writes,
            deletions,
            user_events,
        })
    }
}

fn check_for_owner_cycles(
    mut object_owner_map: BTreeMap<ObjectID, ObjectID>,
    transfers: impl IntoIterator<Item = (ObjectID, Owner)>,
) -> Result<(), ExecutionError> {
    for (id, recipient) in transfers {
        object_owner_map.remove(&id);
        match recipient {
            Owner::AddressOwner(_) | Owner::Shared | Owner::Immutable => (),
            Owner::ObjectOwner(new_owner) => {
                let new_owner: ObjectID = new_owner.into();
                let mut parent = new_owner;
                while parent != id && object_owner_map.contains_key(&parent) {
                    parent = *object_owner_map.get(&parent).unwrap();
                }
                if parent == id {
                    return Err(ExecutionErrorKind::circular_object_ownership(parent).into());
                }
                object_owner_map.insert(id, new_owner);
            }
        }
    }
    Ok(())
}
