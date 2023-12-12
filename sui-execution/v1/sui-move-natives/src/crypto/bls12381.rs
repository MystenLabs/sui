// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use fastcrypto::{
    bls12381::{min_pk, min_sig},
    traits::{ToFromBytes, VerifyingKey},
};
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

use crate::NativesCostTable;

const BLS12381_BLOCK_SIZE: usize = 64;

#[derive(Clone)]
pub struct Bls12381Bls12381MinSigVerifyCostParams {
    /// Base cost for invoking the `bls12381_min_sig_verify` function
    pub bls12381_bls12381_min_sig_verify_cost_base: InternalGas,
    /// Cost per byte of `msg`
    pub bls12381_bls12381_min_sig_verify_msg_cost_per_byte: InternalGas,
    /// Cost per block of `msg`, where a block is 64 bytes
    pub bls12381_bls12381_min_sig_verify_msg_cost_per_block: InternalGas,
}
/***************************************************************************************************
 * native fun bls12381_min_sig_verify
 * Implementation of the Move native function `bls12381_min_sig_verify(signature: &vector<u8>, public_key: &vector<u8>, msg: &vector<u8>): bool`
 *   gas cost: bls12381_bls12381_min_sig_verify_cost_base                    | covers various fixed costs in the oper
 *              + bls12381_bls12381_min_sig_verify_msg_cost_per_byte    * size_of(msg)        | covers cost of operating on each byte of `msg`
 *              + bls12381_bls12381_min_sig_verify_msg_cost_per_block   * num_blocks(msg)     | covers cost of operating on each block in `msg`
 * Note: each block is of size `BLS12381_BLOCK_SIZE` bytes, and we round up.
 *       `signature` and `public_key` are fixed size, so their costs are included in the base cost.
 **************************************************************************************************/
pub fn bls12381_min_sig_verify(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    // Load the cost parameters from the protocol config
    let bls12381_bls12381_min_sig_verify_cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .bls12381_bls12381_min_sig_verify_cost_params
        .clone();
    // Charge the base cost for this oper
    native_charge_gas_early_exit!(
        context,
        bls12381_bls12381_min_sig_verify_cost_params.bls12381_bls12381_min_sig_verify_cost_base
    );

    let msg = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let signature_bytes = pop_arg!(args, VectorRef);

    let msg_ref = msg.as_bytes_ref();
    let public_key_bytes_ref = public_key_bytes.as_bytes_ref();
    let signature_bytes_ref = signature_bytes.as_bytes_ref();

    // Charge the arg size dependent costs
    native_charge_gas_early_exit!(
        context,
        bls12381_bls12381_min_sig_verify_cost_params
            .bls12381_bls12381_min_sig_verify_msg_cost_per_byte
            * (msg_ref.len() as u64).into()
            + bls12381_bls12381_min_sig_verify_cost_params
                .bls12381_bls12381_min_sig_verify_msg_cost_per_block
                * (((msg_ref.len() + BLS12381_BLOCK_SIZE - 1) / BLS12381_BLOCK_SIZE) as u64).into()
    );

    let cost = context.gas_used();

    let Ok(signature) =
        <min_sig::BLS12381Signature as ToFromBytes>::from_bytes(&signature_bytes_ref)
    else {
        return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)]));
    };

    let public_key =
        match <min_sig::BLS12381PublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
            Ok(public_key) => match public_key.validate() {
                Ok(_) => public_key,
                Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
            },
            Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        };

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::bool(public_key.verify(&msg_ref, &signature).is_ok())],
    ))
}

#[derive(Clone)]
pub struct Bls12381Bls12381MinPkVerifyCostParams {
    /// Base cost for invoking the `bls12381_min_sig_verify` function
    pub bls12381_bls12381_min_pk_verify_cost_base: InternalGas,
    /// Cost per byte of `msg`
    pub bls12381_bls12381_min_pk_verify_msg_cost_per_byte: InternalGas,
    /// Cost per block of `msg`, where a block is 64 bytes
    pub bls12381_bls12381_min_pk_verify_msg_cost_per_block: InternalGas,
}
/***************************************************************************************************
 * native fun bls12381_min_pk_verify
 * Implementation of the Move native function `bls12381_min_pk_verify(signature: &vector<u8>, public_key: &vector<u8>, msg: &vector<u8>): bool`
 *   gas cost: bls12381_bls12381_min_pk_verify_cost_base                    | covers various fixed costs in the oper
 *              + bls12381_bls12381_min_pk_verify_msg_cost_per_byte    * size_of(msg)        | covers cost of operating on each byte of `msg`
 *              + bls12381_bls12381_min_pk_verify_msg_cost_per_block   * num_blocks(msg)     | covers cost of operating on each block in `msg`
 * Note: each block is of size `BLS12381_BLOCK_SIZE` bytes, and we round up.
 *       `signature` and `public_key` are fixed size, so their costs are included in the base cost.
 **************************************************************************************************/
pub fn bls12381_min_pk_verify(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    // Load the cost parameters from the protocol config
    let bls12381_bls12381_min_pk_verify_cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .bls12381_bls12381_min_pk_verify_cost_params
        .clone();

    // Charge the base cost for this oper
    native_charge_gas_early_exit!(
        context,
        bls12381_bls12381_min_pk_verify_cost_params.bls12381_bls12381_min_pk_verify_cost_base
    );

    let msg = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let signature_bytes = pop_arg!(args, VectorRef);

    let msg_ref = msg.as_bytes_ref();
    let public_key_bytes_ref = public_key_bytes.as_bytes_ref();
    let signature_bytes_ref = signature_bytes.as_bytes_ref();

    // Charge the arg size dependent costs
    native_charge_gas_early_exit!(
        context,
        bls12381_bls12381_min_pk_verify_cost_params
            .bls12381_bls12381_min_pk_verify_msg_cost_per_byte
            * (msg_ref.len() as u64).into()
            + bls12381_bls12381_min_pk_verify_cost_params
                .bls12381_bls12381_min_pk_verify_msg_cost_per_block
                * (((msg_ref.len() + BLS12381_BLOCK_SIZE - 1) / BLS12381_BLOCK_SIZE) as u64).into()
    );

    let cost = context.gas_used();

    let signature =
        match <min_pk::BLS12381Signature as ToFromBytes>::from_bytes(&signature_bytes_ref) {
            Ok(signature) => signature,
            Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        };

    let public_key =
        match <min_pk::BLS12381PublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
            Ok(public_key) => match public_key.validate() {
                Ok(_) => public_key,
                Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
            },
            Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        };

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::bool(public_key.verify(&msg_ref, &signature).is_ok())],
    ))
}
