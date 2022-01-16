// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::{adapter, genesis};
use fastx_types::{
    base_types::{self, SequenceNumber},
    error::FastPayResult,
    storage::Storage,
};
use move_binary_format::file_format;
use move_core_types::account_address::AccountAddress;
use std::mem;

use super::*;

const MAX_GAS: u64 = 100000;

// temporary store where writes buffer before they get committed
#[derive(Default, Debug)]
struct ScratchPad {
    updated: BTreeMap<ObjectID, Object>,
    created: BTreeMap<ObjectID, Object>,
    deleted: Vec<ObjectID>,
}
#[derive(Default, Debug)]
struct InMemoryStorage {
    persistent: BTreeMap<ObjectID, Object>,
    temporary: ScratchPad,
}

impl InMemoryStorage {
    pub fn new(objects: Vec<Object>) -> Self {
        let mut persistent = BTreeMap::new();
        for o in objects {
            persistent.insert(o.id(), o);
        }
        Self {
            persistent,
            temporary: ScratchPad::default(),
        }
    }

    /// Return the package that contains the module `name` (if any)
    pub fn find_package(&self, name: &str) -> Option<Object> {
        self.persistent
            .values()
            .find(|o| {
                if let Some(package) = o.data.try_as_package() {
                    if package.get(name).is_some() {
                        return true;
                    }
                }
                false
            })
            .cloned()
    }

    /// Flush writes in scratchpad to persistent storage
    pub fn flush(&mut self) {
        let to_flush = mem::take(&mut self.temporary);
        for (id, o) in to_flush.created {
            assert!(self.persistent.insert(id, o).is_none())
        }
        for (id, o) in to_flush.updated {
            assert!(self.persistent.insert(id, o).is_some())
        }
        for id in to_flush.deleted {
            self.persistent.remove(&id);
        }
    }

    pub fn created(&self) -> &BTreeMap<ObjectID, Object> {
        &self.temporary.created
    }

    pub fn updated(&self) -> &BTreeMap<ObjectID, Object> {
        &self.temporary.updated
    }

    pub fn deleted(&self) -> &[ObjectID] {
        &self.temporary.deleted
    }

    pub fn get_created_keys(&self) -> Vec<ObjectID> {
        self.temporary.created.keys().cloned().collect()
    }
}

impl Storage for InMemoryStorage {
    fn read_object(&self, id: &ObjectID) -> Option<Object> {
        self.persistent.get(id).cloned()
    }

    // buffer write to appropriate place in temporary storage
    fn write_object(&mut self, object: Object) {
        let id = object.id();
        if self.persistent.contains_key(&id) {
            self.temporary.updated.insert(id, object);
        } else {
            self.temporary.created.insert(id, object);
        }
    }

    // buffer delete
    fn delete_object(&mut self, id: &ObjectID) {
        self.temporary.deleted.push(*id)
    }
}

impl ModuleResolver for InMemoryStorage {
    type Error = ();

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self
            .read_object(module_id.address())
            .map(|o| match &o.data {
                Data::Package(m) => m[module_id.name().as_str()].clone(),
                Data::Move(_) => panic!("Type error"),
            }))
    }
}

impl ResourceResolver for InMemoryStorage {
    type Error = ();

    fn get_resource(
        &self,
        _address: &AccountAddress,
        _struct_tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        unreachable!("Should never be called in FastX")
    }
}

fn call(
    storage: &mut InMemoryStorage,
    native_functions: &NativeFunctionTable,
    name: &str,
    gas_object: Object,
    gas_budget: u64,
    object_args: Vec<Object>,
    pure_args: Vec<Vec<u8>>,
) -> FastPayResult {
    let package = storage.find_package("ObjectBasics").unwrap();

    let vm = adapter::new_move_vm(native_functions.clone()).expect("No errors");
    adapter::execute(
        &vm,
        storage,
        native_functions.clone(),
        package,
        &Identifier::new("ObjectBasics").unwrap(),
        &Identifier::new(name).unwrap(),
        Vec::new(),
        object_args,
        pure_args,
        gas_budget,
        gas_object,
        TxContext::random(),
    )
}

/// Exercise test functions that create, transfer, read, update, and delete objects
#[test]
fn test_object_basics() {
    let addr1 = base_types::get_key_pair().0;
    let addr2 = base_types::get_key_pair().0;

    let (genesis_objects, native_functions) = genesis::clone_genesis_data();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object = Object::with_id_owner_for_testing(
        ObjectID::random(),
        base_types::FastPayAddress::default(),
    );
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr1
    // ObjectBasics::create expects integer value and recipient address
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&addr1.to_vec()).unwrap(),
    ];
    call(
        &mut storage,
        &native_functions,
        "create",
        gas_object.clone(),
        MAX_GAS,
        Vec::new(),
        pure_args,
    )
    .unwrap();

    let created = storage.created();
    assert_eq!(created.len(), 1);
    assert_eq!(storage.updated().len(), 1); // The gas object
    assert!(storage.deleted().is_empty());
    let id1 = storage.get_created_keys().pop().unwrap();
    storage.flush();
    let mut obj1 = storage.read_object(&id1).unwrap();
    let mut obj1_seq = SequenceNumber::from(1);
    assert_eq!(obj1.owner, addr1);
    assert_eq!(obj1.version(), obj1_seq);

    // 2. Transfer obj1 to addr2
    let pure_args = vec![bcs::to_bytes(&addr2.to_vec()).unwrap()];
    call(
        &mut storage,
        &native_functions,
        "transfer",
        gas_object.clone(),
        MAX_GAS,
        vec![obj1.clone()],
        pure_args,
    )
    .unwrap();

    let updated = storage.updated();
    assert_eq!(updated.len(), 2);
    assert!(storage.created().is_empty());
    assert!(storage.deleted().is_empty());
    storage.flush();
    let transferred_obj = storage.read_object(&id1).unwrap();
    assert_eq!(transferred_obj.owner, addr2);
    obj1_seq = obj1_seq.increment().unwrap();
    assert_eq!(obj1.id(), transferred_obj.id());
    assert_eq!(transferred_obj.version(), obj1_seq);
    assert_eq!(
        obj1.data.try_as_move().unwrap().type_specific_contents(),
        transferred_obj
            .data
            .try_as_move()
            .unwrap()
            .type_specific_contents()
    );
    obj1 = transferred_obj;

    // 3. Create another object obj2 owned by addr2, use it to update addr1
    let pure_args = vec![
        20u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&addr2.to_vec()).unwrap(),
    ];
    call(
        &mut storage,
        &native_functions,
        "create",
        gas_object.clone(),
        MAX_GAS,
        Vec::new(),
        pure_args,
    )
    .unwrap();
    let obj2 = storage
        .created()
        .values()
        .cloned()
        .collect::<Vec<Object>>()
        .pop()
        .unwrap();
    storage.flush();

    call(
        &mut storage,
        &native_functions,
        "update",
        gas_object.clone(),
        MAX_GAS,
        vec![obj1.clone(), obj2],
        Vec::new(),
    )
    .unwrap();
    let updated = storage.updated();
    assert_eq!(updated.len(), 2);
    assert!(storage.created().is_empty());
    assert!(storage.deleted().is_empty());
    storage.flush();
    let updated_obj = storage.read_object(&id1).unwrap();
    assert_eq!(updated_obj.owner, addr2);
    obj1_seq = obj1_seq.increment().unwrap();
    assert_eq!(updated_obj.version(), obj1_seq);
    assert_ne!(
        obj1.data.try_as_move().unwrap().type_specific_contents(),
        updated_obj
            .data
            .try_as_move()
            .unwrap()
            .type_specific_contents()
    );
    obj1 = updated_obj;

    // 4. Delete obj1
    call(
        &mut storage,
        &native_functions,
        "delete",
        gas_object,
        MAX_GAS,
        vec![obj1],
        Vec::new(),
    )
    .unwrap();
    let deleted = storage.deleted();
    assert_eq!(deleted.len(), 1);
    assert!(storage.created().is_empty());
    assert_eq!(storage.updated().len(), 1);
    storage.flush();
    assert!(storage.read_object(&id1).is_none())
}

/// Exercise test functions that wrap and object and subsequently unwrap it
/// Ensure that the object's version is consistent
#[test]
fn test_wrap_unwrap() {
    let addr = base_types::FastPayAddress::default();

    let (genesis_objects, native_functions) = genesis::clone_genesis_data();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment. Note that we won't really use it because we won't be providing a gas budget.
    let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), addr);
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&addr.to_vec()).unwrap(),
    ];
    call(
        &mut storage,
        &native_functions,
        "create",
        gas_object.clone(),
        MAX_GAS,
        Vec::new(),
        pure_args,
    )
    .unwrap();
    let id1 = storage.get_created_keys().pop().unwrap();
    storage.flush();
    let obj1 = storage.read_object(&id1).unwrap();
    let obj1_version = obj1.version();
    let obj1_contents = obj1
        .data
        .try_as_move()
        .unwrap()
        .type_specific_contents()
        .to_vec();
    assert_eq!(obj1.version(), SequenceNumber::from(1));

    // 2. wrap addr
    call(
        &mut storage,
        &native_functions,
        "wrap",
        gas_object.clone(),
        MAX_GAS,
        vec![obj1],
        Vec::new(),
    )
    .unwrap();
    // wrapping should create wrapper object and "delete" wrapped object
    assert_eq!(storage.created().len(), 1);
    assert_eq!(storage.deleted().len(), 1);
    assert_eq!(storage.deleted()[0].clone(), id1);
    let id2 = storage.get_created_keys().pop().unwrap();
    storage.flush();
    assert!(storage.read_object(&id1).is_none());
    let obj2 = storage.read_object(&id2).unwrap();

    // 3. unwrap addr
    call(
        &mut storage,
        &native_functions,
        "unwrap",
        gas_object,
        MAX_GAS,
        vec![obj2],
        Vec::new(),
    )
    .unwrap();
    // wrapping should delete wrapped object and "create" unwrapped object
    assert_eq!(storage.created().len(), 1);
    assert_eq!(storage.deleted().len(), 1);
    assert_eq!(storage.deleted()[0].clone(), id2);
    assert_eq!(id1, storage.get_created_keys().pop().unwrap());
    storage.flush();
    assert!(storage.read_object(&id2).is_none());
    let new_obj1 = storage.read_object(&id1).unwrap();
    // sequence # should increase after unwrapping
    assert_eq!(new_obj1.version(), obj1_version.increment().unwrap());
    // type-specific contents should not change after unwrapping
    assert_eq!(
        new_obj1
            .data
            .try_as_move()
            .unwrap()
            .type_specific_contents(),
        &obj1_contents
    );
}

#[test]
fn test_move_call_insufficient_gas() {
    let (genesis_objects, native_functions) = genesis::clone_genesis_data();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object = Object::with_id_owner_for_testing(
        ObjectID::random(),
        base_types::FastPayAddress::default(),
    );
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr1
    // ObjectBasics::create expects integer value and recipient address
    let addr1 = base_types::get_key_pair().0;
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&addr1.to_vec()).unwrap(),
    ];
    let response = call(
        &mut storage,
        &native_functions,
        "create",
        gas_object,
        20, // This budget is not enough to execute all bytecode.
        Vec::new(),
        pure_args,
    );
    assert!(response
        .unwrap_err()
        .to_string()
        .contains("VMError with status OUT_OF_GAS"));
}

#[test]
fn test_publish_module_insufficient_gas() {
    let (genesis_objects, _) = genesis::clone_genesis_data();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object = Object::with_id_owner_gas_for_testing(
        ObjectID::random(),
        SequenceNumber::from(1),
        base_types::FastPayAddress::default(),
        30,
    );
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create a module.
    let module = file_format::empty_module();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];

    let mut tx_context = TxContext::random();
    let response = adapter::publish(
        &mut storage,
        module_bytes,
        base_types::FastPayAddress::default(),
        &mut tx_context,
        gas_object,
    );
    assert!(response
        .unwrap_err()
        .to_string()
        .contains("Gas balance is 30, not enough to pay 58"));
}

#[test]
fn test_transfer_and_freeze() {
    let addr1 = base_types::get_key_pair().0;
    let addr2 = base_types::get_key_pair().0;

    let (genesis_objects, native_functions) = genesis::clone_genesis_data();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object = Object::with_id_owner_for_testing(
        ObjectID::random(),
        base_types::FastPayAddress::default(),
    );
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr1
    // ObjectBasics::create expects integer value and recipient address
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&addr1.to_vec()).unwrap(),
    ];
    call(
        &mut storage,
        &native_functions,
        "create",
        gas_object.clone(),
        MAX_GAS,
        Vec::new(),
        pure_args,
    )
    .unwrap();

    let created = storage.created();
    let id1 = created
        .keys()
        .cloned()
        .collect::<Vec<ObjectID>>()
        .pop()
        .unwrap();
    storage.flush();
    let obj1 = storage.read_object(&id1).unwrap();
    assert!(!obj1.is_read_only());

    // 2. Call transfer_and_freeze.
    let pure_args = vec![bcs::to_bytes(&addr2.to_vec()).unwrap()];
    call(
        &mut storage,
        &native_functions,
        "transfer_and_freeze",
        gas_object.clone(),
        MAX_GAS,
        vec![obj1],
        pure_args,
    )
    .unwrap();
    assert_eq!(storage.updated().len(), 2);
    storage.flush();
    let obj1 = storage.read_object(&id1).unwrap();
    assert!(obj1.is_read_only());
    assert_eq!(obj1.owner, addr2);

    // 3. Call transfer again and it should fail.
    let pure_args = vec![bcs::to_bytes(&addr1.to_vec()).unwrap()];
    let result = call(
        &mut storage,
        &native_functions,
        "transfer",
        gas_object.clone(),
        MAX_GAS,
        vec![obj1],
        pure_args,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Argument 0 is expected to be mutable, immutable object found"));

    // 4. Call set_value (pass as mutable reference) should fail as well.
    let obj1 = storage.read_object(&id1).unwrap();
    let pure_args = vec![bcs::to_bytes(&1u64).unwrap()];
    let result = call(
        &mut storage,
        &native_functions,
        "set_value",
        gas_object,
        MAX_GAS,
        vec![obj1],
        pure_args,
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Argument 0 is expected to be mutable, immutable object found"));
}

// TODO(https://github.com/MystenLabs/fastnft/issues/92): tests that exercise all the error codes of the adapter
