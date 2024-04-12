// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::natives::helpers::make_module_natives;
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::{
    native_charge_gas_early_exit,
    native_functions::{NativeContext, NativeFunction},
};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{values_impl::SignerRef, Value},
};
use smallvec::smallvec;
use std::{collections::VecDeque, sync::Arc};

/***************************************************************************************************
 * native fun borrow_address
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct BorrowAddressGasParameters {
    pub base: InternalGas,
}

#[inline]
fn native_borrow_address(
    gas_params: &BorrowAddressGasParameters,
    context: &mut NativeContext,
    _ty_args: Vec<Type>,
    mut arguments: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(arguments.len() == 1);

    native_charge_gas_early_exit!(context, gas_params.base);
    let signer_reference = pop_arg!(arguments, SignerRef);

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![signer_reference.borrow_signer()?],
    ))
}

pub fn make_native_borrow_address(gas_params: BorrowAddressGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_borrow_address(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * module
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub borrow_address: BorrowAddressGasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [(
        "borrow_address",
        make_native_borrow_address(gas_params.borrow_address),
    )];

    make_module_natives(natives)
}
