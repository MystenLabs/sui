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
use std::ops::Deref;
use fastcrypto::error::FastCryptoError;

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

    // TODO: verify_bls with the mocked generator - currently compares with msg
    let res = match msg.as_bytes_ref().deref() == sig.as_bytes_ref().deref() {
        true => Ok(NativeResult::ok(cost, smallvec![Value::bool(true)])),
        _ => Ok(NativeResult::ok(cost, smallvec![Value::bool(false)])),
    }; res
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

    // TODO: verify_bls with the mocked generator - currently returns msg
    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(msg.as_bytes_ref().deref().clone())],
    ))
}
