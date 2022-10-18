// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{legacy_emit_cost, legacy_empty_cost};
use curve25519_dalek_ng::scalar::Scalar;
use fastcrypto::{
    bls12381::{BLS12381PublicKey, BLS12381Signature},
    bulletproofs::{BulletproofsRangeProof, PedersenCommitment},
    ed25519::{Ed25519PublicKey, Ed25519Signature},
    hmac::{hmac, HmacKey},
    secp256k1::{Secp256k1PublicKey, Secp256k1Signature},
    traits::ToFromBytes,
    Verifier,
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
pub const INVALID_BULLETPROOF: u64 = 2;
pub const INVALID_RISTRETTO_GROUP_ELEMENT: u64 = 3;
pub const INVALID_RISTRETTO_SCALAR: u64 = 4;
pub const BULLETPROOFS_VERIFICATION_FAILED: u64 = 5;
pub const INVALID_PUBKEY: u64 = 6;

/// Using the word "sui" for nothing-up-my-sleeve number guarantees.
pub const BP_DOMAIN: &[u8] = b"sui";

/// Native implementation of ecrecover in public Move API, see crypto.move for specifications.
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

fn recover_pubkey(signature: &[u8], hashed_msg: &[u8]) -> Result<Secp256k1PublicKey, SuiError> {
    match <Secp256k1Signature as ToFromBytes>::from_bytes(signature) {
        Ok(signature) => match signature.recover(hashed_msg) {
            Ok(pubkey) => Ok(pubkey),
            Err(e) => Err(SuiError::KeyConversionError(e.to_string())),
        },
        Err(e) => Err(SuiError::InvalidSignature {
            error: e.to_string(),
        }),
    }
}

/// Convert a compressed 33-bytes Secp256k1 pubkey to an 65-bytes uncompressed one.
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
    let msg = pop_arg!(args, VectorRef);
    let msg_ref = msg.as_bytes_ref();

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(
            <sha3::Keccak256 as sha3::digest::Digest>::digest(&*msg_ref)
                .as_slice()
                .to_vec()
        )],
    ))
}

/// Native implemention of secp256k1_verify in public Move API, see crypto.move for specifications.
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
    let cost = legacy_emit_cost();

    let signature = match <Secp256k1Signature as ToFromBytes>::from_bytes(&signature_bytes_ref) {
        Ok(signature) => signature,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let public_key = match <Secp256k1PublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
        Ok(public_key) => public_key,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    match public_key.verify_hashed(&hashed_msg_ref, &signature) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}

/// Native implementation of bls12381_verify in public Move API, see crypto.move for specifications.
/// Note that this function only works for signatures in G1 and public keys in G2.
pub fn bls12381_verify_g1_sig(
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
    let cost = legacy_emit_cost();

    let signature = match <BLS12381Signature as ToFromBytes>::from_bytes(&signature_bytes_ref) {
        Ok(signature) => signature,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let public_key = match <BLS12381PublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
        Ok(public_key) => public_key,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    match public_key.verify(&msg_ref, &signature) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}

/// Native implementation of Bulletproofs range proof in public Move API, see crypto.move for
/// specifications.
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

    let proof = if let Ok(val) = BulletproofsRangeProof::from_bytes(&proof_bytes_ref) {
        val
    } else {
        return Ok(NativeResult::err(cost, INVALID_BULLETPROOF));
    };

    let commitment = if let Ok(val) = PedersenCommitment::from_bytes(&commitment_bytes_ref) {
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

    let msg = pop_arg!(args, VectorRef);
    let msg_ref = msg.as_bytes_ref();
    let public_key_bytes = pop_arg!(args, VectorRef);
    let public_key_bytes_ref = public_key_bytes.as_bytes_ref();
    let signature_bytes = pop_arg!(args, VectorRef);
    let signature_bytes_ref = signature_bytes.as_bytes_ref();

    let cost = legacy_empty_cost();

    let signature = match <Ed25519Signature as ToFromBytes>::from_bytes(&signature_bytes_ref) {
        Ok(signature) => signature,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    let public_key = match <Ed25519PublicKey as ToFromBytes>::from_bytes(&public_key_bytes_ref) {
        Ok(public_key) => public_key,
        Err(_) => return Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    };

    match public_key.verify(&msg_ref, &signature) {
        Ok(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        Err(_) => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}

/// Native implementation of hmac-sha2-256 in public Move API, see crypto.move for specifications.
pub fn hmac_sha2_256(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let message = pop_arg!(args, VectorRef);
    let key = pop_arg!(args, VectorRef);
    let hmac_key = HmacKey::from_bytes(&key.as_bytes_ref()).unwrap();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(hmac(&hmac_key, &message.as_bytes_ref()).to_vec())],
    ))
}