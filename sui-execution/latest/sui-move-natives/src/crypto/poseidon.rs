// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::NativesCostTable;
use fastcrypto_zkp::bn254::poseidon::hash_to_bytes;
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_core_types::u256::U256;
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::natives::function::PartialVMError;
use move_vm_types::values::{Reference, VMValueCast};
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

#[derive(Clone)]
pub struct PoseidonBN254CostParams {
    /// Base cost for invoking the `poseidon_bn254` function
    pub poseidon_bn254_cost_base: Option<InternalGas>,
    /// Cost per block of `data`, where a block is 32 bytes
    pub poseidon_bn254_data_cost_per_block: Option<InternalGas>,
}

/***************************************************************************************************
 * native fun poseidon_bn254
 * Implementation of the Move native function `poseidon::poseidon_bn254(data: &vector<u256>): u256
 *   gas cost: poseidon_bn254_cost_base                           | base cost for function call and fixed opers
 *              + poseidon_bn254_data_cost_per_block * num_inputs | cost depends on number of inputs
 **************************************************************************************************/
pub fn poseidon_bn254(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
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
    let length = inputs.len(&Type::U256)?.value_as::<u64>()?;

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

    // Read the input vector and convert each element to a field element in le representation
    let mut field_elements: Vec<Vec<u8>> = Vec::new();
    for i in 0..length {
        let reference: Reference = inputs.borrow_elem(i as usize, &Type::U256)?.cast()?;
        let value = reference.read_ref()?.value_as::<U256>()?;
        field_elements.push(value.to_le_bytes().to_vec());
    }

    match hash_to_bytes(&field_elements) {
        Ok(hash) => {
            let result = U256::from_le_bytes(&hash);
            Ok(NativeResult::ok(
                context.gas_used(),
                smallvec![Value::u256(result)],
            ))
        }
        Err(_) => Ok(NativeResult::err(context.gas_used(), NON_CANONICAL_INPUT)),
    }
}
