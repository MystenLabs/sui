// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::EventType;
use move_binary_format::errors::PartialVMResult;
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    gas_schedule::NativeCostIndex,
    loaded_data::runtime_types::Type,
    natives::function::{native_gas, NativeResult},
    pop_arg,
    values::{Struct, StructRef, Value},
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
    assert!(addr_bytes.len() == 32);
    // truncate the ID to 16 bytes
    // TODO: truncation not secure. we'll either need to support longer account addresses in Move or do this a different way
    // TODO: fix unwrap
    let addr = AccountAddress::from_bytes(&addr_bytes[0..16]).unwrap();

    // TODO: what should the cost of this be?
    let cost = native_gas(context.cost_table(), NativeCostIndex::CREATE_SIGNER, 0);

    Ok(NativeResult::ok(cost, smallvec![Value::address(addr)]))
}

pub fn get_id(
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

pub fn delete(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let obj = pop_arg!(args, Struct);
    // All of the following unwraps are safe by construction because a properly verified
    // bytecode ensures that the parameter must be of ID type with the correct fields.
    // Get the `id` field of the ID struct, which is of type IDBytes.
    let id_field = obj
        .unpack()
        .unwrap()
        .next()
        .unwrap()
        .value_as::<Struct>()
        .unwrap();
    // Get the inner address of type IDBytes.
    let id = id_field.unpack().unwrap().next().unwrap();

    // TODO: what should the cost of this be?
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);

    if !context.save_event(vec![], EventType::DeleteObjectID as u64, Type::Address, id)? {
        return Ok(NativeResult::err(cost, 0));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}
