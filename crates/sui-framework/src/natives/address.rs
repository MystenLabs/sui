// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, u256::U256};
use move_vm_runtime::{
    native_charge_gas_early_exit, native_functions::NativeContext, native_gas_total_cost,
};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::{collections::VecDeque, ops::Mul};
use sui_cost_tables::natives_tables::NATIVES_COST_LOW;

const E_ADDRESS_PARSE_ERROR: u64 = 0;

/***************************************************************************************************
 * native fun from_bytes
 * Implementation of the Move native function `address::from_bytes(bytes: vector<u8>)`
 *   gas cost: NATIVES_COST_LOW * AccountAddress::LENGTH         | converting bytes into an address
 *
 **************************************************************************************************/
pub fn from_bytes(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);
    let mut gas_left = context.gas_budget();
    let addr_bytes = pop_arg!(args, Vec<u8>);
    // Copying bytes is a simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        gas_left,
        NATIVES_COST_LOW.mul((AccountAddress::LENGTH as u64).into())
    );

    let cost = native_gas_total_cost!(context, gas_left);

    // Address parsing can fail if fed the incorrect number of bytes.
    Ok(match AccountAddress::from_bytes(addr_bytes) {
        Ok(addr) => NativeResult::ok(cost, smallvec![Value::address(addr)]),
        Err(_) => NativeResult::err(cost, E_ADDRESS_PARSE_ERROR),
    })
}

/***************************************************************************************************
 * native fun to_u256
 * Implementation of the Move native function `address::to_u256(address): u256`
 *   gas cost: NATIVES_COST_LOW * AccountAddress::LENGTH        | converting address into an vec<u8>
 *              + NATIVES_COST_LOW * AccountAddress::LENGTH     | reversing the vec<u8>
 *              + NATIVES_COST_LOW * 2 * AccountAddress::LENGTH | copying and converting to Value::u256
 **************************************************************************************************/
pub fn to_u256(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);
    let mut gas_left = context.gas_budget();

    let addr = pop_arg!(args, AccountAddress);
    // Copying bytes is a simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        gas_left,
        NATIVES_COST_LOW.mul((AccountAddress::LENGTH as u64).into())
    );
    let mut addr_bytes_le = addr.to_vec();
    // Reversing bytes is a simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        gas_left,
        NATIVES_COST_LOW.mul((AccountAddress::LENGTH as u64).into())
    );
    addr_bytes_le.reverse();

    // Copying bytes and converting to Value::u256 are simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        gas_left,
        NATIVES_COST_LOW.mul((2 * AccountAddress::LENGTH as u64).into())
    );
    // unwrap safe because we know addr_bytes_le is length 32
    let u256_val = Value::u256(U256::from_le_bytes(&addr_bytes_le.try_into().unwrap()));
    Ok(NativeResult::ok(
        native_gas_total_cost!(context, gas_left),
        smallvec![u256_val],
    ))
}

/***************************************************************************************************
 * native fun from_u256
 * Implementation of the Move native function `address::from_u256(u256): address`
 *   gas cost: NATIVES_COST_LOW * 2 * AccountAddress::LENGTH        | converting u256 into byte[] and vec<u8>
 *              + NATIVES_COST_LOW * AccountAddress::LENGTH         | reversing the vec<u8>
 *              + NATIVES_COST_LOW * 2 * AccountAddress::LENGTH     | copying and converting to Address::address
 **************************************************************************************************/
pub fn from_u256(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);
    let mut gas_left = context.gas_budget();

    let u256 = pop_arg!(args, U256);

    // Copying bytes snd converting sre simple low-cost operations
    native_charge_gas_early_exit!(
        context,
        gas_left,
        NATIVES_COST_LOW.mul((2 * AccountAddress::LENGTH as u64).into())
    );
    let mut u256_bytes = u256.to_le_bytes().to_vec();
    // Reversing bytes is a simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        gas_left,
        NATIVES_COST_LOW.mul((AccountAddress::LENGTH as u64).into())
    );
    u256_bytes.reverse();

    // Copying bytes snd converting sre simple low-cost operations
    native_charge_gas_early_exit!(
        context,
        gas_left,
        NATIVES_COST_LOW.mul((2 * AccountAddress::LENGTH as u64).into())
    );
    // unwrap safe because we are passing a 32 byte slice
    let addr_val = Value::address(AccountAddress::from_bytes(&u256_bytes[..]).unwrap());
    Ok(NativeResult::ok(
        native_gas_total_cost!(context, gas_left),
        smallvec![addr_val],
    ))
}
