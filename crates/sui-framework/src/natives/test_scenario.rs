// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    legacy_test_cost,
    natives::{
        get_nth_struct_field,
        object_runtime::{ObjectRuntime, RuntimeResults},
    },
};
use linked_hash_map::LinkedHashMap;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::StructTag,
    value::{MoveStruct, MoveValue},
    vm_status::StatusCode,
};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{self, StructRef, Value},
};
use smallvec::smallvec;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    id::UID,
    object::Owner,
    storage::WriteKind,
};

const E_COULD_NOT_GENERATE_EFFECTS: u64 = 0;
const E_INVALID_SHARED_OR_IMMUTABLE_USAGE: u64 = 1;
const E_OBJECT_NOT_FOUND_CODE: u64 = 4;

// LinkedHashSet has a bug for accessing the back/last element
type Set<K> = LinkedHashMap<K, ()>;

// This function updates the inventories based on the transfers and deletes that occurred in the
// transaction
// native fun end_transaction(): TransactionResult;
pub fn end_transaction(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.is_empty());
    assert!(args.is_empty());
    let object_runtime_ref: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let taken_shared_or_imm: BTreeMap<_, _> = object_runtime_ref
        .test_inventories
        .taken
        .iter()
        .filter(|(_id, owner)| matches!(owner, Owner::Shared { .. } | Owner::Immutable))
        .map(|(id, owner)| (*id, *owner))
        .collect();
    // set to true if a shared or imm object was:
    // - transferred in a way that changes it from its original shared/imm state
    // - wraps the object
    // if true, we will "abort"
    let mut incorrect_shared_or_imm_handling = false;
    let mut new_object_values = LinkedHashMap::new();
    let mut transferred = vec![];
    for (id, owner, ty, _, value) in &object_runtime_ref.state.transfers {
        new_object_values.insert(*id, (ty.clone(), value.copy_value().unwrap()));
        transferred.push((*id, *owner));
        // mark as "incorrect" if the shared/imm owner was changed
        incorrect_shared_or_imm_handling = incorrect_shared_or_imm_handling
            || taken_shared_or_imm
                .get(id)
                .map(|shared_or_imm_owner| shared_or_imm_owner != owner)
                .unwrap_or(/* not incorrect */ false);
    }
    for id in object_runtime_ref.state.deleted_ids.keys() {
        // mark as "incorrect" if the shared/imm owner was deleted
        incorrect_shared_or_imm_handling =
            incorrect_shared_or_imm_handling || taken_shared_or_imm.contains_key(id);
    }
    let mut object_runtime_state = object_runtime_ref.take_state();
    // find all wrapped objects
    let all_wrapped = find_all_wrapped_objects(context, &new_object_values);
    // mark as "incorrect" if a shared/imm object was wrapped
    incorrect_shared_or_imm_handling = incorrect_shared_or_imm_handling
        || taken_shared_or_imm
            .keys()
            .any(|id| all_wrapped.contains(id));
    // if incorrect handling, return with an 'abort'
    if incorrect_shared_or_imm_handling {
        return Ok(NativeResult::err(
            legacy_test_cost(),
            E_INVALID_SHARED_OR_IMMUTABLE_USAGE,
        ));
    }
    // set all wrapped objects to being by_value so they get correctly marked as
    // wrapped
    for wrapped in &all_wrapped {
        object_runtime_state
            .input_objects
            .entry(*wrapped)
            .and_modify(|(by_value, _owner)| *by_value = true);
    }
    // Determine writes and deletes
    let results = object_runtime_state.finish();
    let RuntimeResults {
        writes,
        deletions,
        user_events,
    } = match results {
        Ok(res) => res,
        Err(_) => {
            return Ok(NativeResult::err(
                legacy_test_cost(),
                E_COULD_NOT_GENERATE_EFFECTS,
            ));
        }
    };
    let object_runtime_ref: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime_ref.test_inventories;
    // cleanup inventories
    // we will remove all changed objects
    // - deleted objects need to be removed to mark deletions
    // - written objects are removed and later replaced to mark new values and new owners
    for id in deletions.keys().chain(writes.keys()) {
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
        inventories.taken.remove(id);
    }
    // handle transfers, inserting transferred/written objects into their respective inventory
    let mut created = vec![];
    let mut written = vec![];
    for (id, (kind, owner, ty, _, _)) in writes {
        let (_, value) = new_object_values.remove(&id).unwrap();
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
                    .entry(ty)
                    .or_insert_with(Set::new)
                    .insert(id, ());
            }
            Owner::ObjectOwner(_) => (),
            Owner::Shared { .. } => {
                inventories
                    .shared_inventory
                    .entry(ty)
                    .or_insert_with(Set::new)
                    .insert(id, ());
            }
            Owner::Immutable => {
                inventories
                    .immutable_inventory
                    .entry(ty)
                    .or_insert_with(Set::new)
                    .insert(id, ());
            }
        }
    }
    // deletions already handled above, but we drop the delete kind for the effects
    let mut deleted = vec![];
    for (id, _) in deletions {
        deleted.push(id);
    }

    // new input objects are remaining taken objects not written/deleted
    object_runtime_ref.state.input_objects = inventories
        .taken
        .iter()
        .map(|(id, owner)| {
            // by value will be set to true later, if wrapped
            (*id, (/* by_value */ false, *owner))
        })
        .collect::<BTreeMap<_, _>>();
    let effects = transaction_effects(
        created,
        written,
        deleted,
        transferred,
        user_events.len() as u64,
    );
    Ok(NativeResult::ok(legacy_test_cost(), smallvec![effects]))
}

// native fun take_from_address_by_id<T: key>(account: address, id: ID): T;
pub fn take_from_address_by_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let specified_ty = get_specified_ty(ty_args);
    let id = pop_id(&mut args)?;
    let account: SuiAddress = pop_arg!(args, AccountAddress).into();
    pop_arg!(args, StructRef);
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
        &mut object_runtime.state.input_objects,
        id,
        Owner::AddressOwner(account),
    ))
}

// native fun ids_for_address<T: key>(account: address): vector<ID>;
pub fn ids_for_address(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let specified_ty = get_specified_ty(ty_args);
    let account: SuiAddress = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let ids = inventories
        .address_inventories
        .get(&account)
        .and_then(|inv| inv.get(&specified_ty))
        .map(|s| {
            s.keys()
                .map(|id| pack_id(*id))
                .into_iter()
                .collect::<Vec<Value>>()
        })
        .unwrap_or_default();
    let ids_vector = Value::vector_for_testing_only(ids);
    Ok(NativeResult::ok(legacy_test_cost(), smallvec![ids_vector]))
}

// native fun most_recent_id_for_address<T: key>(account: address): Option<ID>;
pub fn most_recent_id_for_address(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let specified_ty = get_specified_ty(ty_args);
    let account: SuiAddress = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let most_recent_id = match inventories.address_inventories.get(&account) {
        None => pack_option(None),
        Some(inv) => most_recent_at_ty(&inventories.taken, inv, specified_ty),
    };
    Ok(NativeResult::ok(
        legacy_test_cost(),
        smallvec![most_recent_id],
    ))
}

// native fun was_taken_from_address(account: address, id: ID): bool;
pub fn was_taken_from_address(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.is_empty());
    let id = pop_id(&mut args)?;
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
        legacy_test_cost(),
        smallvec![Value::bool(was_taken)],
    ))
}

// native fun take_immutable_by_id<T: key>(id: ID): T;
pub fn take_immutable_by_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let specified_ty = get_specified_ty(ty_args);
    let id = pop_id(&mut args)?;
    pop_arg!(args, StructRef);
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
        &mut object_runtime.state.input_objects,
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
    let specified_ty = get_specified_ty(ty_args);
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let most_recent_id = most_recent_at_ty(
        &inventories.taken,
        &inventories.immutable_inventory,
        specified_ty,
    );
    Ok(NativeResult::ok(
        legacy_test_cost(),
        smallvec![most_recent_id],
    ))
}

// native fun was_taken_immutable(id: ID): bool;
pub fn was_taken_immutable(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.is_empty());
    let id = pop_id(&mut args)?;
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let was_taken = inventories
        .taken
        .get(&id)
        .map(|owner| owner == &Owner::Immutable)
        .unwrap_or(false);
    Ok(NativeResult::ok(
        legacy_test_cost(),
        smallvec![Value::bool(was_taken)],
    ))
}

// native fun take_shared_by_id<T: key>(id: ID): T;
pub fn take_shared_by_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let specified_ty = get_specified_ty(ty_args);
    let id = pop_id(&mut args)?;
    pop_arg!(args, StructRef);
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
        &mut object_runtime.state.input_objects,
        id,
        Owner::Shared { initial_shared_version: /* dummy */ SequenceNumber::new() },
    ))
}

// native fun most_recent_id_shared<T: key>(): Option<ID>;
pub fn most_recent_id_shared(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let specified_ty = get_specified_ty(ty_args);
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let most_recent_id = most_recent_at_ty(
        &inventories.taken,
        &inventories.shared_inventory,
        specified_ty,
    );
    Ok(NativeResult::ok(
        legacy_test_cost(),
        smallvec![most_recent_id],
    ))
}

// native fun was_taken_shared(id: ID): bool;
pub fn was_taken_shared(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.is_empty());
    let id = pop_id(&mut args)?;
    assert!(args.is_empty());
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let inventories = &mut object_runtime.test_inventories;
    let was_taken = inventories
        .taken
        .get(&id)
        .map(|owner| matches!(owner, Owner::Shared { .. }))
        .unwrap_or(false);
    Ok(NativeResult::ok(
        legacy_test_cost(),
        smallvec![Value::bool(was_taken)],
    ))
}

// impls

fn take_from_inventory(
    is_in_inventory: impl FnOnce(&ObjectID) -> bool,
    objects: &BTreeMap<ObjectID, Value>,
    taken: &mut BTreeMap<ObjectID, Owner>,
    input_objects: &mut BTreeMap<ObjectID, (/* by_value */ bool, Owner)>,
    id: ObjectID,
    owner: Owner,
) -> NativeResult {
    let obj_opt = objects.get(&id);
    let is_taken = taken.contains_key(&id);
    if is_taken || !is_in_inventory(&id) || obj_opt.is_none() {
        return NativeResult::err(legacy_test_cost(), E_OBJECT_NOT_FOUND_CODE);
    }
    taken.insert(id, owner);
    // by_value will be set to true later, if wrapped
    input_objects.insert(id, (false, owner));
    let obj = obj_opt.unwrap();
    NativeResult::ok(legacy_test_cost(), smallvec![obj.copy_value().unwrap()])
}

fn most_recent_at_ty(
    taken: &BTreeMap<ObjectID, Owner>,
    inv: &BTreeMap<Type, Set<ObjectID>>,
    ty: Type,
) -> Value {
    pack_option(most_recent_at_ty_opt(taken, inv, ty))
}

fn most_recent_at_ty_opt(
    taken: &BTreeMap<ObjectID, Owner>,
    inv: &BTreeMap<Type, Set<ObjectID>>,
    ty: Type,
) -> Option<Value> {
    let s = inv.get(&ty)?;
    let most_recent_id = s.keys().filter(|id| !taken.contains_key(id)).last()?;
    Some(pack_id(*most_recent_id))
}

fn get_specified_ty(mut ty_args: Vec<Type>) -> Type {
    assert!(ty_args.len() == 1);
    ty_args.pop().unwrap()
}

// helpers
fn pop_id(args: &mut VecDeque<Value>) -> PartialVMResult<ObjectID> {
    let v = match args.pop_back() {
        None => {
            return Err(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            ))
        }
        Some(v) => v,
    };
    Ok(get_nth_struct_field(v, 0)?
        .value_as::<AccountAddress>()?
        .into())
}

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
            Owner::Shared { .. } => shared.push(id),
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

fn find_all_wrapped_objects(
    context: &mut NativeContext,
    new_object_values: &LinkedHashMap<ObjectID, (Type, Value)>,
) -> BTreeSet<ObjectID> {
    let mut ids = BTreeSet::new();
    for (ty, value) in new_object_values.values() {
        let layout = match context.type_to_type_layout(ty) {
            Ok(Some(layout)) => layout,
            _ => {
                debug_assert!(false);
                continue;
            }
        };
        let annotated_layout = match context.type_to_fully_annotated_layout(ty) {
            Ok(Some(layout)) => layout,
            _ => {
                debug_assert!(false);
                continue;
            }
        };
        let blob = value.simple_serialize(&layout).unwrap();
        let move_value = MoveValue::simple_deserialize(&blob, &annotated_layout).unwrap();
        let uid = UID::type_();
        visit_structs(
            &move_value,
            |_, _| panic!("unexpected struct without a struct tag. Layout: {}", layout),
            |_, _| panic!("unexpected struct without a struct tag. Layout: {}", layout),
            |depth, tag, fields| {
                if tag != &uid {
                    return if depth == 0 {
                        debug_assert!(!fields.is_empty());
                        // all object values so the first field is a UID that should be skipped
                        &fields[1..]
                    } else {
                        fields
                    };
                }
                debug_assert!(fields.len() == 1);
                let id = &fields[0].1;
                let addr_field = match &id {
                    MoveValue::Struct(MoveStruct::WithTypes { fields, .. }) => {
                        debug_assert!(fields.len() == 1);
                        &fields[0].1
                    }
                    v => unreachable!("Not reachable via Move type system: {:?}", v),
                };
                let addr = match addr_field {
                    MoveValue::Address(a) => *a,
                    v => unreachable!("Not reachable via Move type system: {:?}", v),
                };
                ids.insert(addr.into());
                fields
            },
        )
    }
    ids
}

fn visit_structs<FVisitTypes>(
    move_value: &MoveValue,
    mut visit_runtime: impl FnMut(/* value depth */ usize, &Vec<MoveValue>) -> &[MoveValue],
    mut visit_with_fields: impl FnMut(
        /* value depth */ usize,
        &Vec<(Identifier, MoveValue)>,
    ) -> &[(Identifier, MoveValue)],
    mut visit_with_types: FVisitTypes,
) where
    for<'a> FVisitTypes: FnMut(
        /* value depth */ usize,
        &StructTag,
        &'a Vec<(Identifier, MoveValue)>,
    ) -> &'a [(Identifier, MoveValue)],
{
    visit_structs_impl(
        move_value,
        &mut visit_runtime,
        &mut visit_with_fields,
        &mut visit_with_types,
        0,
    )
}

fn visit_structs_impl<FVisitTypes>(
    move_value: &MoveValue,
    visit_runtime: &mut impl FnMut(/* value depth */ usize, &Vec<MoveValue>) -> &[MoveValue],
    visit_with_fields: &mut impl FnMut(
        /* value depth */ usize,
        &Vec<(Identifier, MoveValue)>,
    ) -> &[(Identifier, MoveValue)],
    visit_with_types: &mut FVisitTypes,
    depth: usize,
) where
    for<'a> FVisitTypes: FnMut(
        /* value depth */ usize,
        &StructTag,
        &'a Vec<(Identifier, MoveValue)>,
    ) -> &'a [(Identifier, MoveValue)],
{
    let next_depth = depth + 1;
    match move_value {
        MoveValue::U8(_)
        | MoveValue::U64(_)
        | MoveValue::U128(_)
        | MoveValue::Bool(_)
        | MoveValue::Address(_)
        | MoveValue::Signer(_) => (),
        MoveValue::Vector(vs) => {
            for v in vs {
                visit_structs_impl(
                    v,
                    visit_runtime,
                    visit_with_fields,
                    visit_with_types,
                    next_depth,
                )
            }
        }
        MoveValue::Struct(s) => match s {
            MoveStruct::Runtime(vs) => {
                let vs = visit_runtime(depth, vs);
                for v in vs {
                    visit_structs_impl(
                        v,
                        visit_runtime,
                        visit_with_fields,
                        visit_with_types,
                        next_depth,
                    )
                }
            }
            MoveStruct::WithFields(fields) => {
                let fields = visit_with_fields(depth, fields);
                for (_, v) in fields {
                    visit_structs_impl(
                        v,
                        visit_runtime,
                        visit_with_fields,
                        visit_with_types,
                        next_depth,
                    )
                }
            }
            MoveStruct::WithTypes { type_, fields } => {
                let fields = visit_with_types(depth, type_, fields);
                for (_, v) in fields {
                    visit_structs_impl(
                        v,
                        visit_runtime,
                        visit_with_fields,
                        visit_with_types,
                        next_depth,
                    )
                }
            }
        },
    }
}
