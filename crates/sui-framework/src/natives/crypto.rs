// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use move_binary_format::errors::PartialVMResult;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    gas_schedule::NativeCostIndex,
    loaded_data::runtime_types::Type,
    natives::function::{native_gas, NativeResult},
    pop_arg,
    values::Value,
};
use narwhal_crypto::traits::ToFromBytes;
use smallvec::smallvec;
use std::collections::VecDeque;

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
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
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
