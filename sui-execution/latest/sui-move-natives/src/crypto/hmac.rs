// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::NativesCostTable;
use fastcrypto::{hmac, traits::ToFromBytes};
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

const HMAC_SHA3_256_BLOCK_SIZE: usize = 136;

#[derive(Clone)]
pub struct HmacHmacSha3256CostParams {
    /// Base cost for invoking the `hmac_sha3_256` function
    pub hmac_hmac_sha3_256_cost_base: InternalGas,
    ///  Cost per byte of `msg` and `key`
    pub hmac_hmac_sha3_256_input_cost_per_byte: InternalGas,
    ///  Cost per block of `msg` and `key`, with block size = 136
    pub hmac_hmac_sha3_256_input_cost_per_block: InternalGas,
}
/***************************************************************************************************
 * native fun ed25519_verify
 * Implementation of the Move native function `hmac_sha3_256(key: &vector<u8>, msg: &vector<u8>): vector<u8>;`
 *   gas cost: hmac_hmac_sha3_256_cost_base                          | base cost for function call and fixed opers
 *              + hmac_hmac_sha3_256_input_cost_per_byte * msg.len()   | cost depends on length of message
 *              + hmac_hmac_sha3_256_input_cost_per_block * num_blocks(msg) | cost depends on number of blocks in message
 * Note: each block is of size `HMAC_SHA3_256_BLOCK_SIZE` bytes, and we round up.
 *       `key` is fixed size, so the cost is included in the base cost.
 **************************************************************************************************/
pub fn hmac_sha3_256(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    // Load the cost parameters from the protocol config
    let hmac_hmac_sha3_256_cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .hmac_hmac_sha3_256_cost_params
        .clone();

    // Charge the base cost for this operation
    native_charge_gas_early_exit!(
        context,
        hmac_hmac_sha3_256_cost_params.hmac_hmac_sha3_256_cost_base
    );

    let message = pop_arg!(args, VectorRef);
    let key = pop_arg!(args, VectorRef);

    let msg_len = message.as_bytes_ref().len();
    let key_len = key.as_bytes_ref().len();
    // Charge the arg size dependent costs
    native_charge_gas_early_exit!(
        context,
        hmac_hmac_sha3_256_cost_params.hmac_hmac_sha3_256_input_cost_per_byte
            // same cost for msg and key
            * ((msg_len + key_len) as u64).into()
            + hmac_hmac_sha3_256_cost_params.hmac_hmac_sha3_256_input_cost_per_block
                * ((((msg_len + key_len) + (2 * HMAC_SHA3_256_BLOCK_SIZE - 2))
                    / HMAC_SHA3_256_BLOCK_SIZE) as u64)
                    .into()
    );

    let hmac_key = hmac::HmacKey::from_bytes(&key.as_bytes_ref())
        .expect("HMAC key can be of any length and from_bytes should always succeed");
    let cost = context.gas_used();

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(
            hmac::hmac_sha3_256(&hmac_key, &message.as_bytes_ref()).to_vec()
        )],
    ))
}
