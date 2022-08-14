// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use move_binary_format::errors::{PartialVMResult, PartialVMError};
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    gas_schedule::NativeCostIndex,
    loaded_data::runtime_types::Type,
    natives::function::{native_gas, NativeResult},
    pop_arg,
    values::Value,
};
use narwhal_crypto::{traits::ToFromBytes, bulletproofs::{BulletproofsRangeProof, PedersenCommitment}};
use smallvec::smallvec;
use std::collections::VecDeque;
use curve25519_dalek_ng::{
    scalar::Scalar
};

pub const FAIL_TO_RECOVER_PUBKEY: u64 = 0;
pub const INVALID_SIGNATURE: u64 = 1;

/// Native implemention of ecrecover in public Move API, see crypto.move for specifications.
pub fn ecrecover(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let hashed_msg = pop_arg!(args, Vec<u8>);
    let signature = pop_arg!(args, Vec<u8>);
    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMPTY, 0);
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
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMPTY, 0);
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

/// Native implemention of bulletproofs proof in public Move API, see crypto.move for specifications.
pub fn verify_full_range_proof(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let commitment_bytes = pop_arg!(args, Vec<u8>);
    let proof_bytes = pop_arg!(args, Vec<u8>);
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMPTY, 0);

    let proof = if let Ok(val) = BulletproofsRangeProof::from_bytes(&proof_bytes[..]) {
        val
    } else {
        return Ok(NativeResult::err(
            cost,
            FAIL_TO_RECOVER_PUBKEY,
        ));
    };

    let commitment = if let Ok(val) = PedersenCommitment::from_bytes(&commitment_bytes[..]) {
        val
    } else {
        return Ok(NativeResult::err(
            cost,
            FAIL_TO_RECOVER_PUBKEY,
        ));
    };

    match proof.verify_bound(&commitment) {
        Ok(_) => Ok(NativeResult::ok(
            cost,
            smallvec![]
        )),
        _ => Ok(NativeResult::err(
            cost,
            FAIL_TO_RECOVER_PUBKEY,
        ))
    }
}

pub fn add_ristretto_point(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let point_a = pop_arg!(args, Vec<u8>);
    let point_b = pop_arg!(args, Vec<u8>);
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMPTY, 0);

    let rist_point_a = if let Ok(val) = PedersenCommitment::from_bytes(&point_a[..]) {
        val
    } else {
        return Ok(
            NativeResult::err(
                cost,
                FAIL_TO_RECOVER_PUBKEY,
            )
        );
    };
    let rist_point_b = if let Ok(val) = PedersenCommitment::from_bytes(&point_b[..]) {
        val
    } else {
        return Ok(
            NativeResult::err(
                cost,
                FAIL_TO_RECOVER_PUBKEY,
            )
        );
    };

    let sum = rist_point_a + rist_point_b;

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(sum.as_bytes().to_vec())],
    ))
}

pub fn subtract_ristretto_point(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let point_b = pop_arg!(args, Vec<u8>);
    let point_a = pop_arg!(args, Vec<u8>);
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMPTY, 0);

    let rist_point_a = if let Ok(val) = PedersenCommitment::from_bytes(&point_a[..]) {
        val
    } else {
        return Ok(
            NativeResult::err(
                cost,
                FAIL_TO_RECOVER_PUBKEY,
            )
        );
    };
    let rist_point_b = if let Ok(val) = PedersenCommitment::from_bytes(&point_b[..]) {
        val
    } else {
        return Ok(
            NativeResult::err(
                cost,
                FAIL_TO_RECOVER_PUBKEY,
            )
        );
    };

    let sum = rist_point_a - rist_point_b;

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(sum.as_bytes().to_vec())],
    ))
}

pub fn pedersen_commit(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let blinding_factor_vec = pop_arg!(args, Vec<u8>);
    let value_vec = pop_arg!(args, Vec<u8>);
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMPTY, 0);    

    let blinding_factor: [u8; 32] = if let Ok(val) = blinding_factor_vec.try_into() {
        val
    } else {
        return Ok(
            NativeResult::err(
                cost,
                FAIL_TO_RECOVER_PUBKEY,
            )
        );
    };

    let value: [u8; 32] = if let Ok(val) = value_vec.try_into() {
        val
    } else {
        return Ok(
            NativeResult::err(
                cost,
                FAIL_TO_RECOVER_PUBKEY,
            )
        );
    };

    let commitment = PedersenCommitment::new(
        value,
        blinding_factor
    );

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(commitment.as_bytes().to_vec())]
    ))
}

pub fn big_scalar_from_u64(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let value = pop_arg!(args, u64);
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMPTY, 0);    

    let scalar = Scalar::from(value);

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(scalar.as_bytes().to_vec())],
    ))
}

#[test]
fn test_range_proof() {
    let secret = 990u64;
    let blinding = 980u64;

    let blinding_vec = Scalar::from(blinding).to_bytes();
    let secret_vec = Scalar::from(secret).to_bytes();

    let (commitment, range_proof) = BulletproofsRangeProof::prove_bound(secret, blinding_vec).unwrap();

    eprintln!("{:?}", commitment.as_ref());
    eprintln!("{:?}", range_proof.as_ref());
}
