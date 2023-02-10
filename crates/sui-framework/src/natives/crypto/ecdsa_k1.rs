// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use fastcrypto::{
    secp256k1::{
        recoverable::{Secp256k1RecoverablePublicKey, Secp256k1RecoverableSignature},
        Secp256k1PublicKey, Secp256k1Signature,
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
use sui_types::error::SuiError;

pub const FAIL_TO_RECOVER_PUBKEY: u64 = 0;
pub const INVALID_SIGNATURE: u64 = 1;
pub const INVALID_PUBKEY: u64 = 2;

pub fn ecrecover(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let hashed_msg = pop_arg!(args, VectorRef);
    let signature = pop_arg!(args, VectorRef);

    let hashed_msg_ref = hashed_msg.as_bytes_ref();
    let signature_ref = signature.as_bytes_ref();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();
    match recover_pubkey(&signature_ref, &hashed_msg_ref) {
        Ok(pubkey) => Ok(NativeResult::ok(
            cost,
            smallvec![Value::vector_u8(pubkey.as_bytes().to_vec())],
        )),
        Err(SuiError::InvalidSignature { error: _ }) => {
            Ok(NativeResult::err(cost, INVALID_SIGNATURE))
        }
        Err(_) => Ok(NativeResult::err(cost, FAIL_TO_RECOVER_PUBKEY)),
    }
}

fn recover_pubkey(
    signature: &[u8],
    hashed_msg: &[u8],
) -> Result<Secp256k1RecoverablePublicKey, SuiError> {
    match <Secp256k1RecoverableSignature as ToFromBytes>::from_bytes(signature) {
        Ok(signature) => match signature.recover_hashed(hashed_msg) {
            Ok(pubkey) => Ok(pubkey),
            Err(e) => Err(SuiError::KeyConversionError(e.to_string())),
        },
        Err(e) => Err(SuiError::InvalidSignature {
            error: e.to_string(),
        }),
    }
}

pub fn decompress_pubkey(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let pubkey = pop_arg!(args, VectorRef);
    let pubkey_ref = pubkey.as_bytes_ref();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    match Secp256k1PublicKey::from_bytes(&pubkey_ref) {
        Ok(pubkey) => {
            let uncompressed = &pubkey.pubkey.serialize_uncompressed();
            Ok(NativeResult::ok(
                cost,
                smallvec![Value::vector_u8(uncompressed.to_vec())],
            ))
        }
        Err(_) => Ok(NativeResult::err(cost, INVALID_PUBKEY)),
    }
}

pub fn secp256k1_verify(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let hashed_msg = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let signature_bytes = pop_arg!(args, VectorRef);

    let hashed_msg_ref = hashed_msg.as_bytes_ref();
    let public_key_bytes_ref = public_key_bytes.as_bytes_ref();
    let signature_bytes_ref = signature_bytes.as_bytes_ref();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/4086
    let cost = legacy_empty_cost();

    let signature = match <Secp256k1Signature as ToFromBytes>::from_bytes(&signature_bytes_ref) {
        Ok(signature) => signature,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let public_key = match <Secp256k1PublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
        Ok(public_key) => public_key,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let result = public_key
        .verify_hashed(&hashed_msg_ref, &signature)
        .is_ok();
    Ok(NativeResult::ok(cost, smallvec![Value::bool(result)]))
}

pub fn secp256k1_verify_recoverable(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let hashed_msg = pop_arg!(args, VectorRef);
    let public_key_bytes = pop_arg!(args, VectorRef);
    let signature_bytes = pop_arg!(args, VectorRef);

    let hashed_msg_ref = hashed_msg.as_bytes_ref();
    let public_key_bytes_ref = public_key_bytes.as_bytes_ref();
    let signature_bytes_ref = signature_bytes.as_bytes_ref();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/4086
    let cost = legacy_empty_cost();

    let signature =
        match <Secp256k1RecoverableSignature as ToFromBytes>::from_bytes(&signature_bytes_ref) {
            Ok(signature) => signature,
            Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        };

    let public_key =
        match <Secp256k1RecoverablePublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
            Ok(public_key) => public_key,
            Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
        };

    let result = public_key
        .verify_hashed(&hashed_msg_ref, &signature)
        .is_ok();
    Ok(NativeResult::ok(cost, smallvec![Value::bool(result)]))
}
