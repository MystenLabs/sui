// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::{adapter, genesis};
use fastx_types::{base_types, storage::Storage};
use move_core_types::account_address::AccountAddress;
use std::mem;

use super::*;

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
            match o.data.as_module() {
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

/// Exercise test functions that create, transfer, read, update, and delete objects
#[test]
fn test_object_basics() {
    let addr1 = base_types::get_key_pair().0;
    let addr2 = base_types::get_key_pair().0;

    let genesis = genesis::GENESIS.lock().unwrap();
    let native_functions = genesis.native_functions.clone();
    let mut storage = InMemoryStorage::new(genesis.objects.clone());

    #[allow(clippy::ptr_arg)]
    fn call(
        storage: &mut InMemoryStorage,
        native_functions: &NativeFunctionTable,
        name: &str,
        object_args: Vec<Object>,
        pure_args: Vec<Vec<u8>>,
    ) {
        let gas_budget = None;
        let module = storage.find_module("ObjectBasics").unwrap();

        adapter::execute(
            storage,
            native_functions.to_owned(),
            module,
            &Identifier::new(name).unwrap(),
            Vec::new(),
            object_args,
            pure_args,
            gas_budget,
            TxContext::random(),
        )
        .unwrap();
    }

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
        Vec::new(),
        pure_args,
    );

    let created = storage.created();
    assert_eq!(created.len(), 1);
    assert!(storage.updated().is_empty());
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
        vec![obj1.clone()],
        pure_args,
    );

    let updated = storage.updated();
    assert_eq!(updated.len(), 1);
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
        Vec::new(),
        pure_args,
    );
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
        vec![obj1.clone(), obj2],
        Vec::new(),
    );
    let updated = storage.updated();
    assert_eq!(updated.len(), 1);
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
        vec![obj1],
        Vec::new(),
    );
    let deleted = storage.deleted();
    assert_eq!(deleted.len(), 1);
    assert!(storage.created().is_empty());
    assert!(storage.updated().is_empty());
    storage.flush();
    assert!(storage.read_object(&id1).is_none())
}

// TODO(https://github.com/MystenLabs/fastnft/issues/92): tests that exercise all the error codes of the adapter
