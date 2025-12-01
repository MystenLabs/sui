// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::helpers::make_module_natives;
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::{
    native_charge_gas_early_exit,
    native_functions::{NativeContext, NativeFunction},
};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use smallvec::smallvec;
use std::{collections::VecDeque, sync::Arc};

#[derive(Debug, Clone)]
pub struct PoisonGasParameters {
    pub base_cost: InternalGas,
}

#[derive(Debug, Clone)]
pub struct DestroyGasParameters {
    pub base: InternalGas,
}

fn native_poison(
    gas_params: &PoisonGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());
    let cost = gas_params.base_cost;
    native_charge_gas_early_exit!(context, cost);
    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

pub fn make_native_poison(gas_params: PoisonGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_poison(&gas_params, context, ty_args, args)
        },
    )
}

fn native_destroy(
    gas_params: &DestroyGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(!ty_args.is_empty());
    debug_assert!(!args.is_empty());

    native_charge_gas_early_exit!(context, gas_params.base);

    // let value = pop_arg!(args, Value);
    // value.destroy(&ty_args[0]);

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}

pub fn make_native_destroy(gas_params: DestroyGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_destroy(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * module
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub poison: PoisonGasParameters,
    pub destroy: DestroyGasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [
        ("poison", make_native_poison(gas_params.poison)),
        ("destroy", make_native_destroy(gas_params.destroy)),
    ];

    make_module_natives(natives)
}
