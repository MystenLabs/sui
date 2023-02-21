// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use fastcrypto::error::FastCryptoError;
use fastcrypto::hash::{Keccak256, Sha256};
use fastcrypto::traits::RecoverableSignature;
use fastcrypto::{
    secp256r1::{
        recoverable::Secp256r1RecoverableSignature, Secp256r1PublicKey, Secp256r1Signature,
    },
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

pub const FAIL_TO_RECOVER_PUBKEY: u64 = 0;
pub const INVALID_SIGNATURE: u64 = 1;

pub const KECCAK256: u8 = 0;
pub const SHA256: u8 = 1;

pub fn ecrecover(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let hash = pop_arg!(args, u8);
    let msg = pop_arg!(args, VectorRef);
    let signature = pop_arg!(args, VectorRef);

    let msg_ref = msg.as_bytes_ref();
    let signature_ref = signature.as_bytes_ref();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    let sig = match <Secp256r1RecoverableSignature as ToFromBytes>::from_bytes(&signature_ref) {
        Ok(s) => s,
        Err(_) => return Ok(NativeResult::err(cost, INVALID_SIGNATURE)),
    };

    let pk = match hash {
        KECCAK256 => sig.recover_with_hash::<Keccak256>(&msg_ref),
        SHA256 => sig.recover_with_hash::<Sha256>(&msg_ref),
        _ => Err(FastCryptoError::InvalidInput),
    };

    match pk {
        Ok(pk) => Ok(NativeResult::ok(
            cost,
            smallvec![Value::vector_u8(pk.as_bytes().to_vec())],
        )),
        Err(_) => Ok(NativeResult::err(cost, FAIL_TO_RECOVER_PUBKEY)),
    }
}

pub fn secp256r1_verify(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 4);

    let hash = pop_arg!(args, u8);
    let msg = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let signature_bytes = pop_arg!(args, VectorRef);

    let msg_ref = msg.as_bytes_ref();
    let public_key_bytes_ref = public_key_bytes.as_bytes_ref();
    let signature_bytes_ref = signature_bytes.as_bytes_ref();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/4086
    let cost = legacy_empty_cost();

    let sig = match <Secp256r1Signature as ToFromBytes>::from_bytes(&signature_bytes_ref) {
        Ok(s) => s,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let pk = match <Secp256r1PublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
        Ok(p) => p,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let result = match hash {
        KECCAK256 => pk.verify_with_hash::<Keccak256>(&msg_ref, &sig).is_ok(),
        SHA256 => pk.verify_with_hash::<Sha256>(&msg_ref, &sig).is_ok(),
        _ => false,
    };

    Ok(NativeResult::ok(cost, smallvec![Value::bool(result)]))
}
