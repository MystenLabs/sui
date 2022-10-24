// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{legacy_create_signer_cost, legacy_emit_cost, natives::object_runtime::ObjectRuntime};
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

const E_ADDRESS_PARSE_ERROR: u64 = 0;

// native fun address_from_bytes(bytes: vector<u8>): address;
pub fn address_from_bytes(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let addr_bytes = pop_arg!(args, Vec<u8>);

    // TODO: what should the cost of this be?
    let cost = legacy_create_signer_cost();

    // Address parsing can fail if fed the incorrect number of bytes.
    Ok(match AccountAddress::from_bytes(addr_bytes) {
        Ok(addr) => NativeResult::ok(cost, smallvec![Value::address(addr)]),
        Err(_) => NativeResult::err(cost, E_ADDRESS_PARSE_ERROR),
    })
}

// native fun borrow_uid<T: key>(obj: &T): &UID;
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

// native fun delete_impl(id: address);
pub fn delete_impl(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    // unwrap safe because the interface of native function guarantees it.
    let uid_bytes = pop_arg!(args, AccountAddress);

    // TODO: what should the cost of this be?
    let cost = legacy_emit_cost();

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    obj_runtime.delete_id(uid_bytes.into());
    Ok(NativeResult::ok(cost, smallvec![]))
}

// native fun record_new_uid(id: address);
pub fn record_new_uid(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);
    // unwrap safe because the interface of native function guarantees it.
    let uid_bytes = pop_arg!(args, AccountAddress);

    // TODO: what should the cost of this be?
    let cost = legacy_emit_cost();

    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    obj_runtime.new_id(uid_bytes.into());
    Ok(NativeResult::ok(cost, smallvec![]))
}
