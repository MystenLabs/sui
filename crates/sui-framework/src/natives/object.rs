// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::EventType;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, language_storage::TypeTag};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    gas_schedule::NativeCostIndex,
    loaded_data::runtime_types::Type,
    natives::function::{native_gas, NativeResult},
    pop_arg,
    values::{StructRef, Value},
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub fn bytes_to_address(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let addr_bytes = pop_arg!(args, Vec<u8>);
    // unwrap safe because this native function is only called from new_from_bytes,
    // which already asserts the size of bytes to be equal of account address.
    let addr = AccountAddress::from_bytes(addr_bytes).unwrap();

    // TODO: what should the cost of this be?
    let cost = native_gas(context.cost_table(), NativeCostIndex::CREATE_SIGNER, 0);

    Ok(NativeResult::ok(cost, smallvec![Value::address(addr)]))
}

pub fn borrow_uid(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let obj = pop_arg!(args, StructRef);
    let id_field = obj.borrow_field(0)?;

    // TODO: what should the cost of this be?
    let cost = native_gas(context.cost_table(), NativeCostIndex::SIGNER_BORROW, 0);

    Ok(NativeResult::ok(cost, smallvec![id_field]))
}

pub fn is_one_time_witness(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    // unwrap safe because the interface of native function guarantees it.
    let type_tag = context.type_to_type_tag(&ty_args.pop().unwrap())?;

    // TODO: what should the cost of this be?
    let cost = native_gas(context.cost_table(), NativeCostIndex::LENGTH, 1);

    // If a struct type has the same name as the module that defines it but capitalized, it means
    // that it's a characteristic type (which is one way of implementing a one-time witness
    // type). This is checked in the char_type validator pass (a type with this type of name that
    // does not have all properties required of a characteristic type will cause a validator error).
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::bool(
            matches!(type_tag, TypeTag::Struct(struct_tag) if struct_tag.name.to_string() == struct_tag.module.to_string().to_ascii_uppercase())
        )],
    ))
}

pub fn delete_impl(
    context: &mut NativeContext,
    mut ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    // unwrap safe because the interface of native function guarantees it.
    let ty = ty_args.pop().unwrap();
    let info = args.pop_back().unwrap();

    // TODO: what should the cost of this be?
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);

    if !context.save_event(vec![], EventType::DeleteObjectID as u64, ty, info)? {
        return Ok(NativeResult::err(cost, 0));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}
