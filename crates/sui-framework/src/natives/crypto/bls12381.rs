// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use fastcrypto::{
    bls12381::{min_pk, min_sig},
    traits::{ToFromBytes, VerifyingKey},
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

pub fn bls12381_min_sig_verify(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let msg = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let signature_bytes = pop_arg!(args, VectorRef);

    let msg_ref = msg.as_bytes_ref();
    let public_key_bytes_ref = public_key_bytes.as_bytes_ref();
    let signature_bytes_ref = signature_bytes.as_bytes_ref();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3868
    let cost = legacy_empty_cost();

    let signature =
        match <min_sig::BLS12381Signature as ToFromBytes>::from_bytes(&signature_bytes_ref) {
            Ok(signature) => signature,
            Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        };

    let public_key =
        match <min_sig::BLS12381PublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
            Ok(public_key) => public_key,
            Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        };

    match public_key.verify(&msg_ref, &signature) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}

pub fn bls12381_min_pk_verify(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let msg = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let signature_bytes = pop_arg!(args, VectorRef);

    let msg_ref = msg.as_bytes_ref();
    let public_key_bytes_ref = public_key_bytes.as_bytes_ref();
    let signature_bytes_ref = signature_bytes.as_bytes_ref();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3868
    let cost = legacy_empty_cost();

    let signature =
        match <min_pk::BLS12381Signature as ToFromBytes>::from_bytes(&signature_bytes_ref) {
            Ok(signature) => signature,
            Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        };

    let public_key =
        match <min_pk::BLS12381PublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
            Ok(public_key) => public_key,
            Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        };

    match public_key.verify(&msg_ref, &signature) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}
