// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::NativesCostTable;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas, u256::U256};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

const E_ADDRESS_PARSE_ERROR: u64 = 0;
#[derive(Clone)]
pub struct AddressFromBytesCostParams {
    /// addresses are constant size, so base cost suffices
    pub address_from_bytes_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun from_bytes
 * Implementation of the Move native function `address::from_bytes(bytes: vector<u8>)`
 *   gas cost: address_from_bytes_cost_base                                        | addresses are constant size, so base cost suffices
 **************************************************************************************************/
pub fn from_bytes(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let address_from_bytes_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .address_from_bytes_cost_params
        .clone();

    // Charge base fee
    native_charge_gas_early_exit!(
        context,
        address_from_bytes_cost_params.address_from_bytes_cost_base
    );

    let addr_bytes = pop_arg!(args, Vec<u8>);
    let cost = context.gas_used();

    // Address parsing can fail if fed the incorrect number of bytes.
    Ok(match AccountAddress::from_bytes(addr_bytes) {
        Ok(addr) => NativeResult::ok(cost, smallvec![Value::address(addr)]),
        Err(_) => NativeResult::err(cost, E_ADDRESS_PARSE_ERROR),
    })
}
#[derive(Clone)]
pub struct AddressToU256CostParams {
    /// addresses and u256 are constant size, so base cost suffices
    pub address_to_u256_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun to_u256
 * Implementation of the Move native function `address::to_u256(address): u256`
 *   gas cost:  address_to_u256_cost_base                   | addresses and u256 are constant size, so base cost suffices
 **************************************************************************************************/
pub fn to_u256(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let address_to_u256_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .address_to_u256_cost_params
        .clone();

    // Charge flat cost
    native_charge_gas_early_exit!(
        context,
        address_to_u256_cost_params.address_to_u256_cost_base
    );

    let addr = pop_arg!(args, AccountAddress);
    let mut addr_bytes_le = addr.to_vec();
    addr_bytes_le.reverse();

    // unwrap safe because we know addr_bytes_le is length 32
    let u256_val = Value::u256(U256::from_le_bytes(&addr_bytes_le.try_into().unwrap()));
    Ok(NativeResult::ok(context.gas_used(), smallvec![u256_val]))
}

#[derive(Clone)]
pub struct AddressFromU256CostParams {
    /// addresses and u256 are constant size, so base cost suffices
    pub address_from_u256_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun from_u256
 * Implementation of the Move native function `address::from_u256(u256): address`
 *   gas cost: address_from_u256_cost_base              | addresses and u256 are constant size, so base cost suffices
 **************************************************************************************************/
pub fn from_u256(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let address_from_u256_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .address_from_u256_cost_params
        .clone();

    // charge flat fee
    native_charge_gas_early_exit!(
        context,
        address_from_u256_cost_params.address_from_u256_cost_base
    );

    let u256 = pop_arg!(args, U256);
    let mut u256_bytes = u256.to_le_bytes().to_vec();
    u256_bytes.reverse();

    // unwrap safe because we are passing a 32 byte slice
    let addr_val = Value::address(AccountAddress::from_bytes(&u256_bytes[..]).unwrap());
    Ok(NativeResult::ok(context.gas_used(), smallvec![addr_val]))
}
