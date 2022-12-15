// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::legacy_empty_cost;
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
use fastcrypto_tbls::{mocked_dkg, tbls, tbls::ThresholdBls, types};

pub fn verify_tbls_signature(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 3);

    let sig = pop_arg!(args, VectorRef);
    let msg = pop_arg!(args, VectorRef);
    let epoch = pop_arg!(args, u64);

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    let (pk_bls, _pk_vss) = mocked_dkg::generate_public_keys(1, epoch);
    let sig: types::Signature = bincode::deserialize(&sig.as_bytes_ref()).unwrap();
    let valid = types::ThresholdBls12381MinSig::verify(&pk_bls, &msg.as_bytes_ref(), &sig).is_ok();

    match valid {
        true => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        _ => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }
}

// Used only in tests.
pub fn tbls_sign(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let msg = pop_arg!(args, VectorRef);
    let epoch = pop_arg!(args, u64);

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    let (sk, _pk) = mocked_dkg::generate_full_key_pair(epoch);
    let sig = types::ThresholdBls12381MinSig::sign(&sk, &msg.as_bytes_ref());
    let sig = bincode::serialize(&sig).unwrap();

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(sig)],
    ))
}
