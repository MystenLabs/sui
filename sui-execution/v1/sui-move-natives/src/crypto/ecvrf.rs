// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::NativesCostTable;
use fastcrypto::vrf::ecvrf::{ECVRFProof, ECVRFPublicKey};
use fastcrypto::vrf::VRFProof;
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Value, VectorRef},
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub const INVALID_ECVRF_HASH_LENGTH: u64 = 1;
pub const INVALID_ECVRF_PUBLIC_KEY: u64 = 2;
pub const INVALID_ECVRF_PROOF: u64 = 3;

const ECVRF_SHA512_BLOCK_SIZE: usize = 128;

#[derive(Clone)]
pub struct EcvrfEcvrfVerifyCostParams {
    /// Base cost for invoking the `ecvrf_verify`
    pub ecvrf_ecvrf_verify_cost_base: InternalGas,
    ///  Cost per byte of `alpha_string`
    pub ecvrf_ecvrf_verify_alpha_string_cost_per_byte: InternalGas,
    ///  Cost per block of `alpha_string` with block size = 128
    pub ecvrf_ecvrf_verify_alpha_string_cost_per_block: InternalGas,
}
/***************************************************************************************************
 * native fun ecvrf_verify
 * Implementation of the Move native function `ecvrf_verify(hash: &vector<u8>, alpha_string: &vector<u8>, public_key: &vector<u8>, proof: &vector<u8>): bool`
 *   gas cost: ecvrf_ecvrf_verify_cost_base                    | covers various fixed costs in the oper
 *              + ecvrf_ecvrf_verify_alpha_string_cost_per_byte    * size_of(alpha_string)        | covers cost of operating on each byte of `alpha_string`
 *              + ecvrf_ecvrf_verify_alpha_string_cost_per_block   * num_blocks(alpha_string)     | covers cost of operating on each block in `alpha_string`
 * Note: each block is of size `ECVRF_SHA512_BLOCK_SIZE` bytes, and we round up.
 *       `hash`, `proof`, and `public_key` are fixed size, so their costs are included in the base cost.
 **************************************************************************************************/
pub fn ecvrf_verify(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 4);

    // Load the cost parameters from the protocol config
    let ecvrf_ecvrf_verify_cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .ecvrf_ecvrf_verify_cost_params
        .clone();
    // Charge the base cost for this oper
    native_charge_gas_early_exit!(
        context,
        ecvrf_ecvrf_verify_cost_params.ecvrf_ecvrf_verify_cost_base
    );

    let proof_bytes = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let alpha_string = pop_arg!(args, VectorRef);
    let hash_bytes = pop_arg!(args, VectorRef);

    let alpha_string_len = alpha_string.as_bytes_ref().len();
    // Charge the arg size dependent costs
    native_charge_gas_early_exit!(
        context,
        ecvrf_ecvrf_verify_cost_params.ecvrf_ecvrf_verify_alpha_string_cost_per_byte
            * (alpha_string_len as u64).into()
            + ecvrf_ecvrf_verify_cost_params.ecvrf_ecvrf_verify_alpha_string_cost_per_block
                * (((alpha_string_len + ECVRF_SHA512_BLOCK_SIZE - 1) / ECVRF_SHA512_BLOCK_SIZE)
                    as u64)
                    .into()
    );

    let cost = context.gas_used();

    let Ok(hash) = hash_bytes.as_bytes_ref().as_slice().try_into() else {
        return Ok(NativeResult::err(cost, INVALID_ECVRF_HASH_LENGTH));
    };

    let Ok(public_key) =
        bcs::from_bytes::<ECVRFPublicKey>(public_key_bytes.as_bytes_ref().as_slice())
    else {
        return Ok(NativeResult::err(cost, INVALID_ECVRF_PUBLIC_KEY));
    };

    let Ok(proof) = bcs::from_bytes::<ECVRFProof>(proof_bytes.as_bytes_ref().as_slice()) else {
        return Ok(NativeResult::err(cost, INVALID_ECVRF_PROOF));
    };

    let result = proof.verify_output(alpha_string.as_bytes_ref().as_slice(), &public_key, &hash);
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::bool(result.is_ok())],
    ))
}
