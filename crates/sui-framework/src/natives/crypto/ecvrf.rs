// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use fastcrypto::vrf::ecvrf::ECVRFProof;
use fastcrypto::vrf::VRFProof;
use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::native_functions::NativeContext;
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

pub fn ecvrf_verify(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 4);

    let proof_bytes = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let alpha_string = pop_arg!(args, VectorRef);
    let hash_bytes = pop_arg!(args, VectorRef);

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    let hash: [u8; 64] = match hash_bytes.as_bytes_ref().as_slice().try_into() {
        Ok(h) => h,
        Err(_) => return Ok(NativeResult::err(cost, INVALID_ECVRF_HASH_LENGTH)),
    };

    let public_key = match bincode::deserialize(public_key_bytes.as_bytes_ref().as_slice()) {
        Ok(pk) => pk,
        Err(_) => return Ok(NativeResult::err(cost, INVALID_ECVRF_PUBLIC_KEY)),
    };

    let proof: ECVRFProof = match bincode::deserialize(proof_bytes.as_bytes_ref().as_slice()) {
        Ok(p) => p,
        Err(_) => return Ok(NativeResult::err(cost, INVALID_ECVRF_PROOF)),
    };

    let result = proof.verify_output(alpha_string.as_bytes_ref().as_slice(), &public_key, hash);
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::bool(result.is_ok())],
    ))
}
