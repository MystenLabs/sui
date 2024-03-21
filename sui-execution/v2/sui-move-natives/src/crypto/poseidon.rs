// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::object_runtime::ObjectRuntime;
use crate::NativesCostTable;
use fastcrypto_zkp::bn254::poseidon::poseidon_bytes;
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::natives::function::PartialVMError;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Value, VectorRef},
};
use smallvec::smallvec;
use std::collections::VecDeque;
use std::ops::Mul;

pub const NON_CANONICAL_INPUT: u64 = 0;
pub const NOT_SUPPORTED_ERROR: u64 = 1;

fn is_supported(context: &NativeContext) -> bool {
    context
        .extensions()
        .get::<ObjectRuntime>()
        .protocol_config
        .enable_poseidon()
}

#[derive(Clone)]
pub struct PoseidonBN254CostParams {
    /// Base cost for invoking the `poseidon_bn254` function
    pub poseidon_bn254_cost_base: Option<InternalGas>,
    /// Cost per block of `data`, where a block is 32 bytes
    pub poseidon_bn254_data_cost_per_block: Option<InternalGas>,
}

/***************************************************************************************************
 * native fun poseidon_bn254
 * Implementation of the Move native function `poseidon::poseidon_bn254_internal(data: &vector<vector<u8>>): vector<u8>
 *   gas cost: poseidon_bn254_cost_base                           | base cost for function call and fixed opers
 *              + poseidon_bn254_data_cost_per_block * num_inputs | cost depends on number of inputs
 **************************************************************************************************/
pub fn poseidon_bn254_internal(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    let cost = context.gas_used();
    if !is_supported(context) {
        return Ok(NativeResult::err(cost, NOT_SUPPORTED_ERROR));
    }

    // Load the cost parameters from the protocol config
    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .poseidon_bn254_cost_params
        .clone();

    // Charge the base cost for this operation
    native_charge_gas_early_exit!(
        context,
        cost_params
            .poseidon_bn254_cost_base
            .ok_or_else(
                || PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for poseidon_bn254 not available".to_string())
            )?
    );

    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    // The input is a reference to a vector of vector<u8>'s
    let inputs = pop_arg!(args, VectorRef);

    let length = inputs
        .len(&Type::Vector(Box::new(Type::U8)))?
        .value_as::<u64>()?;

    // Charge the msg dependent costs
    native_charge_gas_early_exit!(
        context,
        cost_params
            .poseidon_bn254_data_cost_per_block
            .ok_or_else(
                || PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for poseidon_bn254 not available".to_string())
            )?
            .mul(length.into())
    );

    // Read the input vector
    let field_elements = (0..length)
        .map(|i| {
            let reference = inputs.borrow_elem(i as usize, &Type::Vector(Box::new(Type::U8)))?;
            let value = reference.value_as::<VectorRef>()?.as_bytes_ref().clone();
            Ok(value)
        })
        .collect::<Result<Vec<_>, _>>()?;

    match poseidon_bytes(&field_elements) {
        Ok(result) => Ok(NativeResult::ok(
            context.gas_used(),
            smallvec![Value::vector_u8(result)],
        )),
        // This is also checked in the poseidon_bn254 move function but to be sure we handle it here also.
        Err(_) => Ok(NativeResult::err(context.gas_used(), NON_CANONICAL_INPUT)),
    }
}
