// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::{adapter, genesis};
use fastx_types::{base_types, error::FastPayResult, storage::Storage};
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

    /// Return the object wrapping the module `name` (if any)
    pub fn find_module(&self, name: &str) -> Option<Object> {
        let id = Identifier::new(name).unwrap();
        for o in self.persistent.values() {
            match o.data.try_as_module() {
                Some(m) if m.self_id().name() == id.as_ident_str() => return Some(o.clone()),
                _ => (),
            }
        }
        None
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
                Data::Module(m) => m.clone(),
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
    let module = storage.find_module("ObjectBasics").unwrap();

    adapter::execute(
        storage,
        native_functions.clone(),
        module,
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

    let genesis = genesis::GENESIS.lock().unwrap();
    let native_functions = genesis.native_functions.clone();
    let mut storage = InMemoryStorage::new(genesis.objects.clone());

    // 0. Create a gas object for gas payment. Note that we won't really use it because we won't be providing a gas budget.
    let gas_object = Object::with_id_owner_gas_for_testing(
        ObjectID::random(),
        base_types::FastPayAddress::default(),
        MAX_GAS,
    );
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr1
    // ObjectBasics::create expects integer value and recipient address
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        //bcs::to_bytes(&old_addr1.to_vec()).unwrap(),
        bcs::to_bytes(&addr1.to_vec()).unwrap(),
        //MoveValue::vector_u8(addr1.to_vec()).simple_serialize().unwrap(),
        //        transaction_argument::convert_txn_args(&vec![TransactionArgument::U8Vector(addr1.to_vec())]).pop().unwrap(),
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
    let id1 = created
        .keys()
        .cloned()
        .collect::<Vec<ObjectID>>()
        .pop()
        .unwrap();
    storage.flush();
    let mut obj1 = storage.read_object(&id1).unwrap();
    let mut obj1_seq = SequenceNumber::new();
    assert_eq!(obj1.owner, addr1);
    assert_eq!(obj1.next_sequence_number, obj1_seq);

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
    assert_eq!(transferred_obj.next_sequence_number, obj1_seq);
    assert_eq!(obj1.data, transferred_obj.data);
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
    assert_eq!(updated_obj.next_sequence_number, obj1_seq);
    assert_ne!(obj1.data, updated_obj.data);
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

#[test]
fn test_move_call_insufficient_gas() {
    let genesis = genesis::GENESIS.lock().unwrap();
    let native_functions = genesis.native_functions.clone();
    let mut storage = InMemoryStorage::new(genesis.objects.clone());

    // 0. Create a gas object for gas payment.
    let gas_object = Object::with_id_owner_gas_for_testing(
        ObjectID::random(),
        base_types::FastPayAddress::default(),
        MAX_GAS,
    );
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr1
    // ObjectBasics::create expects integer value and recipient address
    let addr1 = base_types::get_key_pair().0;
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        //bcs::to_bytes(&old_addr1.to_vec()).unwrap(),
        bcs::to_bytes(&addr1.to_vec()).unwrap(),
        //MoveValue::vector_u8(addr1.to_vec()).simple_serialize().unwrap(),
        //        transaction_argument::convert_txn_args(&vec![TransactionArgument::U8Vector(addr1.to_vec())]).pop().unwrap(),
    ];
    let response = call(
        &mut storage,
        &native_functions,
        "create",
        gas_object,
        50, // This budget is not enough to execute all bytecode.
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
    let genesis = genesis::GENESIS.lock().unwrap();
    let mut storage = InMemoryStorage::new(genesis.objects.clone());

    // 0. Create a gas object for gas payment.
    let gas_object = Object::with_id_owner_gas_for_testing(
        ObjectID::random(),
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

// TODO(https://github.com/MystenLabs/fastnft/issues/92): tests that exercise all the error codes of the adapter
