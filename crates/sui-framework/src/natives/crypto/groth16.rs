// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use fastcrypto_zkp::bls12381::api::{prepare_pvk_bytes, verify_groth16_in_bytes};
use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{self, Value, VectorRef},
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub const INVALID_VERIFYING_KEY: u64 = 0;

pub fn prepare_verifying_key(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let bytes = pop_arg!(args, VectorRef);
    let verifying_key = bytes.as_bytes_ref();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    match prepare_pvk_bytes(&verifying_key) {
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

pub fn verify_groth16_proof_internal(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 6);

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

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    match verify_groth16_in_bytes(
        &vk_gamma_abc_g1,
        &alpha_g1_beta_g2,
        &gamma_g2_neg_pc,
        &delta_g2_neg_pc,
        &public_proof_inputs,
        &proof_points,
    ) {
        Ok(res) => {
            if res {
                Ok(NativeResult::ok(cost, smallvec![Value::bool(true)]))
            } else {
                Ok(NativeResult::ok(cost, smallvec![Value::bool(false)]))
            }
        }
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}
