// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use curve25519_dalek_ng::scalar::Scalar;
use fastcrypto::{bulletproofs::PedersenCommitment, traits::ToFromBytes};
use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub const INVALID_RISTRETTO_GROUP_ELEMENT: u64 = 1;
pub const INVALID_RISTRETTO_SCALAR: u64 = 2;

/// Native implementations for Pedersen Commitment on a prime order group.

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

    let Ok(rist_point_a) = PedersenCommitment::from_bytes(&point_a[..]) else { return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT)); };
    let Ok(rist_point_b) = PedersenCommitment::from_bytes(&point_b[..]) else { return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT)); };

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

    let Ok(rist_point_a) = PedersenCommitment::from_bytes(&point_a[..]) else { return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT)); };
    let Ok(rist_point_b) = PedersenCommitment::from_bytes(&point_b[..]) else { return Ok(NativeResult::err(cost, INVALID_RISTRETTO_GROUP_ELEMENT)); };

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

    let Ok(blinding_factor) = blinding_factor_vec.try_into() else { return Ok(NativeResult::err(cost, INVALID_RISTRETTO_SCALAR)); };

    let Ok(value) = value_vec.try_into() else { return Ok(NativeResult::err(cost, INVALID_RISTRETTO_SCALAR)); };

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

    let Ok(value) = value.try_into() else { return Ok(NativeResult::err(cost, INVALID_RISTRETTO_SCALAR)); };

    let Some(scalar) = Scalar::from_canonical_bytes(value) else { return Ok(NativeResult::err(cost, INVALID_RISTRETTO_SCALAR)); };

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(scalar.as_bytes().to_vec())],
    ))
}
