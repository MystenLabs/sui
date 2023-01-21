// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use fastcrypto::{
    bulletproofs::{BulletproofsRangeProof, PedersenCommitment},
    traits::ToFromBytes,
};
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

pub const INVALID_BULLETPROOF: u64 = 0;
pub const INVALID_RISTRETTO_GROUP_ELEMENT: u64 = 1;

/// Using the word "sui" for nothing-up-my-sleeve number guarantees.
pub const BP_DOMAIN: &[u8] = b"sui";

pub fn verify_range_proof(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let bit_length = pop_arg!(args, u64);
    let commitment_bytes = pop_arg!(args, VectorRef);
    let proof_bytes = pop_arg!(args, VectorRef);
    let cost = legacy_empty_cost();

    let commitment_bytes_ref = commitment_bytes.as_bytes_ref();
    let proof_bytes_ref = proof_bytes.as_bytes_ref();

    let Ok(proof) = BulletproofsRangeProof::from_bytes(&proof_bytes_ref) else { return Ok(NativeResult::err(cost, INVALID_BULLETPROOF)); };

    let Ok(commitment) = PedersenCommitment::from_bytes(&commitment_bytes_ref) else { return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT)); };

    match proof.verify_bit_length(&commitment, bit_length as usize, BP_DOMAIN) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        _ => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}
