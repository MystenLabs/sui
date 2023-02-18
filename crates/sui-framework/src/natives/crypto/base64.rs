// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use base64;
use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

const E_BASE64_DECODE_WITH_WRONG_BTTES: u64 = 1;

pub fn base64_encode(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let bytes = pop_arg!(args, Vec<u8>);
    let cost = legacy_empty_cost();

    let base64_encoded = base64::encode(&bytes);

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(base64_encoded.as_bytes().to_vec())],
    ))
}


pub fn base64_decode(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let bytes = pop_arg!(args, Vec<u8>);

    let cost = legacy_empty_cost();

    let base64_decoded = base64::decode(&bytes);
    if let Ok(base64_decoded) = base64_decoded {
        Ok(NativeResult::ok(
            cost,
            smallvec![Value::vector_u8(base64_decoded)],
        ))
    } else {
        Ok(NativeResult::err(
            cost,
            E_BASE64_DECODE_WITH_WRONG_BTTES,
        ))
    }
}
