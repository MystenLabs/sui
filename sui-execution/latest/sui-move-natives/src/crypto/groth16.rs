// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{object_runtime::ObjectRuntime, NativesCostTable};
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{self, Value, VectorRef},
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub const INVALID_VERIFYING_KEY: u64 = 0;
pub const INVALID_CURVE: u64 = 1;
pub const TOO_MANY_PUBLIC_INPUTS: u64 = 2;

// These must match the corresponding values in sui::groth16::Curve.
pub const BLS12381: u8 = 0;
pub const BN254: u8 = 1;

// We need to set an upper bound on the number of public inputs to avoid a DoS attack
pub const MAX_PUBLIC_INPUTS: usize = 8;

#[derive(Clone)]
pub struct Groth16PrepareVerifyingKeyCostParams {
    pub groth16_prepare_verifying_key_bls12381_cost_base: InternalGas,
    pub groth16_prepare_verifying_key_bn254_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun prepare_verifying_key_internal
 * Implementation of the Move native function `prepare_verifying_key_internal(curve: u8, verifying_key: &vector<u8>): PreparedVerifyingKey`
 * This function has two cost modes depending on the curve being set to `BLS12381` or `BN254`. The core formula is same but constants differ.
 * If curve = 0, we use the `bls12381` cost constants, otherwise we use the `bn254` cost constants.
 *   gas cost: groth16_prepare_verifying_key_cost_base                    | covers various fixed costs in the oper
 * Note: `curve` and `verifying_key` are fixed size, so their costs are included in the base cost.
 **************************************************************************************************/
pub fn prepare_verifying_key_internal(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    // Load the cost parameters from the protocol config
    let (groth16_prepare_verifying_key_cost_params, crypto_invalid_arguments_cost) = {
        let cost_table = &context.extensions().get::<NativesCostTable>();
        (
            cost_table.groth16_prepare_verifying_key_cost_params.clone(),
            cost_table.crypto_invalid_arguments_cost,
        )
    };
    let bytes = pop_arg!(args, VectorRef);
    let verifying_key = bytes.as_bytes_ref();

    let curve = pop_arg!(args, u8);

    // Load the cost parameters from the protocol config
    let base_cost = match curve {
        BLS12381 => {
            groth16_prepare_verifying_key_cost_params
                .groth16_prepare_verifying_key_bls12381_cost_base
        }
        BN254 => {
            groth16_prepare_verifying_key_cost_params.groth16_prepare_verifying_key_bn254_cost_base
        }
        _ => {
            // Charge for failure but dont fail if we run out of gas otherwise the actual error is masked by OUT_OF_GAS error
            context.charge_gas(crypto_invalid_arguments_cost);
            return Ok(NativeResult::err(context.gas_used(), INVALID_CURVE));
        }
    };
    // Charge the base cost for this oper
    native_charge_gas_early_exit!(context, base_cost);
    let cost = context.gas_used();

    let result;
    if curve == BLS12381 {
        result = fastcrypto_zkp::bls12381::api::prepare_pvk_bytes(&verifying_key);
    } else if curve == BN254 {
        result = fastcrypto_zkp::bn254::api::prepare_pvk_bytes(&verifying_key);
    } else {
        return Ok(NativeResult::err(cost, INVALID_CURVE));
    }

    match result {
        Ok(pvk) => Ok(NativeResult::ok(
            cost,
            smallvec![Value::struct_(values::Struct::pack(vec![
                Value::vector_u8(pvk[0].to_vec()),
                Value::vector_u8(pvk[1].to_vec()),
                Value::vector_u8(pvk[2].to_vec()),
                Value::vector_u8(pvk[3].to_vec())
            ]))],
        )),
        Err(_) => Ok(NativeResult::err(cost, INVALID_VERIFYING_KEY)),
    }
}

#[derive(Clone)]
pub struct Groth16VerifyGroth16ProofInternalCostParams {
    pub groth16_verify_groth16_proof_internal_bls12381_cost_base: InternalGas,
    pub groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input: InternalGas,

    pub groth16_verify_groth16_proof_internal_bn254_cost_base: InternalGas,
    pub groth16_verify_groth16_proof_internal_bn254_cost_per_public_input: InternalGas,

    pub groth16_verify_groth16_proof_internal_public_input_cost_per_byte: InternalGas,
}
/***************************************************************************************************
 * native fun verify_groth16_proof_internal
 * Implementation of the Move native function `verify_groth16_proof_internal(curve: u8, vk_gamma_abc_g1_bytes: &vector<u8>,
 *                          alpha_g1_beta_g2_bytes: &vector<u8>, gamma_g2_neg_pc_bytes: &vector<u8>, delta_g2_neg_pc_bytes: &vector<u8>,
 *                          public_proof_inputs: &vector<u8>, proof_points: &vector<u8>): bool`
 *
 * This function has two cost modes depending on the curve being set to `BLS12381` or `BN254`. The core formula is same but constants differ.
 * If curve = 0, we use the `bls12381` cost constants, otherwise we use the `bn254` cost constants.
 *   gas cost: groth16_prepare_verifying_key_cost_base                    | covers various fixed costs in the oper
 *              + groth16_verify_groth16_proof_internal_public_input_cost_per_byte
 *                                                   * size_of(public_proof_inputs) | covers the cost of verifying each public input per byte
 *              + groth16_verify_groth16_proof_internal_cost_per_public_input
 *                                                   * num_public_inputs) | covers the cost of verifying each public input per input
 * Note: every other arg is fixed size, so their costs are included in the base cost.
 **************************************************************************************************/
pub fn verify_groth16_proof_internal(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 7);

    // Load the cost parameters from the protocol config
    let (groth16_verify_groth16_proof_internal_cost_params, crypto_invalid_arguments_cost) = {
        let cost_table = &context.extensions().get::<NativesCostTable>();
        (
            cost_table
                .groth16_verify_groth16_proof_internal_cost_params
                .clone(),
            cost_table.crypto_invalid_arguments_cost,
        )
    };
    let bytes5 = pop_arg!(args, VectorRef);
    let proof_points = bytes5.as_bytes_ref();

    let bytes4 = pop_arg!(args, VectorRef);
    let public_proof_inputs = bytes4.as_bytes_ref();

    let bytes3 = pop_arg!(args, VectorRef);
    let delta_g2_neg_pc = bytes3.as_bytes_ref();

    let bytes2 = pop_arg!(args, VectorRef);
    let gamma_g2_neg_pc = bytes2.as_bytes_ref();

    let byte1 = pop_arg!(args, VectorRef);
    let alpha_g1_beta_g2 = byte1.as_bytes_ref();

    let bytes = pop_arg!(args, VectorRef);
    let vk_gamma_abc_g1 = bytes.as_bytes_ref();

    let curve = pop_arg!(args, u8);

    let (base_cost, cost_per_public_input, num_public_inputs) = match curve {
        BLS12381 => (
            groth16_verify_groth16_proof_internal_cost_params
                .groth16_verify_groth16_proof_internal_bls12381_cost_base,
            groth16_verify_groth16_proof_internal_cost_params
                .groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input,
            (public_proof_inputs.len() + fastcrypto::groups::bls12381::SCALAR_LENGTH - 1)
                / fastcrypto::groups::bls12381::SCALAR_LENGTH,
        ),
        BN254 => (
            groth16_verify_groth16_proof_internal_cost_params
                .groth16_verify_groth16_proof_internal_bn254_cost_base,
            groth16_verify_groth16_proof_internal_cost_params
                .groth16_verify_groth16_proof_internal_bn254_cost_per_public_input,
            (public_proof_inputs.len() + fastcrypto_zkp::bn254::api::SCALAR_SIZE - 1)
                / fastcrypto_zkp::bn254::api::SCALAR_SIZE,
        ),
        _ => {
            // Charge for failure but dont fail if we run out of gas otherwise the actual error is masked by OUT_OF_GAS error
            context.charge_gas(crypto_invalid_arguments_cost);
            let cost = if context
                .extensions()
                .get::<ObjectRuntime>()
                .protocol_config
                .native_charging_v2()
            {
                context.gas_used()
            } else {
                context.gas_budget()
            };
            return Ok(NativeResult::err(cost, INVALID_CURVE));
        }
    };
    // Charge the base cost for this oper
    native_charge_gas_early_exit!(context, base_cost);
    // Charge the arg size dependent costs
    native_charge_gas_early_exit!(
        context,
        cost_per_public_input * (num_public_inputs as u64).into()
            + groth16_verify_groth16_proof_internal_cost_params
                .groth16_verify_groth16_proof_internal_public_input_cost_per_byte
                * (public_proof_inputs.len() as u64).into()
    );

    let cost = context.gas_used();

    let result;
    if curve == BLS12381 {
        if public_proof_inputs.len()
            > fastcrypto::groups::bls12381::SCALAR_LENGTH * MAX_PUBLIC_INPUTS
        {
            return Ok(NativeResult::err(cost, TOO_MANY_PUBLIC_INPUTS));
        }
        result = fastcrypto_zkp::bls12381::api::verify_groth16_in_bytes(
            &vk_gamma_abc_g1,
            &alpha_g1_beta_g2,
            &gamma_g2_neg_pc,
            &delta_g2_neg_pc,
            &public_proof_inputs,
            &proof_points,
        );
    } else if curve == BN254 {
        if public_proof_inputs.len() > fastcrypto_zkp::bn254::api::SCALAR_SIZE * MAX_PUBLIC_INPUTS {
            return Ok(NativeResult::err(cost, TOO_MANY_PUBLIC_INPUTS));
        }
        result = fastcrypto_zkp::bn254::api::verify_groth16_in_bytes(
            &vk_gamma_abc_g1,
            &alpha_g1_beta_g2,
            &gamma_g2_neg_pc,
            &delta_g2_neg_pc,
            &public_proof_inputs,
            &proof_points,
        );
    } else {
        return Ok(NativeResult::err(cost, INVALID_CURVE));
    }

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::bool(result.unwrap_or(false))],
    ))
}
