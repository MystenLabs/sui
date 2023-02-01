// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use fastcrypto::hash::{Blake2b256, HashFunction, Keccak256};
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

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(
            Keccak256::digest(msg.as_bytes_ref().as_slice()).to_vec()
        )],
    ))
}

pub fn blake2b256(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();
    let msg = pop_arg!(args, VectorRef);

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(
            Blake2b256::digest(msg.as_bytes_ref().as_slice()).to_vec()
        )],
    ))
}
