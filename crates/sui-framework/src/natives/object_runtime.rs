// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use better_any::{Tid, TidAble};
use linked_hash_map::LinkedHashMap;
use move_core_types::{account_address::AccountAddress, language_storage::StructTag};
use move_vm_types::values::Value;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    error::ExecutionError,
    object::{MoveObject, Owner},
    storage::ObjectChange,
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
    pub(crate) address_inventories: BTreeMap<SuiAddress, BTreeMap<StructTag, Set<ObjectID>>>,
    // global inventories.Most recent objects are at the back of the set
    pub(crate) shared_inventory: BTreeMap<StructTag, Set<ObjectID>>,
    pub(crate) immutable_inventory: BTreeMap<StructTag, Set<ObjectID>>,
    // object has been taken from the inventory
    pub(crate) taken: BTreeMap<ObjectID, Owner>,
}
pub struct RuntimeResults {
    pub changes: LinkedHashMap<ObjectID, ObjectChange>,
    pub user_events: Vec<(StructTag, Vec<u8>)>,
}

#[derive(Tid, Default)]
pub struct ObjectRuntime {
    pub(crate) test_inventories: TestInventories,
    // will eventually need a reference to the state view to access child objects
    // pub(crate) state_view: &'a mut dyn ____,
    pub(crate) input_objects: BTreeSet<ObjectID>,
    // new ids from object::new
    new_ids: Set<ObjectID>,
    // ids passed to object::delete
    deleted_ids: Set<ObjectID>,
    // transfers to a new owner (shared, immutable, object, or account address)
    pub(crate) transfers: Vec<(Owner, StructTag, Value)>,
    pub(crate) events: Vec<(StructTag, Value)>,
}

impl TestInventories {
    fn new() -> Self {
        Self::default()
    }
}

impl ObjectRuntime {
    pub fn new(input_objects: BTreeSet<ObjectID>) -> Self {
        Self {
            test_inventories: TestInventories::new(),
            input_objects,
            new_ids: Set::new(),
            deleted_ids: Set::new(),
            transfers: vec![],
            events: vec![],
        }
    }

    pub fn new_id(&mut self, id: ObjectID) {
        self.new_ids.insert(id, ());
    }

    pub fn delete_id(&mut self, id: ObjectID) {
        let was_new = self.new_ids.remove(&id).is_some();
        // testing cleanup if it is an address owned or object owned value
        if cfg!(feature = "testing") {
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
            self.deleted_ids.insert(id, ());
        }
    }

    pub fn transfer(&mut self, owner: Owner, ty: StructTag, obj: Value) {
        // testing cleanup if it is an address owned or object owned value
        // or if it is a shared/imm object being returned
        if cfg!(feature = "testing") {
            let id: ObjectID = get_object_id(obj.copy_value().unwrap())
                .unwrap()
                .value_as::<AccountAddress>()
                .unwrap()
                .into();
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
        self.transfers.push((owner, ty, obj))
    }

    pub fn emit_event(&mut self, ty: StructTag, event: Value) {
        self.events.push((ty, event))
    }

    pub(crate) fn take(&mut self) -> Self {
        // take fields for empty version
        let test_inventories = std::mem::take(&mut self.test_inventories);
        let input_objects = std::mem::take(&mut self.input_objects);
        let taken = std::mem::take(self);
        // restore fields
        self.test_inventories = test_inventories;
        self.input_objects = input_objects;
        taken
    }

    pub fn finish(self) -> Result<RuntimeResults, ExecutionError> {
        // TODO bring in the adapter rules here for deleting children
        todo!()
    }
}
