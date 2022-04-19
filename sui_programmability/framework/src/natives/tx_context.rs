// Copyright (c) 2022, Mysten Labs, Inc.
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
use std::{collections::VecDeque, convert::TryFrom};
use sui_types::base_types::TransactionDigest;

pub fn derive_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let ids_created = pop_arg!(args, u64);
    let tx_hash = pop_arg!(args, Vec<u8>);

    // TODO(https://github.com/MystenLabs/sui/issues/58): finalize digest format
    // unwrap safe because all digests in Move are serialized from the Rust `TransactionDigest`
    let digest = TransactionDigest::try_from(tx_hash.as_slice()).unwrap();
    let id = Value::address(AccountAddress::from(digest.derive_id(ids_created)));

    // TODO: choose cost
    let cost = native_gas(context.cost_table(), NativeCostIndex::CREATE_SIGNER, 0);

    Ok(NativeResult::ok(cost, smallvec![id]))
}

/// Create a new signer (for test only) from an address.
pub fn new_signer_from_address(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert_eq!(args.len(), 1);

    let address = pop_arg!(args, AccountAddress);
    let signer = Value::signer(address);

    // Gas amount doesn't matter as this is test only.
    let cost = native_gas(context.cost_table(), NativeCostIndex::EMIT_EVENT, 0);
    Ok(NativeResult::ok(cost, smallvec![signer]))
}
