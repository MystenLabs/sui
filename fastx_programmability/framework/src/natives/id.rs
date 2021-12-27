// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::native_functions::NativeContext;
use move_vm_types::{
    gas_schedule::NativeCostIndex,
    loaded_data::runtime_types::Type,
    natives::function::{native_gas, NativeResult},
    pop_arg,
    values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;

pub fn bytes_to_address(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let addr_bytes = pop_arg!(args, Vec<u8>);
    assert!(addr_bytes.len() == 32);
    // truncate the ID to 16 bytes
    // TODO: truncation not secure. we'll either need to support longer account addresses in Move or do this a different way
    // TODO: fix unwrap
    let addr = AccountAddress::from_bytes(&addr_bytes[0..16]).unwrap();

    // TODO: what should the cost of this be?
    let cost = native_gas(context.cost_table(), NativeCostIndex::CREATE_SIGNER, 0);

    Ok(NativeResult::ok(cost, smallvec![Value::address(addr)]))
}
