// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO remove once used
#![allow(dead_code)]

use crate::{
    legacy_emit_cost,
    natives::{
        get_object_id,
        object_runtime::{ObjectRuntime, RuntimeResults},
    },
};
use linked_hash_map::LinkedHashMap;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{StructTag, TypeTag},
};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{self, Value},
};
use smallvec::smallvec;
use std::collections::{BTreeMap, VecDeque};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    object::Owner,
    storage::{ObjectChange, WriteKind},
};

const E_OBJECT_NOT_FOUND_CODE: u64 = 5;
const E_WRONG_OBJECT_TYPE_CODE: u64 = 6;

// LinkedHashSet has a bug for accessing the back/last element
type Set<K> = LinkedHashMap<K, ()>;

// native fun end_transaction(): TransactionResult;
pub fn end_transaction(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    assert!(ty_args.is_empty());
    assert!(args.is_empty());
    let object_runtime_ref: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let mut new_object_values = LinkedHashMap::new();
    let mut transferred = vec![];
    for (owner, tag, value) in &object_runtime_ref.transfers {
        let id: ObjectID = get_object_id(value.copy_value().unwrap())
            .unwrap()
            .value_as::<AccountAddress>()
            .unwrap()
            .into();
        new_object_values.insert(id, (*owner, tag.clone(), value.copy_value().unwrap()));
        transferred.push((id, *owner));
    }
    assert!(object_runtime_ref.input_objects.is_empty());
    let object_runtime = object_runtime_ref.take();
    let results = object_runtime.finish();
    let RuntimeResults {
        changes,
        user_events,
    } = match results {
        Ok(res) => res,
        Err(_) => {
            return Ok(NativeResult::ok(
                legacy_emit_cost(),
                smallvec![transaction_result(None)],
            ));
        }
    };
    let inventories = &mut object_runtime_ref.test_inventories;
    let mut created = vec![];
    let mut written = vec![];
    let mut deleted = vec![];
    // handle transfers
    for (id, change) in changes {
        match change {
            ObjectChange::Delete(_, _) => deleted.push(id),
            ObjectChange::Write(_, kind) => {
                let (owner, tag, value) = new_object_values.remove(&id).unwrap();
                inventories.objects.insert(id, value);
                match kind {
                    WriteKind::Create => created.push(id),
                    WriteKind::Mutate | WriteKind::Unwrap => written.push(id),
                }
                match owner {
                    Owner::AddressOwner(a) => {
                        inventories
                            .address_inventories
                            .entry(a)
                            .or_insert_with(BTreeMap::new)
                            .entry(tag)
                            .or_insert_with(Set::new)
                            .insert(id, ());
                    }
                    Owner::ObjectOwner(_) => (),
                    Owner::Shared => {
                        inventories
                            .shared_inventory
                            .entry(tag)
                            .or_insert_with(Set::new)
                            .insert(id, ());
                    }
                    Owner::Immutable => {
                        inventories
                            .immutable_inventory
                            .entry(tag)
                            .or_insert_with(Set::new)
                            .insert(id, ());
                    }
                }
            }
        }
    }
    // handle deletions
    for id in &deleted {
        for addr_inventory in inventories.address_inventories.values_mut() {
            for s in addr_inventory.values_mut() {
                s.remove(id);
            }
        }
        for s in &mut inventories.shared_inventory.values_mut() {
            s.remove(id);
        }
        for s in &mut inventories.immutable_inventory.values_mut() {
            s.remove(id);
        }
    }
    let effects = transaction_effects(
        created,
        written,
        deleted,
        transferred,
        user_events.len() as u64,
    );
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![transaction_result(Some(effects))],
    ))
}

// native fun take_from_address_by_id<T: key>(account: address, id: ID): T;
pub fn take_from_address_by_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    let specified_ty = get_specified_ty(context, ty_args);
    let id: ObjectID = pop_arg!(args, AccountAddress).into();
    let account: SuiAddress = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    Ok(take_from_inventory(
        |x| {
            inventories
                .address_inventories
                .get(&account)
                .and_then(|inv| inv.get(&specified_ty))
                .map(|s| s.contains_key(x))
                .unwrap_or(false)
        },
        &inventories.objects,
        &mut inventories.taken,
        id,
        Owner::AddressOwner(account),
    ))
}

// native fun most_recent_id_for_address<T: key>(account: address): Option<ID>;
pub fn most_recent_id_for_address(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    let specified_ty = get_specified_ty(context, ty_args);
    let account: SuiAddress = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let most_recent_id = match inventories.address_inventories.get(&account) {
        None => pack_option(None),
        Some(inv) => most_recent_at_ty(inv, specified_ty),
    };
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![most_recent_id],
    ))
}

// native fun was_taken_from_address(account: address, id: ID): bool;
pub fn was_taken_from_address(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    assert!(ty_args.is_empty());
    let id: ObjectID = pop_arg!(args, AccountAddress).into();
    let account: SuiAddress = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let was_taken = inventories
        .taken
        .get(&id)
        .map(|owner| owner == &Owner::AddressOwner(account))
        .unwrap_or(false);
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![Value::bool(was_taken)],
    ))
}

// native fun take_immutable_by_id<T: key>(id: ID): T;
pub fn take_immutable_by_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    let specified_ty = get_specified_ty(context, ty_args);
    let id: ObjectID = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    Ok(take_from_inventory(
        |x| {
            inventories
                .immutable_inventory
                .get(&specified_ty)
                .map(|s| s.contains_key(x))
                .unwrap_or(false)
        },
        &inventories.objects,
        &mut inventories.taken,
        id,
        Owner::Immutable,
    ))
}

// native fun most_recent_immutable_id<T: key>(): Option<ID>;
pub fn most_recent_immutable_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    let specified_ty = get_specified_ty(context, ty_args);
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let most_recent_id = most_recent_at_ty(&inventories.immutable_inventory, specified_ty);
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![most_recent_id],
    ))
}

// native fun was_taken_immutable(id: ID): bool;
pub fn was_taken_immutable(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    assert!(ty_args.is_empty());
    let id: ObjectID = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let was_taken = inventories
        .taken
        .get(&id)
        .map(|owner| owner == &Owner::Immutable)
        .unwrap_or(false);
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![Value::bool(was_taken)],
    ))
}

// native fun take_shared_by_id<T: key>(id: ID): T;
pub fn take_shared_by_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    let specified_ty = get_specified_ty(context, ty_args);
    let id: ObjectID = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    Ok(take_from_inventory(
        |x| {
            inventories
                .shared_inventory
                .get(&specified_ty)
                .map(|s| s.contains_key(x))
                .unwrap_or(false)
        },
        &inventories.objects,
        &mut inventories.taken,
        id,
        Owner::Shared,
    ))
}

// native fun most_recent_id_shared<T: key>(): Option<ID>;
pub fn most_recent_id_shared(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    let specified_ty = get_specified_ty(context, ty_args);
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let most_recent_id = most_recent_at_ty(&inventories.shared_inventory, specified_ty);
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![most_recent_id],
    ))
}

// native fun was_taken_shared(id: ID): bool;
pub fn was_taken_shared(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    assert!(ty_args.is_empty());
    let id: ObjectID = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let was_taken = inventories
        .taken
        .get(&id)
        .map(|owner| owner == &Owner::Shared)
        .unwrap_or(false);
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![Value::bool(was_taken)],
    ))
}

// native fun all_shared_and_immutable_returned(): bool;
pub fn all_shared_and_immutable_returned(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(cfg!(feature = "testing"));
    assert!(ty_args.is_empty());
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let result = inventories
        .taken
        .values()
        .all(|owner| matches!(owner, Owner::Shared | Owner::Immutable));
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![Value::bool(result)],
    ))
}

// impls

fn take_from_inventory(
    is_in_inventory: impl FnOnce(&ObjectID) -> bool,
    objects: &BTreeMap<ObjectID, Value>,
    taken: &mut BTreeMap<ObjectID, Owner>,
    id: ObjectID,
    owner: Owner,
) -> NativeResult {
    let obj_opt = objects.get(&id);
    let is_taken = taken.contains_key(&id);
    if !is_taken && (!is_in_inventory(&id) || obj_opt.is_none()) {
        return NativeResult::err(legacy_emit_cost(), E_OBJECT_NOT_FOUND_CODE);
    }
    taken.insert(id, owner);
    let obj = obj_opt.unwrap();
    NativeResult::ok(legacy_emit_cost(), smallvec![obj.copy_value().unwrap()])
}

fn most_recent_at_ty(inv: &BTreeMap<StructTag, Set<ObjectID>>, tag: StructTag) -> Value {
    pack_option(
        inv.get(&tag)
            .and_then(|s| s.back().map(|(id, ())| pack_id(*id))),
    )
}

fn is_expected_ty(specified_ty: &TypeTag, expected_ty: &StructTag) -> bool {
    matches!(specified_ty, TypeTag::Struct(s) if s == expected_ty)
}

fn get_specified_ty(context: &mut NativeContext, ty_args: Vec<Type>) -> StructTag {
    assert_eq!(ty_args.len(), 1);
    match context.type_to_type_tag(&ty_args[0]).unwrap() {
        TypeTag::Struct(s) => s,
        _ => panic!("impossible, must be a struct since it has key"),
    }
}

// helpers
fn pack_id(a: impl Into<AccountAddress>) -> Value {
    Value::struct_(values::Struct::pack(vec![Value::address(a.into())]))
}

fn pack_ids(items: impl IntoIterator<Item = impl Into<AccountAddress>>) -> Value {
    Value::vector_for_testing_only(items.into_iter().map(pack_id))
}

fn pack_vec_map(items: impl IntoIterator<Item = (Value, Value)>) -> Value {
    Value::struct_(values::Struct::pack(vec![Value::vector_for_testing_only(
        items
            .into_iter()
            .map(|(k, v)| Value::struct_(values::Struct::pack(vec![k, v]))),
    )]))
}

fn transaction_effects(
    created: impl IntoIterator<Item = impl Into<AccountAddress>>,
    written: impl IntoIterator<Item = impl Into<AccountAddress>>,
    deleted: impl IntoIterator<Item = impl Into<AccountAddress>>,
    transferred: impl IntoIterator<Item = (ObjectID, Owner)>,
    num_events: u64,
) -> Value {
    let mut transferred_to_account = vec![];
    let mut transferred_to_object = vec![];
    let mut shared = vec![];
    let mut frozen = vec![];
    for (id, owner) in transferred {
        match owner {
            Owner::AddressOwner(a) => {
                transferred_to_account.push((pack_id(id), Value::address(a.into())))
            }
            Owner::ObjectOwner(o) => transferred_to_object.push((pack_id(id), pack_id(o))),
            Owner::Shared => shared.push(id),
            Owner::Immutable => frozen.push(id),
        }
    }

    let created_field = pack_ids(created);
    let written_field = pack_ids(written);
    let deleted_field = pack_ids(deleted);
    let transferred_to_account_field = pack_vec_map(transferred_to_account);
    let transferred_to_object_field = pack_vec_map(transferred_to_object);
    let shared_field = pack_ids(shared);
    let frozen_field = pack_ids(frozen);
    let num_events_field = Value::u64(num_events);
    Value::struct_(values::Struct::pack(vec![
        created_field,
        written_field,
        deleted_field,
        transferred_to_account_field,
        transferred_to_object_field,
        shared_field,
        frozen_field,
        num_events_field,
    ]))
}

fn pack_option(opt: Option<Value>) -> Value {
    let item = match opt {
        Some(v) => vec![v],
        None => vec![],
    };
    Value::struct_(values::Struct::pack(vec![Value::vector_for_testing_only(
        item,
    )]))
}

fn transaction_result(opt: Option<Value>) -> Value {
    Value::struct_(values::Struct::pack(vec![pack_option(opt)]))
}
