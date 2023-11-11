// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::natives::helpers::make_module_natives;
use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    account_address::AccountAddress,
    gas_algebra::{InternalGas, InternalGasPerArg, NumArgs},
};
use move_vm_runtime::{
    native_charge_gas_early_exit,
    native_functions::{NativeContext, NativeFunction},
};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::{collections::VecDeque, sync::Arc};

/***************************************************************************************************
 * native fun create_signers_for_testing
 *
 *   gas cost: base_cost + unit_cost * num_of_signers
 *
 **************************************************************************************************/
fn to_le_bytes(i: u64) -> [u8; AccountAddress::LENGTH] {
    let bytes = i.to_le_bytes();
    let mut result = [0u8; AccountAddress::LENGTH];
    result[..bytes.len()].clone_from_slice(bytes.as_ref());
    result
}

#[derive(Debug, Clone)]
pub struct CreateSignersForTestingGasParameters {
    pub base_cost: InternalGas,
    pub unit_cost: InternalGasPerArg,
}

fn native_create_signers_for_testing(
    gas_params: &CreateSignersForTestingGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let num_signers = pop_arg!(args, u64);
    let signers = Value::vector_for_testing_only(
        (0..num_signers).map(|i| Value::signer(AccountAddress::new(to_le_bytes(i)))),
    );

    let cost = gas_params.base_cost + gas_params.unit_cost * NumArgs::new(num_signers);
    native_charge_gas_early_exit!(context, cost);

    Ok(NativeResult::ok(context.gas_used(), smallvec![signers]))
}

pub fn make_native_create_signers_for_testing(
    gas_params: CreateSignersForTestingGasParameters,
) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_create_signers_for_testing(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * module
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub create_signers_for_testing: CreateSignersForTestingGasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [(
        "create_signers_for_testing",
        make_native_create_signers_for_testing(gas_params.create_signers_for_testing),
    )];

    make_module_natives(natives)
}
