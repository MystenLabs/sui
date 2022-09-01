// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{legacy_emit_cost, legacy_empty_cost};
use curve25519_dalek_ng::scalar::Scalar;
use fastcrypto::{
    bls12381::{BLS12381PublicKey, BLS12381Signature},
    bulletproofs::{BulletproofsRangeProof, PedersenCommitment},
    ed25519::{Ed25519PublicKey, Ed25519Signature},
    secp256k1::{Secp256k1PublicKey, Secp256k1Signature},
    traits::ToFromBytes,
    Verifier,
};
use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::error::SuiError;

pub const FAIL_TO_RECOVER_PUBKEY: u64 = 0;
pub const INVALID_SIGNATURE: u64 = 1;
pub const INVALID_BULLETPROOF: u64 = 2;
pub const INVALID_RISTRETTO_GROUP_ELEMENT: u64 = 3;
pub const INVALID_RISTRETTO_SCALAR: u64 = 4;
pub const BULLETPROOFS_VERIFICATION_FAILED: u64 = 5;

pub const BP_DOMAIN: &[u8] = b"mizu";

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
    match recover_pubkey(signature, hashed_msg) {
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

fn recover_pubkey(signature: Vec<u8>, hashed_msg: Vec<u8>) -> Result<Secp256k1PublicKey, SuiError> {
    match <Secp256k1Signature as ToFromBytes>::from_bytes(&signature) {
        Ok(signature) => match signature.recover(&hashed_msg) {
            Ok(pubkey) => Ok(pubkey),
            Err(e) => Err(SuiError::KeyConversionError(e.to_string())),
        },
        Err(e) => Err(SuiError::InvalidSignature {
            error: e.to_string(),
        }),
    }
}

/// Ecrecover to eth address directly.
/// Native implemention of ecrecover in public Move API, see crypto.move for specifications.
pub fn ecrecover_eth_address(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let hashed_msg = pop_arg!(args, Vec<u8>);
    let mut signature = pop_arg!(args, Vec<u8>);

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    normalized(&mut signature);
    match recover_pubkey(signature, hashed_msg) {
        Ok(pubkey) => {
            let mut address = [0u8; 20];
            address.copy_from_slice(&hash(&pubkey.pubkey.serialize_uncompressed()[1..])[12..]);
            Ok(NativeResult::ok(cost, smallvec![Value::vector_u8(address)]))
        }
        Err(SuiError::InvalidSignature { error: _ }) => {
            Ok(NativeResult::err(cost, INVALID_SIGNATURE))
        }
        Err(_) => Ok(NativeResult::err(cost, FAIL_TO_RECOVER_PUBKEY)),
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
        smallvec![Value::vector_u8(hash(&msg))],
    ))
}

fn hash(msg: &[u8]) -> Vec<u8> {
    <sha3::Keccak256 as sha3::digest::Digest>::digest(msg)
        .as_slice()
        .to_vec()
}

// Normalize v to 0 or 1 from EIP-155 compliant Ethereum signature.
fn normalized(signature: &mut [u8]) {
    signature[64] = match signature[64] {
        0 => 0,
        1 => 1,
        27 => 0,
        28 => 1,
        v if v >= 35 => ((v - 1) % 2) as _,
        _ => 4,
    }
}
/// Native implemention of secp256k1_verify in public Move API, see crypto.move for specifications.
pub fn secp256k1_verify(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let hashed_msg = pop_arg!(args, Vec<u8>);
    let public_key_bytes = pop_arg!(args, Vec<u8>);
    let signature_bytes = pop_arg!(args, Vec<u8>);

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/4086
    let cost = legacy_emit_cost();

    let signature = match <Secp256k1Signature as ToFromBytes>::from_bytes(&signature_bytes) {
        Ok(signature) => signature,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let public_key = match <Secp256k1PublicKey as ToFromBytes>::from_bytes(&public_key_bytes) {
        Ok(public_key) => public_key,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    match public_key.verify_hashed(&hashed_msg, &signature) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
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

    let signature = match <BLS12381Signature as ToFromBytes>::from_bytes(&signature_bytes) {
        Ok(signature) => signature,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let public_key = match <BLS12381PublicKey as ToFromBytes>::from_bytes(&public_key_bytes) {
        Ok(public_key) => public_key,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    match public_key.verify(&msg, &signature) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}

/// Native implemention of Bulletproofs range proof in public Move API, see crypto.move for specifications.
pub fn verify_range_proof(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let bit_length = pop_arg!(args, u64);
    let commitment_bytes = pop_arg!(args, Vec<u8>);
    let proof_bytes = pop_arg!(args, Vec<u8>);
    let cost = legacy_empty_cost();

    let proof = if let Ok(val) = BulletproofsRangeProof::from_bytes(&proof_bytes[..]) {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_BULLETPROOF));
    };

    let commitment = if let Ok(val) = PedersenCommitment::from_bytes(&commitment_bytes[..]) {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT));
    };

    match proof.verify_bit_length(&commitment, bit_length as usize, BP_DOMAIN) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![])),
        _ => Ok(NativeResult::err(cost, BULLETPROOFS_VERIFICATION_FAILED)),
    }
}

pub fn add_ristretto_point(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let point_a = pop_arg!(args, Vec<u8>);
    let point_b = pop_arg!(args, Vec<u8>);
    let cost = legacy_empty_cost();

    let rist_point_a = if let Ok(val) = PedersenCommitment::from_bytes(&point_a[..]) {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT));
    };
    let rist_point_b = if let Ok(val) = PedersenCommitment::from_bytes(&point_b[..]) {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT));
    };

    let sum = rist_point_a + rist_point_b;

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(sum.as_bytes().to_vec())],
    ))
}

pub fn subtract_ristretto_point(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let point_b = pop_arg!(args, Vec<u8>);
    let point_a = pop_arg!(args, Vec<u8>);
    let cost = legacy_empty_cost();

    let rist_point_a = if let Ok(val) = PedersenCommitment::from_bytes(&point_a[..]) {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT));
    };
    let rist_point_b = if let Ok(val) = PedersenCommitment::from_bytes(&point_b[..]) {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT));
    };

    let sum = rist_point_a - rist_point_b;

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(sum.as_bytes().to_vec())],
    ))
}

pub fn pedersen_commit(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let blinding_factor_vec = pop_arg!(args, Vec<u8>);
    let value_vec = pop_arg!(args, Vec<u8>);
    let cost = legacy_empty_cost();

    let blinding_factor: [u8; 32] = if let Ok(val) = blinding_factor_vec.try_into() {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_RISTRETTO_SCALAR));
    };

    let value: [u8; 32] = if let Ok(val) = value_vec.try_into() {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_RISTRETTO_SCALAR));
    };

    let commitment = PedersenCommitment::new(value, blinding_factor);

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(commitment.as_bytes().to_vec())],
    ))
}

pub fn scalar_from_u64(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let value = pop_arg!(args, u64);
    let cost = legacy_empty_cost();
    let scalar = Scalar::from(value);

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(scalar.as_bytes().to_vec())],
    ))
}

pub fn scalar_from_bytes(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let value = pop_arg!(args, Vec<u8>);
    let cost = legacy_empty_cost();

    let value: [u8; 32] = if let Ok(val) = value.try_into() {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_RISTRETTO_SCALAR));
    };

    let scalar = if let Some(value) = Scalar::from_canonical_bytes(value) {
        value
    } else {
        return Ok(NativeResult::err(cost, INVALID_RISTRETTO_SCALAR));
    };

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(scalar.as_bytes().to_vec())],
    ))
}

/// Native implemention of ed25519_verify in public Move API, see crypto.move for specifications.
pub fn ed25519_verify(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let msg = pop_arg!(args, Vec<u8>);
    let public_key_bytes = pop_arg!(args, Vec<u8>);
    let signature_bytes = pop_arg!(args, Vec<u8>);

    let cost = legacy_empty_cost();

    let signature = match <Ed25519Signature as ToFromBytes>::from_bytes(&signature_bytes) {
        Ok(signature) => signature,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let public_key = match <Ed25519PublicKey as ToFromBytes>::from_bytes(&public_key_bytes) {
        Ok(public_key) => public_key,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    match public_key.verify(&msg, &signature) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}
