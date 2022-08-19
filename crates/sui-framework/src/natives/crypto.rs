// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};

use narwhal_crypto::{traits::ToFromBytes, Verifier};
use smallvec::smallvec;
use std::collections::VecDeque;

use crate::{legacy_emit_cost, legacy_empty_cost};

pub const FAIL_TO_RECOVER_PUBKEY: u64 = 0;
pub const INVALID_SIGNATURE: u64 = 1;

/// Native implemention of ecrecover in public Move API, see crypto.move for specifications.
pub fn ecrecover(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let hashed_msg = pop_arg!(args, Vec<u8>);
    let signature = pop_arg!(args, Vec<u8>);
    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();
    match <narwhal_crypto::secp256k1::Secp256k1Signature as ToFromBytes>::from_bytes(&signature) {
        Ok(signature) => match signature.recover(&hashed_msg) {
            Ok(pubkey) => Ok(NativeResult::ok(
                cost,
                smallvec![Value::vector_u8(pubkey.as_bytes().to_vec())],
            )),
            Err(_) => Ok(NativeResult::err(cost, FAIL_TO_RECOVER_PUBKEY)),
        },
        Err(_) => Ok(NativeResult::err(cost, INVALID_SIGNATURE)),
    }
}

/// Native implemention of keccak256 in public Move API, see crypto.move for specifications.
pub fn keccak256(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();
    let msg = pop_arg!(args, Vec<u8>);
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(
            <sha3::Keccak256 as sha3::digest::Digest>::digest(msg)
                .as_slice()
                .to_vec()
        )],
    ))
}

/// Native implemention of bls12381_verify in public Move API, see crypto.move for specifications.
/// Note that this function only works for signatures in G1 and public keys in G2.
pub fn bls12381_verify_g1_sig(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let msg = pop_arg!(args, Vec<u8>);
    let public_key_bytes = pop_arg!(args, Vec<u8>);
    let signature_bytes = pop_arg!(args, Vec<u8>);

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3868
    let cost = legacy_emit_cost();

    let signature = match <narwhal_crypto::bls12381::BLS12381Signature as ToFromBytes>::from_bytes(
        &signature_bytes,
    ) {
        Ok(signature) => signature,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let public_key = match <narwhal_crypto::bls12381::BLS12381PublicKey as ToFromBytes>::from_bytes(
        &public_key_bytes,
    ) {
        Ok(public_key) => public_key,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    match public_key.verify(&msg, &signature) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}
