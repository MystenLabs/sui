// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::object_runtime::ObjectRuntime;
use crate::NativesCostTable;
use fastcrypto_vdf::class_group::discriminant::Discriminant;
use fastcrypto_vdf::class_group::QuadraticForm;
use fastcrypto_vdf::vdf::wesolowski::StrongVDF;
use fastcrypto_vdf::vdf::wesolowski::StrongVDFVerifier;
use fastcrypto_vdf::ToBytes;
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

pub const INVALID_INPUT_ERROR: u64 = 0;
pub const NOT_SUPPORTED_ERROR: u64 = 1;

fn is_supported(context: &NativeContext) -> bool {
    context
        .extensions()
        .get::<ObjectRuntime>()
        .protocol_config
        .enable_vdf()
}

#[derive(Clone)]
pub struct VDFCostParams {
    pub vdf_verify_cost: Option<InternalGas>,
    pub hash_to_input_cost: Option<InternalGas>,
}

/***************************************************************************************************
 * native fun verify_vdf
 *
 * Implementation of the Move native function `vdf::verify_vdf_internal(
 *      discriminant: &vector<u8>,
 *      input: &vector<u8>,
 *      output: &vector<u8>,
 *      proof: &vector<u8>,
 *      iterations: u64): bool`
 *
 * Gas cost: verify_vdf_cost
 **************************************************************************************************/
pub fn vdf_verify_internal(
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
        .vdf_cost_params
        .clone();

    // Charge the base cost for this operation
    native_charge_gas_early_exit!(
        context,
        cost_params
            .vdf_verify_cost
            .ok_or_else(
                || PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for vdf_verify not available".to_string())
            )?
    );

    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 5);

    // The input is a reference to a vector of vector<u8>'s
    let iterations = pop_arg!(args, u64);
    let proof_bytes = pop_arg!(args, VectorRef);
    let output_bytes = pop_arg!(args, VectorRef);
    let input_bytes = pop_arg!(args, VectorRef);
    let discriminant_bytes = pop_arg!(args, VectorRef);

    let discriminant = match Discriminant::try_from_be_bytes(&discriminant_bytes.as_bytes_ref()) {
        Ok(discriminant) => discriminant,
        Err(_) => return Ok(NativeResult::err(context.gas_used(), INVALID_INPUT_ERROR)),
    };

    let input = match QuadraticForm::from_bytes(&input_bytes.as_bytes_ref(), &discriminant) {
        Ok(input) => input,
        Err(_) => return Ok(NativeResult::err(context.gas_used(), INVALID_INPUT_ERROR)),
    };

    let proof = match QuadraticForm::from_bytes(&proof_bytes.as_bytes_ref(), &discriminant) {
        Ok(proof) => proof,
        Err(_) => return Ok(NativeResult::err(context.gas_used(), INVALID_INPUT_ERROR)),
    };

    let output = match QuadraticForm::from_bytes(&output_bytes.as_bytes_ref(), &discriminant) {
        Ok(output) => output,
        Err(_) => return Ok(NativeResult::err(context.gas_used(), INVALID_INPUT_ERROR)),
    };

    let vdf = StrongVDF::new(discriminant, iterations);
    let fast_verifier = StrongVDFVerifier::new(vdf, input);

    match fast_verifier.verify(&output, &proof) {
        Ok(_) => Ok(NativeResult::ok(
            context.gas_used(),
            smallvec![Value::bool(true)],
        )),
        Err(_) => Ok(NativeResult::ok(
            context.gas_used(),
            smallvec![Value::bool(false)],
        )),
    }
}

/***************************************************************************************************
 * native fun verify_vdf
 *
 * Implementation of the Move native function `vdf::verify_vdf_internal(
 *      discriminant: &vector<u8>,
 *      input: &vector<u8>,
 *      output: &vector<u8>,
 *      proof: &vector<u8>,
 *      iterations: u64): bool`
 *
 * Gas cost: verify_vdf_cost
 **************************************************************************************************/
pub fn hash_to_input_internal(
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
        .vdf_cost_params
        .clone();

    // Charge the base cost for this operation
    native_charge_gas_early_exit!(
        context,
        cost_params
            .hash_to_input_cost
            .ok_or_else(
                || PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Gas cost for hash_to_input not available".to_string())
            )?
    );

    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let message = pop_arg!(args, VectorRef);
    let discriminant_bytes = pop_arg!(args, VectorRef);

    let discriminant = match Discriminant::try_from_be_bytes(&discriminant_bytes.as_bytes_ref()) {
        Ok(discriminant) => discriminant,
        Err(_) => return Ok(NativeResult::err(context.gas_used(), INVALID_INPUT_ERROR)),
    };

    let output = match QuadraticForm::hash_to_group_with_default_parameters(
        &message.as_bytes_ref(),
        &discriminant,
    ) {
        Ok(output) => output,
        Err(_) => return Ok(NativeResult::err(context.gas_used(), INVALID_INPUT_ERROR)),
    };

    let output_bytes = output.to_bytes();

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::vector_u8(output_bytes)],
    ))
}
