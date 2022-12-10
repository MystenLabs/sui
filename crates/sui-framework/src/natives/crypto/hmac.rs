// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::legacy_empty_cost;
use fastcrypto::{hmac, traits::ToFromBytes};
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

pub fn hmac_sha3_256(
    _context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let message = pop_arg!(args, VectorRef);
    let key = pop_arg!(args, VectorRef);
    let hmac_key = hmac::HmacKey::from_bytes(&key.as_bytes_ref()).unwrap();

    // TODO: implement native gas cost estimation https://github.com/MystenLabs/sui/issues/3593
    let cost = legacy_empty_cost();

    Ok(NativeResult::ok(
        cost,
        smallvec![Value::vector_u8(
            hmac::hmac_sha3_256(&hmac_key, &message.as_bytes_ref()).to_vec()
        )],
    ))
}
