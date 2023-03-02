// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::legacy_create_signer_cost;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, u256::U256};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

const E_ADDRESS_PARSE_ERROR: u64 = 0;

// Implementation of the Move native function address::from_bytes(bytes: vector<u8>): address;
pub fn from_bytes(
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

/// Implementation of Move native function `address::to_u256(address): u256`
pub fn to_u256(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let addr = pop_arg!(args, AccountAddress);
    let mut addr_bytes_le = addr.to_vec();
    addr_bytes_le.reverse();
    // unwrap safe because we know addr_bytes_le is length 32
    let u256_val = Value::u256(U256::from_le_bytes(&addr_bytes_le.try_into().unwrap()));
    // TODO: what should the cost of this be?
    let cost = legacy_create_signer_cost();
    Ok(NativeResult::ok(cost, smallvec![u256_val]))
}

/// Implementation of Move native function `address::from_u256(u256): address`
pub fn from_u256(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    // TODO: what should the cost of this be?
    let cost = legacy_create_signer_cost();

    let u256 = pop_arg!(args, U256);
    let mut u256_bytes = u256.to_le_bytes().to_vec();
    u256_bytes.reverse();

    // unwrap safe because we are passing a 32 byte slice
    let addr_val = Value::address(AccountAddress::from_bytes(&u256_bytes[..]).unwrap());
    Ok(NativeResult::ok(cost, smallvec![addr_val]))
}
