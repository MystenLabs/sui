// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{legacy_create_signer_cost, legacy_emit_cost, EventType};
use move_binary_format::errors::PartialVMResult;
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{StructRef, Value},
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub fn bytes_to_address(
    _context: &mut NativeContext,
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
    let cost = legacy_create_signer_cost();

    Ok(NativeResult::ok(cost, smallvec![Value::address(addr)]))
}

pub fn borrow_uid(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    let obj = pop_arg!(args, StructRef);
    let id_field = obj.borrow_field(0)?;

    // TODO: what should the cost of this be?
    let cost = legacy_emit_cost();

    Ok(NativeResult::ok(cost, smallvec![id_field]))
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
    let cost = legacy_emit_cost();

    if !context.save_event(vec![], EventType::DeleteObjectID as u64, ty, info)? {
        return Ok(NativeResult::err(cost, 0));
    }

    Ok(NativeResult::ok(cost, smallvec![]))
}
