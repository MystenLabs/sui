// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::natives::helpers::make_module_natives;
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::{InternalGas, InternalGasPerByte, NumBytes};
use move_vm_runtime::{
    native_charge_gas_early_exit,
    native_functions::{NativeContext, NativeFunction},
};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use sha2::{Digest, Sha256};
use sha3::Sha3_256;
use smallvec::smallvec;
use std::{collections::VecDeque, sync::Arc};

/***************************************************************************************************
 * native fun sha2_256
 *
 *   gas cost: base_cost + unit_cost * max(input_length_in_bytes, legacy_min_input_len)
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct Sha2_256GasParameters {
    pub base: InternalGas,
    pub per_byte: InternalGasPerByte,
    pub legacy_min_input_len: NumBytes,
}

#[inline]
fn native_sha2_256(
    gas_params: &Sha2_256GasParameters,
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(arguments.len() == 1);

    let hash_arg = pop_arg!(arguments, Vec<u8>);

    let cost = gas_params.base
        + gas_params.per_byte
            * std::cmp::max(
                NumBytes::new(hash_arg.len() as u64),
                gas_params.legacy_min_input_len,
            );
    // Charge before doing work
    native_charge_gas_early_exit!(context, cost);

    let hash_vec = Sha256::digest(hash_arg.as_slice()).to_vec();
    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::vector_u8(hash_vec)],
    ))
}

pub fn make_native_sha2_256(gas_params: Sha2_256GasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_sha2_256(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun sha3_256
 *
 *   gas cost: base_cost + unit_cost * max(input_length_in_bytes, legacy_min_input_len)
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct Sha3_256GasParameters {
    pub base: InternalGas,
    pub per_byte: InternalGasPerByte,
    pub legacy_min_input_len: NumBytes,
}

#[inline]
fn native_sha3_256(
    gas_params: &Sha3_256GasParameters,
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(arguments.len() == 1);

    let hash_arg = pop_arg!(arguments, Vec<u8>);

    let cost = gas_params.base
        + gas_params.per_byte
            * std::cmp::max(
                NumBytes::new(hash_arg.len() as u64),
                gas_params.legacy_min_input_len,
            );
    // Charge before doing work
    native_charge_gas_early_exit!(context, cost);

    let hash_vec = Sha3_256::digest(hash_arg.as_slice()).to_vec();
    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::vector_u8(hash_vec)],
    ))
}

pub fn make_native_sha3_256(gas_params: Sha3_256GasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_sha3_256(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * module
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub sha2_256: Sha2_256GasParameters,
    pub sha3_256: Sha3_256GasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [
        ("sha2_256", make_native_sha2_256(gas_params.sha2_256)),
        ("sha3_256", make_native_sha3_256(gas_params.sha3_256)),
    ];

    make_module_natives(natives)
}
