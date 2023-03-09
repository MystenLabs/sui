// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    legacy_emit_cost,
    natives::{
        get_nested_struct_field, get_object_id,
        object_runtime::{object_store::ObjectResult, ObjectRuntime},
    },
};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{StructTag, TypeTag},
    value::MoveTypeLayout,
    vm_status::StatusCode,
};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{StructRef, Value},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::dynamic_field::derive_dynamic_field_id;

const E_KEY_DOES_NOT_EXIST: u64 = 1;
const E_FIELD_TYPE_MISMATCH: u64 = 2;
const E_BCS_SERIALIZATION_FAILURE: u64 = 3;

macro_rules! get_or_fetch_object {
    ($context:ident, $ty_args:ident, $parent:ident, $child_id:ident) => {{
        let child_ty = $ty_args.pop().unwrap();
        assert!($ty_args.is_empty());
        let (layout, tag) = match get_tag_and_layout($context, &child_ty)? {
            Some(res) => res,
            None => {
                return Ok(NativeResult::err(
                    legacy_emit_cost(),
                    E_BCS_SERIALIZATION_FAILURE,
                ))
            }
        };
        let object_runtime: &mut ObjectRuntime = $context.extensions_mut().get_mut();
        object_runtime.get_or_fetch_child_object($parent, $child_id, &child_ty, layout, tag)?
    }};
}

// native fun hash_type_and_key<K: copy + drop + store>(parent: address, k: K): address;
pub fn hash_type_and_key(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert_eq!(ty_args.len(), 1);
    assert_eq!(args.len(), 2);
    let k_ty = ty_args.pop().unwrap();
    let k: Value = args.pop_back().unwrap();
    let parent = pop_arg!(args, AccountAddress);
    let k_tag = context.type_to_type_tag(&k_ty)?;
    let k_layout = match context.type_to_type_layout(&k_ty) {
        Ok(Some(layout)) => layout,
        _ => {
            return Ok(NativeResult::err(
                legacy_emit_cost(),
                E_BCS_SERIALIZATION_FAILURE,
            ))
        }
    };
    let Some(id) = derive_dynamic_field_id(parent, &k_tag, &k_layout, &k) else {
        return Ok(NativeResult::err(
            legacy_emit_cost(),
            E_BCS_SERIALIZATION_FAILURE,
        ));
    };

    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![Value::address(id.into())],
    ))
}

// throws `E_KEY_ALREADY_EXISTS` if a child already exists with that ID
// native fun add_child_object<Child: key>(parent: address, child: Child);
pub fn add_child_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.len() == 1);
    assert!(args.len() == 2);
    let child = args.pop_back().unwrap();
    let parent = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    // TODO remove this copy_value, which will require VM changes
    let child_id = get_object_id(child.copy_value().unwrap())
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap()
        .into();
    let child_ty = ty_args.pop().unwrap();
    assert!(ty_args.is_empty());
    let tag = match context.type_to_type_tag(&child_ty)? {
        TypeTag::Struct(s) => s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    object_runtime.add_child_object(parent, child_id, &child_ty, *tag, child)?;
    Ok(NativeResult::ok(legacy_emit_cost(), smallvec![]))
}

// throws `E_KEY_DOES_NOT_EXIST` if a child does not exist with that ID at that type
// or throws `E_FIELD_TYPE_MISMATCH` if the type does not match
// native fun borrow_child_object<Child: key>(parent: &UID, id: address): &Child;
// and (as the runtime does not distinguish different reference types)
// native fun borrow_child_object_mut<Child: key>(parent: &mut UID, id: address): &mut Child;
pub fn borrow_child_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.len() == 1);
    assert!(args.len() == 2);
    let child_id = pop_arg!(args, AccountAddress).into();

    let parent_uid = pop_arg!(args, StructRef).read_ref().unwrap();
    // UID { id: ID { bytes: address } }
    let parent = get_nested_struct_field(parent_uid, &[0, 0])
        .unwrap()
        .value_as::<AccountAddress>()
        .unwrap()
        .into();

    assert!(args.is_empty());
    let global_value_result = get_or_fetch_object!(context, ty_args, parent, child_id);
    let global_value = match global_value_result {
        ObjectResult::MismatchedType => {
            return Ok(NativeResult::err(legacy_emit_cost(), E_FIELD_TYPE_MISMATCH))
        }
        ObjectResult::Loaded(gv) => gv,
    };
    if !global_value.exists()? {
        return Ok(NativeResult::err(legacy_emit_cost(), E_KEY_DOES_NOT_EXIST));
    }
    let child_ref = global_value.borrow_global().map_err(|err| {
        assert!(err.major_status() != StatusCode::MISSING_DATA);
        err
    })?;
    Ok(NativeResult::ok(legacy_emit_cost(), smallvec![child_ref]))
}

// throws `E_KEY_DOES_NOT_EXIST` if a child does not exist with that ID at that type
// or throws `E_FIELD_TYPE_MISMATCH` if the type does not match
// native fun remove_child_object<Child: key>(parent: address, id: address): Child;
pub fn remove_child_object(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.len() == 1);
    assert!(args.len() == 2);
    let child_id = pop_arg!(args, AccountAddress).into();
    let parent = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let global_value_result = get_or_fetch_object!(context, ty_args, parent, child_id);
    let global_value = match global_value_result {
        ObjectResult::MismatchedType => {
            return Ok(NativeResult::err(legacy_emit_cost(), E_FIELD_TYPE_MISMATCH))
        }
        ObjectResult::Loaded(gv) => gv,
    };
    if !global_value.exists()? {
        return Ok(NativeResult::err(legacy_emit_cost(), E_KEY_DOES_NOT_EXIST));
    }
    let child = global_value.move_from().map_err(|err| {
        assert!(err.major_status() != StatusCode::MISSING_DATA);
        err
    })?;
    Ok(NativeResult::ok(legacy_emit_cost(), smallvec![child]))
}

//native fun has_child_object(parent: address, id: address): bool;
pub fn has_child_object(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.is_empty());
    assert!(args.len() == 2);
    let child_id = pop_arg!(args, AccountAddress).into();
    let parent = pop_arg!(args, AccountAddress).into();
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let has_child = object_runtime.child_object_exists(parent, child_id)?;
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![Value::bool(has_child)],
    ))
}

//native fun has_child_object_with_ty<Child: key>(parent: address, id: address): bool;
pub fn has_child_object_with_ty(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    assert!(ty_args.len() == 1);
    assert!(args.len() == 2);
    let child_id = pop_arg!(args, AccountAddress).into();
    let parent = pop_arg!(args, AccountAddress).into();
    assert!(args.is_empty());
    let ty = ty_args.pop().unwrap();
    let tag = match context.type_to_type_tag(&ty)? {
        TypeTag::Struct(s) => s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    let has_child = object_runtime.child_object_exists_and_has_type(parent, child_id, *tag)?;
    Ok(NativeResult::ok(
        legacy_emit_cost(),
        smallvec![Value::bool(has_child)],
    ))
}

fn get_tag_and_layout(
    context: &NativeContext,
    ty: &Type,
) -> PartialVMResult<Option<(MoveTypeLayout, StructTag)>> {
    let layout = match context.type_to_type_layout(ty)? {
        None => return Ok(None),
        Some(layout) => layout,
    };
    let tag = match context.type_to_type_tag(ty)? {
        TypeTag::Struct(s) => s,
        _ => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Sui verifier guarantees this is a struct".to_string()),
            )
        }
    };
    Ok(Some((layout, *tag)))
}
