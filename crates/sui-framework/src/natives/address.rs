// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::natives::NativesCostTable;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas, u256::U256};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::{collections::VecDeque, ops::Mul};

const E_ADDRESS_PARSE_ERROR: u64 = 0;
#[derive(Clone)]
pub struct AddressFromBytesCostParams {
    pub copy_bytes_to_address_cost_per_byte: InternalGas,
}
/***************************************************************************************************
 * native fun from_bytes
 * Implementation of the Move native function `address::from_bytes(bytes: vector<u8>)`
 *   gas cost: copy_bytes_to_address_cost_per_byte * AccountAddress::LENGTH         | converting bytes into an address
 *
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

    let addr_bytes = pop_arg!(args, Vec<u8>);
    // Copying bytes is a simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        address_from_bytes_cost_params
            .copy_bytes_to_address_cost_per_byte
            .mul((AccountAddress::LENGTH as u64).into())
    );

    let cost = context.gas_used();

    // Address parsing can fail if fed the incorrect number of bytes.
    Ok(match AccountAddress::from_bytes(addr_bytes) {
        Ok(addr) => NativeResult::ok(cost, smallvec![Value::address(addr)]),
        Err(_) => NativeResult::err(cost, E_ADDRESS_PARSE_ERROR),
    })
}
#[derive(Clone)]
pub struct AddressToU256CostParams {
    pub address_to_vec_cost_per_byte: InternalGas,
    pub address_vec_reverse_cost_per_byte: InternalGas,
    pub copy_convert_to_u256_cost_per_byte: InternalGas,
}
/***************************************************************************************************
 * native fun to_u256
 * Implementation of the Move native function `address::to_u256(address): u256`
 *   gas cost: address_to_vec_cost_per_byte * AccountAddress::LENGTH                | converting address into an vec<u8>
 *              + address_vec_reverse_cost_per_byte * AccountAddress::LENGTH        | reversing the vec<u8>
 *              + copy_convert_to_u256_cost_per_byte * 2 * AccountAddress::LENGTH   | copying and converting to Value::u256
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

    let addr = pop_arg!(args, AccountAddress);
    // Copying bytes is a simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        address_to_u256_cost_params
            .address_to_vec_cost_per_byte
            .mul((AccountAddress::LENGTH as u64).into())
    );
    let mut addr_bytes_le = addr.to_vec();
    // Reversing bytes is a simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        address_to_u256_cost_params
            .address_vec_reverse_cost_per_byte
            .mul((AccountAddress::LENGTH as u64).into())
    );
    addr_bytes_le.reverse();

    // Copying bytes and converting to Value::u256 are simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        address_to_u256_cost_params
            .copy_convert_to_u256_cost_per_byte
            .mul((2 * AccountAddress::LENGTH as u64).into())
    );
    // unwrap safe because we know addr_bytes_le is length 32
    let u256_val = Value::u256(U256::from_le_bytes(&addr_bytes_le.try_into().unwrap()));
    Ok(NativeResult::ok(context.gas_used(), smallvec![u256_val]))
}

#[derive(Clone)]
pub struct AddressFromU256CostParams {
    pub u256_to_bytes_to_vec_cost_per_byte: InternalGas,
    pub u256_bytes_vec_reverse_cost_per_byte: InternalGas,
    pub copy_convert_to_address_cost_per_byte: InternalGas,
}
/***************************************************************************************************
 * native fun from_u256
 * Implementation of the Move native function `address::from_u256(u256): address`
 *   gas cost: u256_to_bytes_to_vec_cost_per_byte * 2 * AccountAddress::LENGTH          | converting u256 into byte[] and vec<u8>
 *              + u256_bytes_vec_reverse_cost_per_byte * AccountAddress::LENGTH         | reversing the vec<u8>
 *              + copy_convert_to_address_cost_per_byte * 2 * AccountAddress::LENGTH    | copying and converting to Address::address
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

    let u256 = pop_arg!(args, U256);

    // Copying bytes snd converting sre simple low-cost operations
    native_charge_gas_early_exit!(
        context,
        address_from_u256_cost_params
            .u256_to_bytes_to_vec_cost_per_byte
            .mul((2 * AccountAddress::LENGTH as u64).into())
    );
    let mut u256_bytes = u256.to_le_bytes().to_vec();
    // Reversing bytes is a simple low-cost operation
    native_charge_gas_early_exit!(
        context,
        address_from_u256_cost_params
            .u256_bytes_vec_reverse_cost_per_byte
            .mul((AccountAddress::LENGTH as u64).into())
    );
    u256_bytes.reverse();

    // Copying bytes and converting sre simple low-cost operations
    native_charge_gas_early_exit!(
        context,
        address_from_u256_cost_params
            .copy_convert_to_address_cost_per_byte
            .mul((2 * AccountAddress::LENGTH as u64).into())
    );
    // unwrap safe because we are passing a 32 byte slice
    let addr_val = Value::address(AccountAddress::from_bytes(&u256_bytes[..]).unwrap());
    Ok(NativeResult::ok(context.gas_used(), smallvec![addr_val]))
}
