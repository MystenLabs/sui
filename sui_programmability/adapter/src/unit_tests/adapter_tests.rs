// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{adapter, genesis};
use move_binary_format::file_format::{
    self, AbilitySet, AddressIdentifierIndex, IdentifierIndex, ModuleHandle, ModuleHandleIndex,
    StructHandle,
};
use move_core_types::{account_address::AccountAddress, ident_str, language_storage::StructTag};
use move_package::BuildConfig;
use std::{mem, path::PathBuf};
use sui_types::{
    base_types::{self, SequenceNumber},
    crypto::get_key_pair,
    error::SuiResult,
    gas_coin::GAS,
    object::{Data, Owner},
    storage::Storage,
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};

use super::*;

const GAS_BUDGET: u64 = 10000;

// temporary store where writes buffer before they get committed
#[derive(Default, Debug)]
struct ScratchPad {
    updated: BTreeMap<ObjectID, Object>,
    created: BTreeMap<ObjectID, Object>,
    deleted: BTreeMap<ObjectID, (SequenceNumber, DeleteKind)>,
    events: Vec<Event>,
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
                    if package.serialized_module_map().get(name).is_some() {
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
        for (id, _) in to_flush.deleted {
            self.persistent.remove(&id);
        }
    }

    pub fn created(&self) -> &BTreeMap<ObjectID, Object> {
        &self.temporary.created
    }

    pub fn updated(&self) -> &BTreeMap<ObjectID, Object> {
        &self.temporary.updated
    }

    pub fn deleted(&self) -> &BTreeMap<ObjectID, (SequenceNumber, DeleteKind)> {
        &self.temporary.deleted
    }

    pub fn events(&self) -> &[Event] {
        &self.temporary.events
    }

    pub fn get_created_keys(&self) -> Vec<ObjectID> {
        self.temporary.created.keys().cloned().collect()
    }
}

impl Storage for InMemoryStorage {
    fn reset(&mut self) {
        self.temporary = ScratchPad::default();
    }

    fn read_object(&self, id: &ObjectID) -> Option<Object> {
        // there should be no read after delete
        assert!(!self.temporary.deleted.contains_key(id));
        // try objects updated in temp memory first
        self.temporary.updated.get(id).cloned().or_else(|| {
            self.temporary.created.get(id).cloned().or_else(||
                // try persistent memory
                 self.persistent.get(id).cloned())
        })
    }

    // buffer write to appropriate place in temporary storage
    fn write_object(&mut self, object: Object) {
        let id = object.id();
        // there should be no write after delete
        assert!(!self.temporary.deleted.contains_key(&id));
        if self.persistent.contains_key(&id) {
            self.temporary.updated.insert(id, object);
        } else {
            self.temporary.created.insert(id, object);
        }
    }

    fn log_event(&mut self, event: Event) {
        self.temporary.events.push(event)
    }

    // buffer delete
    fn delete_object(&mut self, id: &ObjectID, version: SequenceNumber, kind: DeleteKind) {
        // there should be no deletion after write
        assert!(self.temporary.updated.get(id) == None);
        let old_entry = self.temporary.deleted.insert(*id, (version, kind));
        // this object was not previously deleted
        assert!(old_entry.is_none());
    }
}

impl ModuleResolver for InMemoryStorage {
    type Error = ();
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self
            .read_object(&ObjectID::from(*module_id.address()))
            .map(|o| match &o.data {
                Data::Package(m) => m.serialized_module_map()[module_id.name().as_str()]
                    .clone()
                    .into_vec(),
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
        unreachable!("Should never be called in Sui")
    }
}

#[allow(clippy::too_many_arguments)]
fn call(
    storage: &mut InMemoryStorage,
    native_functions: &NativeFunctionTable,
    module_name: &str,
    fun_name: &str,
    gas_object: Object,
    gas_budget: u64,
    type_args: Vec<TypeTag>,
    object_args: Vec<Object>,
    pure_args: Vec<Vec<u8>>,
) -> SuiResult<ExecutionStatus> {
    let package = storage.find_package(module_name).unwrap();

    let vm = adapter::new_move_vm(native_functions.clone()).expect("No errors");
    adapter::execute(
        &vm,
        storage,
        native_functions.clone(),
        package,
        &Identifier::new(module_name).unwrap(),
        &Identifier::new(fun_name).unwrap(),
        type_args,
        object_args,
        pure_args,
        gas_budget,
        gas_object,
        &mut TxContext::random_for_testing_only(),
    )
}

/// Exercise test functions that create, transfer, read, update, and delete objects
#[test]
fn test_object_basics() {
    let addr1 = base_types::get_new_address();
    let addr2 = base_types::get_new_address();

    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr1
    // ObjectBasics::create expects integer value and recipient address
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&AccountAddress::from(addr1)).unwrap(),
    ];
    call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "create",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        Vec::new(),
        pure_args,
    )
    .unwrap()
    .unwrap();

    assert_eq!(storage.created().len(), 1);
    assert_eq!(storage.updated().len(), 1); // The gas object
    assert!(storage.deleted().is_empty());
    let id1 = storage.get_created_keys().pop().unwrap();
    storage.flush();
    let mut obj1 = storage.read_object(&id1).unwrap();
    let mut obj1_seq = SequenceNumber::from(1);
    assert!(obj1.owner == addr1);
    assert_eq!(obj1.version(), obj1_seq);

    // 2. Transfer obj1 to addr2
    let pure_args = vec![bcs::to_bytes(&AccountAddress::from(addr2)).unwrap()];
    call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "transfer",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        vec![obj1.clone()],
        pure_args,
    )
    .unwrap()
    .unwrap();

    assert_eq!(storage.updated().len(), 2);
    assert!(storage.created().is_empty());
    assert!(storage.deleted().is_empty());
    storage.flush();
    let transferred_obj = storage.read_object(&id1).unwrap();
    assert!(transferred_obj.owner == addr2);
    obj1_seq = obj1_seq.increment();
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
        bcs::to_bytes(&AccountAddress::from(addr2)).unwrap(),
    ];
    call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "create",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        Vec::new(),
        pure_args,
    )
    .unwrap()
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
        "ObjectBasics",
        "update",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        vec![obj1.clone(), obj2],
        Vec::new(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(storage.updated().len(), 2);
    assert!(storage.created().is_empty());
    assert!(storage.deleted().is_empty());
    // test than an event was emitted as expected
    assert_eq!(storage.events().len(), 1);
    assert_eq!(
        storage.events()[0].clone().type_.name.to_string(),
        "NewValueEvent"
    );
    storage.flush();
    let updated_obj = storage.read_object(&id1).unwrap();
    assert!(updated_obj.owner == addr2);
    obj1_seq = obj1_seq.increment();
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
        "ObjectBasics",
        "delete",
        gas_object,
        GAS_BUDGET,
        Vec::new(),
        vec![obj1],
        Vec::new(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(storage.deleted().len(), 1);
    assert!(storage.created().is_empty());
    assert_eq!(storage.updated().len(), 1);
    storage.flush();
    assert!(storage.read_object(&id1).is_none())
}

/// Exercise test functions that wrap and object and subsequently unwrap it
/// Ensure that the object's version is consistent
#[test]
fn test_wrap_unwrap() {
    let addr = base_types::SuiAddress::default();

    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment. Note that we won't really use it because we won't be providing a gas budget.
    let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), addr);
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&AccountAddress::from(addr)).unwrap(),
    ];
    call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "create",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        Vec::new(),
        pure_args,
    )
    .unwrap()
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
        "ObjectBasics",
        "wrap",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        vec![obj1],
        Vec::new(),
    )
    .unwrap()
    .unwrap();
    // wrapping should create wrapper object and "delete" wrapped object
    assert_eq!(storage.created().len(), 1);
    assert_eq!(storage.deleted().len(), 1);
    assert_eq!(storage.deleted().iter().next().unwrap().0, &id1);
    let id2 = storage.get_created_keys().pop().unwrap();
    storage.flush();
    assert!(storage.read_object(&id1).is_none());
    let obj2 = storage.read_object(&id2).unwrap();

    // 3. unwrap addr
    call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "unwrap",
        gas_object,
        GAS_BUDGET,
        Vec::new(),
        vec![obj2],
        Vec::new(),
    )
    .unwrap()
    .unwrap();
    // wrapping should delete wrapped object and "create" unwrapped object
    assert_eq!(storage.created().len(), 1);
    assert_eq!(storage.deleted().len(), 1);
    assert_eq!(storage.deleted().iter().next().unwrap().0, &id2);
    assert_eq!(id1, storage.get_created_keys().pop().unwrap());
    storage.flush();
    assert!(storage.read_object(&id2).is_none());
    let new_obj1 = storage.read_object(&id1).unwrap();
    // sequence # should increase after unwrapping
    assert_eq!(new_obj1.version(), obj1_version.increment());
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
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object_id = ObjectID::random();
    let gas_object =
        Object::with_id_owner_for_testing(gas_object_id, base_types::SuiAddress::default());
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr1
    // ObjectBasics::create expects integer value and recipient address
    let addr1 = get_key_pair().0;
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&AccountAddress::from(addr1)).unwrap(),
    ];
    let response = call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "create",
        gas_object,
        15, // This budget is not enough to execute all bytecode.
        Vec::new(),
        Vec::new(),
        pure_args.clone(),
    );
    let err = response.unwrap().unwrap_err();
    assert!(err.1.to_string().contains("VMError with status OUT_OF_GAS"));
    // Provided gas_budget will be deducted as gas.
    assert_eq!(err.0, 15);

    // Trying again with a different gas budget.
    let gas_object = storage.read_object(&gas_object_id).unwrap();
    let response = call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "create",
        gas_object,
        50, // This budget is enough to execute bytecode, but not enough for processing transfer events.
        Vec::new(),
        Vec::new(),
        pure_args,
    );
    let err = response.unwrap().unwrap_err();
    assert!(matches!(err.1, SuiError::InsufficientGas { .. }));
    // Provided gas_budget will be deducted as gas.
    assert_eq!(err.0, 50);
}

#[test]
fn test_publish_module_insufficient_gas() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object = Object::with_id_owner_gas_for_testing(
        ObjectID::random(),
        SequenceNumber::from(1),
        base_types::SuiAddress::default(),
        30,
    );
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create a module.
    let module = file_format::empty_module();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];

    let mut tx_context = TxContext::random_for_testing_only();
    let response = adapter::publish(
        &mut storage,
        native_functions,
        module_bytes,
        &mut tx_context,
        GAS_BUDGET,
        gas_object,
    );
    let err = response.unwrap().unwrap_err();
    assert!(err
        .1
        .to_string()
        .contains("Gas balance is 30, not enough to pay"));
}

#[test]
fn test_freeze() {
    let addr1 = base_types::get_new_address();

    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create obj1 owned by addr1
    // ObjectBasics::create expects integer value and recipient address
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        bcs::to_bytes(&AccountAddress::from(addr1)).unwrap(),
    ];
    call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "create",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        Vec::new(),
        pure_args,
    )
    .unwrap()
    .unwrap();

    let id1 = storage.get_created_keys().pop().unwrap();
    storage.flush();
    let obj1 = storage.read_object(&id1).unwrap();
    assert!(!obj1.is_read_only());

    // 2. Call freeze_object.
    call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "freeze_object",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        vec![obj1],
        vec![],
    )
    .unwrap()
    .unwrap();
    assert_eq!(storage.updated().len(), 2);
    storage.flush();
    let obj1 = storage.read_object(&id1).unwrap();
    assert!(obj1.is_read_only());
    assert!(obj1.owner == Owner::SharedImmutable);

    // 3. Call transfer again and it should fail.
    let pure_args = vec![bcs::to_bytes(&AccountAddress::from(addr1)).unwrap()];
    let result = call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "transfer",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        vec![obj1],
        pure_args,
    );
    let err = result.unwrap().unwrap_err();
    assert!(err
        .1
        .to_string()
        .contains("Argument 0 is expected to be mutable, immutable object found"));
    // Since it failed before VM execution, during type resolving,
    // only minimum gas will be charged.
    assert_eq!(err.0, gas::MIN_MOVE);

    // 4. Call set_value (pass as mutable reference) should fail as well.
    let obj1 = storage.read_object(&id1).unwrap();
    let pure_args = vec![bcs::to_bytes(&1u64).unwrap()];
    let result = call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "set_value",
        gas_object,
        GAS_BUDGET,
        Vec::new(),
        vec![obj1],
        pure_args,
    );
    let err = result.unwrap().unwrap_err();
    assert!(err
        .1
        .to_string()
        .contains("Argument 0 is expected to be mutable, immutable object found"));
    // Since it failed before VM execution, during type resolving,
    // only minimum gas will be charged.
    assert_eq!(err.0, gas::MIN_MOVE);
}

#[test]
fn test_move_call_args_type_mismatch() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());
    storage.write_object(gas_object.clone());
    storage.flush();

    // ObjectBasics::create expects 2 args: integer value and recipient address
    // Pass 1 arg only to trigger error.
    let pure_args = vec![10u64.to_le_bytes().to_vec()];
    let status = call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "create",
        gas_object,
        GAS_BUDGET,
        Vec::new(),
        Vec::new(),
        pure_args,
    )
    .unwrap();
    let (gas_used, err) = status.unwrap_err();
    assert_eq!(gas_used, gas::MIN_MOVE);
    assert!(err
        .to_string()
        .contains("Expected 3 arguments calling function 'create', but found 2"));

    /*
    // Need to fix https://github.com/MystenLabs/sui/issues/211
    // in order to enable the following test.
    let pure_args = vec![
        10u64.to_le_bytes().to_vec(),
        10u64.to_le_bytes().to_vec(),
    ];
    let status = call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "create",
        gas_object.clone(),
        GAS_BUDGET,
        Vec::new(),
        Vec::new(),
        pure_args,
    )
    .unwrap();
    let (gas_used, err) = status.unwrap_err();
    assert_eq!(gas_used, gas::MIN_MOVE);
    // Assert on the error message as well.
    */
}

#[test]
fn test_move_call_incorrect_function() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());
    storage.write_object(gas_object.clone());
    storage.flush();

    // Instead of calling on the genesis package, we are calling the gas object.
    let vm = adapter::new_move_vm(native_functions.clone()).expect("No errors");
    let status = adapter::execute(
        &vm,
        &mut storage,
        native_functions.clone(),
        gas_object.clone(),
        &Identifier::new("ObjectBasics").unwrap(),
        &Identifier::new("create").unwrap(),
        vec![],
        vec![],
        vec![],
        GAS_BUDGET,
        gas_object.clone(),
        &mut TxContext::random_for_testing_only(),
    )
    .unwrap();
    let (gas_used, err) = status.unwrap_err();
    assert_eq!(gas_used, gas::MIN_MOVE);
    assert!(err
        .to_string()
        .contains("Expected a module object, but found a Move object"));

    // Calling a non-existing function.
    let pure_args = vec![10u64.to_le_bytes().to_vec()];
    let status = call(
        &mut storage,
        &native_functions,
        "ObjectBasics",
        "foo",
        gas_object,
        GAS_BUDGET,
        Vec::new(),
        Vec::new(),
        pure_args,
    )
    .unwrap();
    let (gas_used, err) = status.unwrap_err();
    assert_eq!(gas_used, gas::MIN_MOVE);
    assert!(err.to_string().contains(&format!(
        "Could not resolve function 'foo' in module {}::ObjectBasics",
        SUI_FRAMEWORK_ADDRESS
    )));
}

#[test]
fn test_publish_module_linker_error() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let id_module = CompiledModule::deserialize(
        genesis_objects[1]
            .data
            .try_as_package()
            .unwrap()
            .serialized_module_map()
            .get("ID")
            .unwrap(),
    )
    .unwrap();

    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create a module that depends on a genesis module that exists, but via an invalid handle
    let mut dependent_module = file_format::empty_module();
    // make `dependent_module` depend on `id_module`
    dependent_module
        .identifiers
        .push(id_module.self_id().name().to_owned());
    dependent_module
        .address_identifiers
        .push(*id_module.self_id().address());
    dependent_module.module_handles.push(ModuleHandle {
        address: AddressIdentifierIndex((dependent_module.address_identifiers.len() - 1) as u16),
        name: IdentifierIndex((dependent_module.identifiers.len() - 1) as u16),
    });
    // now, the invalid part: add a StructHandle to `dependent_module` that doesn't exist in `m`
    dependent_module
        .identifiers
        .push(ident_str!("DoesNotExist").to_owned());
    dependent_module.struct_handles.push(StructHandle {
        module: ModuleHandleIndex((dependent_module.module_handles.len() - 1) as u16),
        name: IdentifierIndex((dependent_module.identifiers.len() - 1) as u16),
        abilities: AbilitySet::EMPTY,
        type_parameters: Vec::new(),
    });

    let mut module_bytes = Vec::new();
    dependent_module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];

    let mut tx_context = TxContext::random_for_testing_only();
    let response = adapter::publish(
        &mut storage,
        native_functions,
        module_bytes,
        &mut tx_context,
        GAS_BUDGET,
        gas_object,
    );
    let err = response.unwrap().unwrap_err();
    assert_eq!(err.0, gas::MIN_MOVE);
    let err_str = err.1.to_string();
    // make sure it's a linker error
    assert!(err_str.contains("VMError with status LOOKUP_FAILED"));
    // related to failed lookup of a struct handle
    assert!(err_str.contains("at index 0 for struct handle"))
}

#[test]
fn test_publish_module_non_zero_address() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();

    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment.
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());
    storage.write_object(gas_object.clone());
    storage.flush();

    // 1. Create an empty module.
    let mut module = file_format::empty_module();
    // 2. Change the module address to non-zero.
    module.address_identifiers.pop();
    module.address_identifiers.push(AccountAddress::random());

    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];

    let mut tx_context = TxContext::random_for_testing_only();
    let response = adapter::publish(
        &mut storage,
        native_functions,
        module_bytes,
        &mut tx_context,
        GAS_BUDGET,
        gas_object,
    );
    let err = response.unwrap().unwrap_err();
    assert_eq!(err.0, gas::MIN_MOVE);
    let err_str = err.1.to_string();
    println!("{:?}", err_str);
    assert!(err_str.contains("Publishing modules with non-zero address is not allowed"));
}

#[test]
fn test_coin_transfer() {
    let addr = base_types::SuiAddress::default();

    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();

    let mut storage = InMemoryStorage::new(genesis_objects);

    // 0. Create a gas object for gas payment. Note that we won't really use it because we won't be providing a gas budget.
    // 1. Create an object to transfer
    let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), addr);
    let to_transfer = Object::with_id_owner_for_testing(ObjectID::random(), addr);
    storage.write_object(gas_object.clone());
    storage.write_object(to_transfer.clone());
    storage.flush();

    let addr1 = sui_types::crypto::get_key_pair().0;

    call(
        &mut storage,
        &native_functions,
        "Coin",
        "transfer_",
        gas_object,
        GAS_BUDGET,
        vec![GAS::type_tag()],
        vec![to_transfer],
        vec![
            10u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&AccountAddress::from(addr1)).unwrap(),
        ],
    )
    .unwrap()
    .unwrap();

    // should update gas object and input coin
    assert_eq!(storage.updated().len(), 2);
    // should create one new coin
    assert_eq!(storage.created().len(), 1);
}

/// A helper function for publishing modules stored in source files.
fn publish_from_src(
    storage: &mut InMemoryStorage,
    natives: &NativeFunctionTable,
    src_path: &str,
    gas_object: Object,
    gas_budget: u64,
) {
    storage.write_object(gas_object.clone());
    storage.flush();

    // build modules to be published
    let build_config = BuildConfig::default();
    let mut module_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    module_path.push(src_path);
    let modules = sui_framework::build_move_package(&module_path, build_config, false).unwrap();

    // publish modules
    let all_module_bytes = modules
        .iter()
        .map(|m| {
            let mut module_bytes = Vec::new();
            m.serialize(&mut module_bytes).unwrap();
            module_bytes
        })
        .collect();
    let mut tx_context = TxContext::random_for_testing_only();
    let response = adapter::publish(
        storage,
        natives.clone(),
        all_module_bytes,
        &mut tx_context,
        gas_budget,
        gas_object,
    );
    assert!(matches!(response.unwrap(), ExecutionStatus::Success { .. }));
}

#[test]
fn test_simple_call() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // crate gas object for payment
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());

    // publish modules at a given path
    publish_from_src(
        &mut storage,
        &native_functions,
        "src/unit_tests/data/simple_call",
        gas_object.clone(),
        GAS_BUDGET,
    );
    // TODO: to be honest I am not sure why this flush is needed but
    // without it, the following assertion below fails:
    // assert!(obj.owner.is_address(&addr));
    storage.flush();

    // call published module function
    let obj_val = 42u64;

    let addr = base_types::get_new_address();
    let pure_args = vec![
        obj_val.to_le_bytes().to_vec(),
        bcs::to_bytes(&AccountAddress::from(addr)).unwrap(),
    ];

    let response = call(
        &mut storage,
        &native_functions,
        "M1",
        "create",
        gas_object,
        GAS_BUDGET,
        Vec::new(),
        Vec::new(),
        pure_args,
    );
    assert!(matches!(response.unwrap(), ExecutionStatus::Success { .. }));

    // check if the object was created and if it has the right value
    let id = storage.get_created_keys().pop().unwrap();
    storage.flush();
    let obj = storage.read_object(&id).unwrap();
    assert!(obj.owner == addr);
    assert_eq!(obj.version(), SequenceNumber::from(1));
    let move_obj = obj.data.try_as_move().unwrap();
    assert_eq!(
        u64::from_le_bytes(move_obj.type_specific_contents().try_into().unwrap()),
        obj_val
    );
}

#[test]
/// Tests publishing of a module with a constructor that creates a
/// single object with a single u64 value 42.
fn test_publish_init() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // crate gas object for payment
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());

    // publish modules at a given path
    publish_from_src(
        &mut storage,
        &native_functions,
        "src/unit_tests/data/publish_init",
        gas_object,
        GAS_BUDGET,
    );

    // a package object and a fresh object in the constructor should
    // have been crated
    assert_eq!(storage.created().len(), 2);
    let to_check = mem::take(&mut storage.temporary.created);
    let mut move_obj_exists = false;
    for o in to_check.values() {
        if let Data::Move(move_obj) = &o.data {
            move_obj_exists = true;
            assert_eq!(
                u64::from_le_bytes(move_obj.type_specific_contents().try_into().unwrap()),
                42u64
            );
        }
    }
    assert!(move_obj_exists);
}

#[test]
/// Tests public initializer that should not be executed upon
/// publishing the module.
fn test_publish_init_public() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // crate gas object for payment
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());

    // publish modules at a given path
    publish_from_src(
        &mut storage,
        &native_functions,
        "src/unit_tests/data/publish_init_public",
        gas_object,
        GAS_BUDGET,
    );

    // only a package object should have been crated
    assert_eq!(storage.created().len(), 1);
}

#[test]
/// Tests initializer returning a value that should not be executed
/// upon publishing the module.
fn test_publish_init_ret() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // crate gas object for payment
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());

    // publish modules at a given path
    publish_from_src(
        &mut storage,
        &native_functions,
        "src/unit_tests/data/publish_init_ret",
        gas_object,
        GAS_BUDGET,
    );

    // only a package object should have been crated
    assert_eq!(storage.created().len(), 1);
}

#[test]
/// Tests initializer with parameters other than &mut TxContext that
/// should not be executed upon publishing the module.
fn test_publish_init_param() {
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let genesis_objects = genesis::clone_genesis_packages();
    let mut storage = InMemoryStorage::new(genesis_objects);

    // crate gas object for payment
    let gas_object =
        Object::with_id_owner_for_testing(ObjectID::random(), base_types::SuiAddress::default());

    // publish modules at a given path
    publish_from_src(
        &mut storage,
        &native_functions,
        "src/unit_tests/data/publish_init_param",
        gas_object,
        GAS_BUDGET,
    );

    // only a package object should have been crated
    assert_eq!(storage.created().len(), 1);
}
